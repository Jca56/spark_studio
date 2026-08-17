//! Pipeline tests. Beyond compiling the shader, these render into an
//! offscreen target and read the pixels back, so the material stack is
//! checked against what actually lands on the surface rather than against
//! what the code looks like it should do.

use super::*;

const DIM: u32 = 64;
const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;

/// A headless device, or `None` on a host with no GPU.
fn device() -> Option<(wgpu::Device, wgpu::Queue)> {
    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
    let adapter = spark_render::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        force_fallback_adapter: false,
        compatible_surface: None,
    }))
    .ok()?;
    spark_render::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).ok()
}

/// Builds the real pipeline on a real adapter, so a broken `ui.wgsl`
/// fails here instead of at Alva's next redraw. wgpu panics on
/// uncaptured validation errors, so getting through `UiPass::new` at all
/// is the assertion.
#[test]
fn shader_compiles_on_this_gpu() {
    let Some((device, queue)) = device() else {
        eprintln!("no GPU adapter available — skipping");
        return;
    };
    UiPass::new(&device, &queue, FORMAT, &[0u8; 4], 1);
}

/// Draw the batches into an offscreen target and read the pixels back.
fn render(batches: &[(&[UiRect], Option<Viewport>)]) -> Option<Vec<u8>> {
    let (device, queue) = device()?;
    let mut pass = UiPass::new(&device, &queue, FORMAT, &[0u8; 4], 1);
    let target = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("test target"),
        size: wgpu::Extent3d {
            width: DIM,
            height: DIM,
            depth_or_array_layers: 1,
        },
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
        label: Some("test readback"),
        size: (DIM * DIM * 4) as u64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&Default::default());
    pass.draw_batches(
        &device,
        &queue,
        &mut encoder,
        &view,
        batches,
        (DIM, DIM),
        Some(wgpu::Color::BLACK),
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
        wgpu::Extent3d {
            width: DIM,
            height: DIM,
            depth_or_array_layers: 1,
        },
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

fn px(pixels: &[u8], x: u32, y: u32) -> [u8; 3] {
    let i = ((y * DIM + x) * 4) as usize;
    [pixels[i], pixels[i + 1], pixels[i + 2]]
}

fn rect(x: f32, y: f32, w: f32, h: f32) -> Viewport {
    Viewport { x, y, w, h }
}

/// The load-bearing assumption of the storage-buffer rewrite: a batch is
/// a *range* of instance indices, so the shader must read instance N's
/// data for instance N even when the draw starts partway in. Get this
/// wrong and every batch after the first paints the first batch's
/// material. It also proves the WGSL struct layout matches `repr(C)` —
/// a mismatch would smear colors into the wrong fields.
#[test]
fn batches_index_their_own_instances() {
    let red = [UiRect::region(
        rect(0.0, 0.0, 32.0, 64.0),
        [1.0, 0.0, 0.0, 1.0],
    )];
    let green = [UiRect::region(
        rect(32.0, 0.0, 32.0, 64.0),
        [0.0, 1.0, 0.0, 1.0],
    )];
    let Some(p) = render(&[(&red, None), (&green, None)]) else {
        eprintln!("no GPU adapter available — skipping");
        return;
    };
    assert_eq!(px(&p, 8, 32), [255, 0, 0], "first batch");
    assert_eq!(
        px(&p, 56, 32),
        [0, 255, 0],
        "second batch reads its own data"
    );
}

/// A real border is exactly `width` px thick and lives *inside* the
/// shape — the whole point of retiring the bigger-rect-behind trick.
#[test]
fn stroke_is_inset_and_exact() {
    let ui = [
        UiRect::region(rect(16.0, 16.0, 32.0, 32.0), [0.0, 0.0, 0.0, 1.0])
            .stroke(4.0, [1.0, 1.0, 1.0, 1.0]),
    ];
    let Some(p) = render(&[(&ui, None)]) else {
        eprintln!("no GPU adapter available — skipping");
        return;
    };
    assert_eq!(px(&p, 32, 32), [0, 0, 0], "fill survives in the middle");
    assert_eq!(px(&p, 18, 32), [255, 255, 255], "2px in is stroke");
    assert_eq!(px(&p, 22, 32), [0, 0, 0], "6px in is past the 4px stroke");
    assert_eq!(px(&p, 14, 32), [0, 0, 0], "outside stays background");
}

/// An outward ring adds to the shape's footprint instead of eating it,
/// which means the vertex stage has to grow the quad to make room.
#[test]
fn outer_stroke_grows_the_quad() {
    let ui = [
        UiRect::region(rect(24.0, 24.0, 16.0, 16.0), [0.0, 0.0, 0.0, 1.0])
            .stroke_outer(4.0, [1.0, 1.0, 1.0, 1.0]),
    ];
    let Some(p) = render(&[(&ui, None)]) else {
        eprintln!("no GPU adapter available — skipping");
        return;
    };
    assert_eq!(px(&p, 32, 32), [0, 0, 0], "fill untouched");
    assert_eq!(
        px(&p, 22, 32),
        [255, 255, 255],
        "ring sits outside the edge"
    );
    assert_eq!(px(&p, 26, 32), [0, 0, 0], "and never crosses into the fill");
}

/// Panels tile edge to edge all over the chrome; if sharp fills took the
/// antialiasing ramp, every shared seam would show a half-lit hairline.
#[test]
fn abutting_sharp_fills_leave_no_seam() {
    let ui = [
        UiRect::region(rect(0.0, 0.0, 32.0, 64.0), [1.0, 1.0, 1.0, 1.0]),
        UiRect::region(rect(32.0, 0.0, 32.0, 64.0), [1.0, 1.0, 1.0, 1.0]),
    ];
    let Some(p) = render(&[(&ui, None)]) else {
        eprintln!("no GPU adapter available — skipping");
        return;
    };
    for x in 29..35 {
        assert_eq!(px(&p, x, 32), [255, 255, 255], "no seam at x={x}");
    }
}

/// The drop shadow has to land outside the shape and fade with distance.
#[test]
fn shadow_falls_outside_and_fades() {
    let ui = [
        UiRect::region(rect(16.0, 16.0, 32.0, 32.0), [1.0, 1.0, 1.0, 1.0]).shadow(
            [0.0, 0.0],
            10.0,
            0.0,
            [1.0, 1.0, 1.0, 1.0],
        ),
    ];
    let Some(p) = render(&[(&ui, None)]) else {
        eprintln!("no GPU adapter available — skipping");
        return;
    };
    assert_eq!(px(&p, 32, 32), [255, 255, 255], "shape itself");
    let near = px(&p, 12, 32)[0];
    let far = px(&p, 8, 32)[0];
    assert!(near > 0, "shadow reaches 4px out (got {near})");
    assert!(far < near, "and fades with distance ({far} < {near})");
    assert_eq!(px(&p, 2, 32), [0, 0, 0], "but not past its blur radius");
}
