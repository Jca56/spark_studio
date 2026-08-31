//! The offscreen GPU harness every pixel test renders through: one device
//! for the whole crate, one test at a time on it, a 64px target read back
//! as bytes.
//!
//! The view is set to 1/10 with no offset, so ten canvas units are one pixel
//! and the 64px target looks at the canvas's top-left 640x640. Star density
//! is measured against the canvas width, not the field, so the tests have
//! to work at canvas scale or a small field would come back empty.

use std::sync::{LazyLock, Mutex, MutexGuard};

use super::*;

pub(super) const DIM: u32 = 64;
pub(super) const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;
/// Canvas units per pixel's reciprocal — the view scale the tests render at.
pub(super) const VIEW: f32 = 0.1;
/// Canvas units per test pixel.
pub(super) const UNIT: f32 = 1.0 / VIEW;

/// One device for every pixel test in the crate — a dozen simultaneous wgpu
/// instances is how you segfault a driver.
static GPU: LazyLock<Option<(wgpu::Device, wgpu::Queue)>> = LazyLock::new(|| {
    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
    let adapter = crate::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        force_fallback_adapter: false,
        compatible_surface: None,
    }))
    .ok()?;
    crate::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).ok()
});

static ONE_AT_A_TIME: Mutex<()> = Mutex::new(());

pub(super) fn device() -> Option<(&'static wgpu::Device, &'static wgpu::Queue)> {
    GPU.as_ref().map(|(d, q)| (d, q))
}

pub(super) fn exclusive() -> MutexGuard<'static, ()> {
    ONE_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner())
}

/// Draw `shapes` on the canvas plane at playhead `time`, through the
/// test view, and read the pixels back.
pub(super) fn render(shapes: &[Shape], time: f32) -> Option<Vec<u8>> {
    render_scene(shapes, &[], (VIEW, 0.0, 0.0), time)
}

/// Draw a scene — `models` placing each shape, `cview` the canvas→px
/// map — through the stage camera, and read the pixels back.
pub(super) fn render_scene(
    shapes: &[Shape],
    models: &[Mat4],
    cview: (f32, f32, f32),
    time: f32,
) -> Option<Vec<u8>> {
    let (device, queue) = device()?;
    let _held = exclusive();
    let mut pass = ShapePass::new(device, FORMAT);
    let size = wgpu::Extent3d {
        width: DIM,
        height: DIM,
        depth_or_array_layers: 1,
    };
    let target = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("shape test target"),
        size,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = target.create_view(&wgpu::TextureViewDescriptor::default());
    // DIM * 4 == 256, already the required row alignment.
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("shape test readback"),
        size: (DIM * DIM * 4) as u64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&Default::default());
    // The shape pass loads rather than clears (the backdrop pass paints the
    // base coat in the real app), so lay down black first.
    encoder
        .begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("shape test clear"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            ..Default::default()
        })
        .forget_lifetime();
    let camera = Camera::stage();
    pass.draw(
        device,
        queue,
        &mut encoder,
        &view,
        &Scene {
            shapes,
            models,
            paths: &[],
            camera: &camera,
            time,
        },
        (DIM, DIM),
        cview,
        Viewport {
            x: 0.0,
            y: 0.0,
            w: DIM as f32,
            h: DIM as f32,
        },
    );
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture: &target,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &readback,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(DIM * 4),
                rows_per_image: Some(DIM),
            },
        },
        size,
    );
    queue.submit([encoder.finish()]);
    readback.slice(..).map_async(wgpu::MapMode::Read, |_| {});
    device
        .poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: None,
        })
        .expect("poll");
    let pixels = readback.slice(..).get_mapped_range().to_vec();
    readback.unmap();
    Some(pixels)
}
