//! Stage-cache tests: the cached picture has to be the live picture, and
//! the cache has to know when it is stale. Same offscreen readback as
//! `tests.rs`, with the stage in the loop.

use super::harness::{DIM, FORMAT, UNIT, VIEW, device, exclusive, render};
use super::tests::field;
use super::*;

/// Render through a stage onto a black target, once per `(playhead,
/// preview)` round, reading back after the last. Returns the pixels and
/// whether each round re-ran the shape pass.
fn render_staged(shapes: &[Shape], rounds: &[(f32, bool)]) -> Option<(Vec<u8>, Vec<bool>)> {
    render_staged_scene(shapes, &[], (VIEW, 0.0, 0.0), rounds)
}

/// The same, with each shape placed by a model matrix and the canvas→px
/// map chosen by the test.
pub(super) fn render_staged_scene(
    shapes: &[Shape],
    models: &[Mat4],
    cview: (f32, f32, f32),
    rounds: &[(f32, bool)],
) -> Option<(Vec<u8>, Vec<bool>)> {
    let (device, queue) = device()?;
    let _held = exclusive();
    let mut pass = ShapePass::new(device, FORMAT);
    let mut stage = Stage::new(device, queue, FORMAT);
    let size = wgpu::Extent3d {
        width: DIM,
        height: DIM,
        depth_or_array_layers: 1,
    };
    let target = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("stage test target"),
        size,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = target.create_view(&wgpu::TextureViewDescriptor::default());
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("stage test readback"),
        size: (DIM * DIM * 4) as u64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let clip = Viewport {
        x: 0.0,
        y: 0.0,
        w: DIM as f32,
        h: DIM as f32,
    };
    let mut fresh = Vec::new();
    let mut encoder = device.create_command_encoder(&Default::default());
    for &(t, preview) in rounds {
        // Every round starts from black, like the backdrop pass does, so a
        // cache hit has to bring the whole picture back by itself.
        encoder
            .begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("stage test clear"),
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
        fresh.push(stage.draw(
            device,
            queue,
            &mut encoder,
            &view,
            &mut pass,
            &Scene {
                shapes,
                models,
                paths: &[],
            meshes: &[],
            lights: &[],
                camera: &camera,
                time: t,
            },
            (DIM, DIM),
            cview,
            clip,
            preview,
        ));
    }
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
    Some((pixels, fresh))
}

/// A stack that exercises every compositing case at once: a solid body
/// with a halo narrow enough to stay with it, a half-faded shape over it,
/// an additive one, and a star field — premultiplied-over has to come out
/// the same whether it runs against the backdrop or against a transparent
/// stage.
fn stack() -> Vec<Shape> {
    // Glow 40 canvas units is 4 px at the test view: under SMALL_HALO_PX,
    // so the body pass keeps it and the picture must match exactly.
    let mut glow = Shape::circle([24.0 * UNIT, 24.0 * UNIT], 10.0 * UNIT)
        .color(1.0, 0.2, 0.8)
        .intensity(1.4);
    glow.set_glow(4.0 * UNIT);
    let mut faded = Shape::rect([36.0 * UNIT, 30.0 * UNIT], [12.0 * UNIT, 8.0 * UNIT])
        .color(0.1, 0.9, 0.3)
        .intensity(1.0);
    faded.set_opacity(0.5);
    let mut add = Shape::ngon([40.0 * UNIT, 44.0 * UNIT], 9.0 * UNIT, 6)
        .color(0.2, 0.4, 1.0)
        .intensity(0.8);
    add.set_additive(true);
    vec![field(5.0), glow, faded, add]
}

/// One shape with a halo wide enough to go to the halo layer: 12 px of
/// glow at the test view, reaching 36 px.
fn big_glow() -> Shape {
    let mut s = Shape::circle([32.0 * UNIT, 32.0 * UNIT], 8.0 * UNIT)
        .color(1.0, 1.0, 1.0)
        .intensity(1.0);
    s.set_glow(12.0 * UNIT);
    s
}

fn total(pixels: &[u8]) -> u64 {
    pixels.iter().map(|&v| v as u64).sum()
}

/// Worst per-channel difference between two readbacks.
fn max_diff(a: &[u8], b: &[u8]) -> u8 {
    a.iter()
        .zip(b)
        .map(|(x, y)| x.abs_diff(*y))
        .max()
        .unwrap_or(0)
}

/// The stage blit reproduces the live frame. The stage is stored in the
/// same 8-bit sRGB the frame is, so an extra quantisation step is allowed
/// one count of rounding and no more.
#[test]
fn the_stage_is_the_live_frame() {
    let shapes = stack();
    let (Some(live), Some((staged, fresh))) = (
        render(&shapes, 0.0),
        render_staged(&shapes, &[(0.0, false)]),
    ) else {
        eprintln!("no GPU adapter available — skipping");
        return;
    };
    assert_eq!(fresh, [true], "the first draw has to render");
    let total: u32 = live.iter().map(|&v| v as u32).sum();
    assert!(total > 0, "the live frame is black — nothing to compare");
    assert!(
        max_diff(&live, &staged) <= 1,
        "staged frame differs from the live one by {} counts",
        max_diff(&live, &staged)
    );
}

/// Unchanged inputs are a hit — and a hit still paints the whole picture,
/// since the frame underneath was cleared.
#[test]
fn a_repeat_frame_is_a_hit_that_still_paints() {
    let shapes = stack();
    let (Some(live), Some((staged, fresh))) = (
        render(&shapes, 0.0),
        render_staged(&shapes, &[(0.0, false), (0.0, false), (0.0, false)]),
    ) else {
        eprintln!("no GPU adapter available — skipping");
        return;
    };
    assert_eq!(fresh, [true, false, false]);
    assert!(max_diff(&live, &staged) <= 1, "a hit lost the picture");
}

/// The playhead is an input: a field twinkles on it, so a new time has to
/// miss, and the picture has to be the new time's.
#[test]
fn a_new_playhead_misses() {
    let mut f = field(9.0);
    f.set_twinkle(1.0);
    f.set_twinkle_rate(6.0);
    let shapes = vec![f];
    let (Some(later), Some((staged, fresh))) = (
        render(&shapes, 1.3),
        render_staged(&shapes, &[(0.0, false), (1.3, false)]),
    ) else {
        eprintln!("no GPU adapter available — skipping");
        return;
    };
    assert_eq!(fresh, [true, true]);
    assert!(max_diff(&later, &staged) <= 1, "a miss drew the old time");
}

/// A changed shape misses; the same shape list in a fresh allocation does
/// not — the key is by value, never by identity.
#[test]
fn shapes_are_keyed_by_value() {
    let Some((device, queue)) = device() else {
        eprintln!("no GPU adapter available — skipping");
        return;
    };
    let _held = exclusive();
    let mut pass = ShapePass::new(device, FORMAT);
    let mut stage = Stage::new(device, queue, FORMAT);
    let target = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("stage key target"),
        size: wgpu::Extent3d {
            width: DIM,
            height: DIM,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let view = target.create_view(&Default::default());
    let clip = Viewport {
        x: 0.0,
        y: 0.0,
        w: DIM as f32,
        h: DIM as f32,
    };
    let camera = Camera::stage();
    let mut draw = |shapes: &[Shape], res: (u32, u32)| {
        let mut encoder = device.create_command_encoder(&Default::default());
        let fresh = stage.draw(
            device,
            queue,
            &mut encoder,
            &view,
            &mut pass,
            &Scene {
                shapes,
                models: &[],
                paths: &[],
            meshes: &[],
            lights: &[],
                camera: &camera,
                time: 0.0,
            },
            res,
            (VIEW, 0.0, 0.0),
            clip,
            false,
        );
        queue.submit([encoder.finish()]);
        fresh
    };
    let a = stack();
    assert!(draw(&a, (DIM, DIM)), "first draw renders");
    assert!(!draw(&a.clone(), (DIM, DIM)), "an equal copy is a hit");
    let mut b = a.clone();
    b[1].set_glow(1.0);
    assert!(draw(&b, (DIM, DIM)), "a nudged glow misses");
    assert!(!draw(&b, (DIM, DIM)), "and then holds");
    assert!(draw(&b, (DIM / 2, DIM / 2)), "a resize misses");
    // An empty paint rect (clip off the frame) still keys and never panics.
    let mut encoder = device.create_command_encoder(&Default::default());
    stage.draw(
        device,
        queue,
        &mut encoder,
        &view,
        &mut pass,
        &Scene {
            shapes: &b,
            models: &[],
            paths: &[],
            meshes: &[],
            lights: &[],
            camera: &camera,
            time: 0.0,
        },
        (DIM, DIM),
        (VIEW, 0.0, 0.0),
        Viewport {
            x: 500.0,
            y: 500.0,
            w: 10.0,
            h: 10.0,
        },
        false,
    );
    queue.submit([encoder.finish()]);
}

/// A wide halo goes through the half-resolution layer. It is the same
/// falloff sampled coarser, so it cannot match a live frame pixel for
/// pixel — but it has to carry the same light, and the body under it
/// has to be the body: a crisp full-resolution fill at its own colour.
#[test]
fn a_wide_halo_keeps_its_light_and_its_body() {
    let shapes = vec![big_glow()];
    let (Some(live), Some((staged, fresh))) = (
        render(&shapes, 0.0),
        render_staged(&shapes, &[(0.0, false)]),
    ) else {
        eprintln!("no GPU adapter available — skipping");
        return;
    };
    assert_eq!(fresh, [true]);
    let (a, b) = (total(&live), total(&staged));
    let ratio = b as f64 / a as f64;
    assert!(
        (0.9..=1.1).contains(&ratio),
        "staged light is {ratio:.3}x the live frame's"
    );
    // The body centre is the picked colour, untouched by the split.
    assert_eq!(
        super::tests::px(&staged, 32, 32),
        super::tests::px(&live, 32, 32)
    );
    // And there is halo well outside the body, where only the halo layer
    // could have put it (body edge is at 40; 46 is 6 px out, half a radius).
    assert!(
        super::tests::px(&staged, 46, 32)[0] > 0,
        "the halo layer drew nothing"
    );
}

/// The deliberate change: a halo now lies over bodies in front of it. Live
/// rendering hides a back shape's halo under a front shape's fill; the
/// stage adds the halo layer over every body, the way bloom spills over
/// what is in front of it. This test holds that on purpose — if the
/// compositing is ever made exact again, this is the assertion to flip.
#[test]
fn a_halo_now_spills_over_what_is_in_front() {
    let mut back = big_glow();
    back.set_rgb([1.0, 0.0, 0.0]);
    // A black fill drawn in front, sitting inside the halo's reach.
    let front = Shape::rect([48.0 * UNIT, 32.0 * UNIT], [6.0 * UNIT, 10.0 * UNIT])
        .color(0.0, 0.0, 0.0)
        .intensity(1.0);
    let shapes = vec![back, front];
    let (Some(live), Some((staged, _))) = (
        render(&shapes, 0.0),
        render_staged(&shapes, &[(0.0, false)]),
    ) else {
        eprintln!("no GPU adapter available — skipping");
        return;
    };
    // Inside the front fill, two px past the back shape's edge.
    let (l, s) = (
        super::tests::px(&live, 44, 32),
        super::tests::px(&staged, 44, 32),
    );
    assert_eq!(l, [0, 0, 0], "live: the front fill hides the halo");
    assert!(
        s[0] > 0 && s[1] == 0,
        "staged: the halo spills over the front fill, got {s:?}"
    );
}

/// Half-resolution playback is its own key — toggling it misses — and
/// its picture is the same picture, softer: same light within a few
/// percent, same body colour at the centre of a fill.
#[test]
fn preview_rerenders_and_keeps_the_picture() {
    // Flat fills only: a star field is a different scatter at half
    // resolution and would make a single-pixel comparison meaningless.
    let mut faded = Shape::rect([20.0 * UNIT, 20.0 * UNIT], [10.0 * UNIT, 8.0 * UNIT])
        .color(0.1, 0.9, 0.3)
        .intensity(1.0);
    faded.set_opacity(0.5);
    let mut add = Shape::ngon([44.0 * UNIT, 44.0 * UNIT], 9.0 * UNIT, 6)
        .color(0.2, 0.4, 1.0)
        .intensity(0.8);
    add.set_additive(true);
    let shapes = vec![faded, add];
    let (Some(live), Some((staged, fresh))) = (
        render(&shapes, 0.0),
        render_staged(&shapes, &[(0.0, false), (0.0, true), (0.0, true)]),
    ) else {
        eprintln!("no GPU adapter available — skipping");
        return;
    };
    assert_eq!(
        fresh,
        [true, true, false],
        "preview on is a miss, then a hit"
    );
    let ratio = total(&staged) as f64 / total(&live) as f64;
    assert!(
        (0.85..=1.15).contains(&ratio),
        "preview light is {ratio:.3}x full-res"
    );
    // The centre of the faded rect — a flat fill — is the same either way.
    let (l, s) = (
        super::tests::px(&live, 20, 20),
        super::tests::px(&staged, 20, 20),
    );
    for c in 0..3 {
        assert!(
            l[c].abs_diff(s[c]) <= 2,
            "preview fill differs at channel {c}: {l:?} vs {s:?}"
        );
    }
}
