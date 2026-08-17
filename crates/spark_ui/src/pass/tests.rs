//! Pipeline tests. Beyond compiling the shader, these render into an
//! offscreen target and read the pixels back, so the material stack is
//! checked against what actually lands on the surface rather than against
//! what the code looks like it should do.

use std::sync::{LazyLock, Mutex, MutexGuard};

use super::*;

const DIM: u32 = 64;
const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;

/// One device for every test in this file.
///
/// Each test used to build its own wgpu instance and device, and with a
/// dozen of them starting at once that segfaulted the driver roughly one run
/// in four. Sharing costs nothing and is faster besides.
static GPU: LazyLock<Option<(wgpu::Device, wgpu::Queue)>> = LazyLock::new(|| {
    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
    let adapter = spark_render::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        force_fallback_adapter: false,
        compatible_surface: None,
    }))
    .ok()?;
    spark_render::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).ok()
});

/// Serializes submit-and-map, which nothing here needs to do concurrently.
/// These tests exist to pin down pixels, so determinism beats parallelism.
static ONE_AT_A_TIME: Mutex<()> = Mutex::new(());

/// The shared device, or `None` on a host with no GPU.
fn device() -> Option<(&'static wgpu::Device, &'static wgpu::Queue)> {
    GPU.as_ref().map(|(d, q)| (d, q))
}

fn exclusive() -> MutexGuard<'static, ()> {
    ONE_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner())
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
    let _held = exclusive();
    UiPass::new(device, queue, FORMAT, &[255u8; 4], 1);
}

/// Draw the batches into an offscreen target and read the pixels back.
fn render(batches: &[(&[UiRect], Option<Viewport>)]) -> Option<Vec<u8>> {
    let (device, queue) = device()?;
    let _held = exclusive();
    let mut pass = UiPass::new(device, queue, FORMAT, &[255u8; 4], 1);
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
        device,
        queue,
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

/// Arc angles run clockwise from straight up, the way a knob reads. A
/// quarter sweep covers twelve-to-three o'clock and nothing else.
#[test]
fn arc_sweeps_clockwise_from_the_top() {
    let ui = [UiRect::arc(
        rect(0.0, 0.0, 64.0, 64.0),
        0.0,
        0.25,
        0.4,
        6.0,
        [1.0, 1.0, 1.0, 1.0],
    )];
    let Some(p) = render(&[(&ui, None)]) else {
        eprintln!("no GPU adapter available — skipping");
        return;
    };
    assert_eq!(px(&p, 32, 6), [255, 255, 255], "12 o'clock is the start");
    assert_eq!(px(&p, 57, 32), [255, 255, 255], "3 o'clock is the end");
    assert_eq!(px(&p, 32, 57), [0, 0, 0], "6 o'clock is past the sweep");
    assert_eq!(px(&p, 6, 32), [0, 0, 0], "9 o'clock too");
    assert_eq!(px(&p, 32, 32), [0, 0, 0], "and the middle stays hollow");
}

/// A full ring closes: every cardinal point is band, the middle is not.
#[test]
fn ring_closes_all_the_way_round() {
    let ui = [UiRect::ring(
        rect(0.0, 0.0, 64.0, 64.0),
        0.4,
        6.0,
        [1.0, 1.0, 1.0, 1.0],
    )];
    let Some(p) = render(&[(&ui, None)]) else {
        eprintln!("no GPU adapter available — skipping");
        return;
    };
    for (x, y) in [(32, 6), (57, 32), (32, 57), (6, 32)] {
        assert_eq!(px(&p, x, y), [255, 255, 255], "band at ({x}, {y})");
    }
    assert_eq!(px(&p, 32, 32), [0, 0, 0], "hollow middle");
}

/// Rotation turns the silhouette itself, so a horizontal bar stands up at a
/// quarter turn.
#[test]
fn rotation_turns_the_silhouette() {
    let bar = |turns: f32| {
        [UiRect::icon_sized(
            rect(0.0, 0.0, 64.0, 64.0),
            crate::rect::ICON_MINUS,
            3.0,
            [1.0, 1.0, 1.0, 1.0],
            0.4,
        )
        .rotate(turns)]
    };
    let flat = bar(0.0);
    let turned = bar(0.25);
    let (Some(a), Some(b)) = (render(&[(&flat, None)]), render(&[(&turned, None)])) else {
        eprintln!("no GPU adapter available — skipping");
        return;
    };
    assert_eq!(px(&a, 50, 32), [255, 255, 255], "flat bar runs across");
    assert_eq!(px(&a, 32, 50), [0, 0, 0], "and not down");
    assert_eq!(px(&b, 50, 32), [0, 0, 0], "turned bar stops running across");
    assert_eq!(px(&b, 32, 50), [255, 255, 255], "and runs down instead");
}

/// The chevron points down at rest and a half turn aims it up, which also
/// pins down which way `rotate` spins.
#[test]
fn chevron_points_down_until_turned() {
    let chev = |turns: f32| {
        [
            UiRect::chevron(rect(0.0, 0.0, 64.0, 64.0), 3.0, [1.0, 1.0, 1.0, 1.0], 0.4)
                .rotate(turns),
        ]
    };
    let down = chev(0.0);
    let up = chev(0.5);
    let (Some(a), Some(b)) = (render(&[(&down, None)]), render(&[(&up, None)])) else {
        eprintln!("no GPU adapter available — skipping");
        return;
    };
    assert_eq!(px(&a, 32, 41), [255, 255, 255], "vertex sits low at rest");
    assert_eq!(px(&a, 32, 23), [0, 0, 0], "nothing up top");
    assert_eq!(px(&b, 32, 23), [255, 255, 255], "half a turn lifts it");
    assert_eq!(px(&b, 32, 41), [0, 0, 0], "and empties the bottom");
}

/// A capsule is all edge and no interior, so dashing one breaks the line
/// itself — the only reading of "dashed line" that means anything.
#[test]
fn dashes_break_a_line_into_segments() {
    let ui =
        [UiRect::line([8.0, 32.0], [56.0, 32.0], 6.0, [1.0, 1.0, 1.0, 1.0]).dash(8.0, 8.0, 0.0)];
    let Some(p) = render(&[(&ui, None)]) else {
        eprintln!("no GPU adapter available — skipping");
        return;
    };
    assert_eq!(px(&p, 12, 32), [255, 255, 255], "first dash");
    assert_eq!(px(&p, 20, 32), [0, 0, 0], "first gap");
    assert_eq!(px(&p, 28, 32), [255, 255, 255], "second dash");
    assert_eq!(px(&p, 36, 32), [0, 0, 0], "second gap");
}

/// On a shape with an interior the dashes break the *border* instead,
/// walking its outline from the top-left while the fill shows through every
/// gap. This is the marching-ants primitive: slide the phase per frame.
#[test]
fn dashes_walk_a_border_and_spare_the_fill() {
    let ui = [
        UiRect::region(rect(0.0, 0.0, 64.0, 64.0), [1.0, 0.0, 0.0, 1.0])
            .stroke(4.0, [1.0, 1.0, 1.0, 1.0])
            .dash(8.0, 8.0, 0.0),
    ];
    let Some(p) = render(&[(&ui, None)]) else {
        eprintln!("no GPU adapter available — skipping");
        return;
    };
    assert_eq!(px(&p, 32, 32), [255, 0, 0], "fill is untouched");
    assert_eq!(px(&p, 2, 1), [255, 255, 255], "outline starts on a dash");
    assert_eq!(px(&p, 12, 1), [255, 0, 0], "gap shows the fill through");
    assert_eq!(px(&p, 20, 1), [255, 255, 255], "next dash");
    assert_eq!(px(&p, 28, 1), [255, 0, 0], "next gap");
}

/// Dashes are measured along the outline, so a phase shift slides them
/// around it — one frame of marching ants.
#[test]
fn dash_phase_slides_the_pattern() {
    let ants = |phase: f32| {
        [
            UiRect::region(rect(0.0, 0.0, 64.0, 64.0), [1.0, 0.0, 0.0, 1.0])
                .stroke(4.0, [1.0, 1.0, 1.0, 1.0])
                .dash(8.0, 8.0, phase),
        ]
    };
    let a0 = ants(0.0);
    let a8 = ants(8.0);
    let (Some(a), Some(b)) = (render(&[(&a0, None)]), render(&[(&a8, None)])) else {
        eprintln!("no GPU adapter available — skipping");
        return;
    };
    // Half a period of phase swaps every dash for the gap beside it.
    assert_eq!(px(&a, 2, 1), [255, 255, 255]);
    assert_eq!(px(&b, 2, 1), [255, 0, 0], "dash moved off this pixel");
    assert_eq!(px(&a, 12, 1), [255, 0, 0]);
    assert_eq!(px(&b, 12, 1), [255, 255, 255], "and onto this one");
}

/// The image blit is the one path that ignores the material stack and just
/// samples the bound texture, tinted. It moved to an explicit-LOD sample so
/// it could sit behind a branch, so it needs pinning down.
#[test]
fn image_blit_tints_the_texture() {
    let ui = [UiRect::icon(
        rect(16.0, 16.0, 32.0, 32.0),
        crate::rect::ICON_IMAGE,
        0.0,
        [1.0, 0.0, 0.0, 1.0],
    )];
    let Some(p) = render(&[(&ui, None)]) else {
        eprintln!("no GPU adapter available — skipping");
        return;
    };
    assert_eq!(
        px(&p, 32, 32),
        [255, 0, 0],
        "white texel wearing a red tint"
    );
    assert_eq!(px(&p, 4, 4), [0, 0, 0], "and it stays inside its quad");
}
