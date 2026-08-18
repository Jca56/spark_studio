//! Shape-pipeline tests. Beyond compiling the shader, these render into an
//! offscreen target and read the pixels back — which is the only way anyone
//! here can check a star field at all, since the person who can see the
//! screen isn't the one who can run the tests.
//!
//! The view is set to 1/10 with no offset, so ten canvas units are one pixel
//! and the 64px target looks at the canvas's top-left 640x640. Star density
//! is measured against the canvas width, not the field, so the test has to
//! work at canvas scale or a small field would come back empty.

use std::sync::{LazyLock, Mutex, MutexGuard};

use super::*;

const DIM: u32 = 64;
const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;
/// Canvas units per pixel's reciprocal — the view scale the tests render at.
const VIEW: f32 = 0.1;
/// Canvas units per test pixel.
const UNIT: f32 = 1.0 / VIEW;

/// One device for every test in this file — a dozen simultaneous wgpu
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

fn device() -> Option<(&'static wgpu::Device, &'static wgpu::Queue)> {
    GPU.as_ref().map(|(d, q)| (d, q))
}

fn exclusive() -> MutexGuard<'static, ()> {
    ONE_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner())
}

/// Builds the real pipeline on a real adapter, so a broken `shape.wgsl`
/// fails here rather than at Alva's next redraw. wgpu panics on uncaptured
/// validation errors, so getting through `ShapePass::new` is the assertion.
#[test]
fn shader_compiles_on_this_gpu() {
    let Some((device, _)) = device() else {
        eprintln!("no GPU adapter available — skipping");
        return;
    };
    let _held = exclusive();
    ShapePass::new(device, FORMAT);
}

/// Draw `shapes` at playhead `time` and read the pixels back.
fn render(shapes: &[Shape], time: f32) -> Option<Vec<u8>> {
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
    pass.draw(
        device,
        queue,
        &mut encoder,
        &view,
        shapes,
        &[],
        (DIM, DIM),
        (VIEW, 0.0, 0.0),
        time,
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

fn px(pixels: &[u8], x: u32, y: u32) -> [u8; 3] {
    let i = ((y * DIM + x) * 4) as usize;
    [pixels[i], pixels[i + 1], pixels[i + 2]]
}

/// A linear channel as the target's sRGB byte — what a colour *should* read
/// back as if the shader passed it through untouched.
fn srgb8(linear: f32) -> u8 {
    let s = if linear <= 0.0031308 {
        linear * 12.92
    } else {
        1.055 * linear.powf(1.0 / 2.4) - 0.055
    };
    (s.clamp(0.0, 1.0) * 255.0).round() as u8
}

/// Total light in a box of the frame — the measure every star test uses,
/// since nobody can say where an individual hashed star landed.
fn light_in(pixels: &[u8], x0: u32, y0: u32, x1: u32, y1: u32) -> u32 {
    let mut sum = 0;
    for y in y0..y1 {
        for x in x0..x1 {
            let i = ((y * DIM + x) * 4) as usize;
            sum += pixels[i] as u32 + pixels[i + 1] as u32 + pixels[i + 2] as u32;
        }
    }
    sum
}

/// A field covering pixels 17..47 of the frame, dim-glowed and small-starred
/// so its edge stays crisp enough to assert on — the widest a star's light
/// can reach past the region here is about 7px, well inside the 9px margin
/// the boundary test leaves itself.
fn field(seed: f32) -> Shape {
    let mut s = Shape::stars([32.0 * UNIT, 32.0 * UNIT], [15.0 * UNIT, 15.0 * UNIT], seed)
        .color(1.0, 1.0, 1.0)
        .intensity(1.5);
    s.set_glow(10.0);
    s.set_thickness(15.0);
    s.set_density(30.0);
    s.set_twinkle(0.0);
    s
}

#[test]
fn a_field_puts_stars_on_the_canvas() {
    let Some(p) = render(&[field(3.0)], 0.0) else {
        eprintln!("no GPU adapter available — skipping");
        return;
    };
    assert!(
        light_in(&p, 17, 17, 47, 47) > 0,
        "the region a field was drawn over came back empty"
    );
}

/// The box you drag is the edge of the sky: a star whose cell falls outside
/// the region doesn't exist. Checked well clear of the boundary so the
/// glow's falloff isn't what's being measured.
#[test]
fn stars_stay_inside_the_region() {
    let Some(p) = render(&[field(3.0)], 0.0) else {
        eprintln!("no GPU adapter available — skipping");
        return;
    };
    assert!(light_in(&p, 17, 17, 47, 47) > 0, "nothing inside");
    assert_eq!(light_in(&p, 0, 0, 64, 8), 0, "light above the region");
    assert_eq!(light_in(&p, 0, 56, 64, 64), 0, "light below the region");
    assert_eq!(light_in(&p, 0, 0, 8, 64), 0, "light left of the region");
    assert_eq!(light_in(&p, 56, 0, 64, 64), 0, "light right of the region");
}

/// Turning density up has to put more light on the canvas, not just
/// rearrange it: cells shrink, so the same region holds more stars.
#[test]
fn density_adds_stars() {
    let mut sparse = field(3.0);
    sparse.set_density(12.0);
    let mut dense = field(3.0);
    dense.set_density(60.0);
    let (Some(a), Some(b)) = (render(&[sparse], 0.0), render(&[dense], 0.0)) else {
        eprintln!("no GPU adapter available — skipping");
        return;
    };
    let (thin, thick) = (light_in(&a, 0, 0, 64, 64), light_in(&b, 0, 0, 64, 64));
    assert!(
        thick > thin * 2,
        "density 12 -> {thin}, density 60 -> {thick}"
    );
}

/// The other half of making density absolute: a field twice as wide holds
/// twice the sky at the same spacing, rather than the same stars stretched.
#[test]
fn a_wider_field_holds_more_sky() {
    let small = field(21.0);
    let mut wide = field(21.0);
    wide.set_box_width(60.0 * UNIT);
    let (Some(a), Some(b)) = (render(&[small], 0.0), render(&[wide], 0.0)) else {
        eprintln!("no GPU adapter available — skipping");
        return;
    };
    // Only the strip the small field never covered — if stretching had
    // magnified the same stars instead of revealing new ones, the left and
    // right edges of the frame would still be empty.
    let edges = light_in(&b, 0, 17, 12, 47);
    assert_eq!(
        light_in(&a, 0, 17, 12, 47),
        0,
        "the small field reached out"
    );
    assert!(edges > 0, "widening the field revealed no new stars");
}

/// Same seed, same sky — twice in a row and at the same playhead time. This
/// is `frame = render(project, t)` for a field nobody placed by hand: if it
/// drifted, an export would flicker and a scrub would never come back.
#[test]
fn the_same_field_renders_identically() {
    let mut twinkly = field(7.0);
    twinkly.set_twinkle(1.0);
    let (Some(a), Some(b)) = (render(&[twinkly], 1.25), render(&[twinkly], 1.25)) else {
        eprintln!("no GPU adapter available — skipping");
        return;
    };
    assert_eq!(a, b, "the same field at the same time drew differently");
}

/// Different seeds are different skies. (Same size, same density — only the
/// scatter changes, so this is really "the seed reaches the hash".)
#[test]
fn the_seed_picks_the_sky() {
    let (Some(a), Some(b)) = (render(&[field(1.0)], 0.0), render(&[field(50.0)], 0.0)) else {
        eprintln!("no GPU adapter available — skipping");
        return;
    };
    assert_ne!(a, b, "two seeds drew the same sky");
}

/// Twinkle rides the playhead, and only when it's turned up: a field at
/// twinkle 0 has to be perfectly still, or scrubbing a static backdrop
/// would shimmer.
#[test]
fn twinkle_follows_the_playhead() {
    let mut still = field(5.0);
    still.set_twinkle(0.0);
    let mut alive = field(5.0);
    alive.set_twinkle(1.0);
    alive.set_twinkle_rate(6.0);
    let (Some(s0), Some(s1)) = (render(&[still], 0.0), render(&[still], 0.4)) else {
        eprintln!("no GPU adapter available — skipping");
        return;
    };
    assert_eq!(s0, s1, "twinkle 0 moved with the playhead");
    let (Some(a0), Some(a1)) = (render(&[alive], 0.0), render(&[alive], 0.4)) else {
        return;
    };
    assert_ne!(a0, a1, "twinkle 1 stood still while the playhead moved");
}

/// Each form has to actually draw something different — a sparkle's arms and
/// a cross's spikes reach further than a dot of the same radius, so they
/// cover more of the frame.
#[test]
fn every_star_form_draws() {
    let mut out = Vec::new();
    for form in 0..crate::STAR_FORMS.len() {
        let mut s = field(9.0);
        s.set_star_form(form);
        let Some(p) = render(&[s], 0.0) else {
            eprintln!("no GPU adapter available — skipping");
            return;
        };
        assert!(
            light_in(&p, 0, 0, 64, 64) > 0,
            "form {form} ({}) drew nothing",
            crate::STAR_FORMS[form]
        );
        out.push(p);
    }
    assert_ne!(out[0], out[1], "dot and sparkle came out identical");
    assert_ne!(out[1], out[2], "sparkle and cross came out identical");
}

/// The one that matters: a filled shape at brightness 1.0 renders as
/// **exactly** the colour you picked.
///
/// It used to render at 1.55x that — the glow's exponential is at full
/// strength across a shape's whole interior, and it was added on top of the
/// body instead of only outside it. Saturated fills clipped their bright
/// channels and came back pastel, so the only way to see your own colour was
/// to crush the brightness, which is exactly what Alva ran into trying to
/// make a background.
#[test]
fn a_plain_fill_is_the_colour_you_picked() {
    let want = [0.9, 0.2, 0.45];
    let mut s = Shape::rect([32.0 * UNIT, 32.0 * UNIT], [15.0 * UNIT, 15.0 * UNIT])
        .color(want[0], want[1], want[2])
        .intensity(1.0);
    s.set_glow(0.0);
    let Some(p) = render(&[s], 0.0) else {
        eprintln!("no GPU adapter available — skipping");
        return;
    };
    let got = px(&p, 32, 32);
    let expect = want.map(srgb8);
    for c in 0..3 {
        let (g, e) = (got[c] as i32, expect[c] as i32);
        assert!(
            (g - e).abs() <= 1,
            "channel {c}: fill came back {got:?}, wanted {expect:?}"
        );
    }
}

/// Glow zero means no glow — not a very tight one. An almost-zero radius
/// still lights the fragments sitting on the boundary, which shows up as a
/// bright rim on an edge that was meant to be hard.
#[test]
fn glow_zero_leaves_nothing_outside_the_shape() {
    let mut s = Shape::rect([32.0 * UNIT, 32.0 * UNIT], [10.0 * UNIT, 10.0 * UNIT])
        .color(1.0, 1.0, 1.0)
        .intensity(1.0);
    s.set_glow(0.0);
    let Some(p) = render(&[s], 0.0) else {
        eprintln!("no GPU adapter available — skipping");
        return;
    };
    // The box covers pixels 22..42. One clear pixel outside it must be black,
    // and the edge must not be brighter than the body it belongs to.
    assert_eq!(px(&p, 44, 32), [0, 0, 0], "light outside a glowless shape");
    let body = px(&p, 32, 32)[0];
    assert!(px(&p, 42, 32)[0] <= body, "a bright rim on a hard edge");
    assert!(light_in(&p, 0, 0, 64, 18) == 0, "light above it");
}

/// ...and turning glow up still works, or the fix would have cost the neon
/// look rather than made it optional.
#[test]
fn glow_still_spills_light_when_turned_up() {
    let mut s = Shape::rect([32.0 * UNIT, 32.0 * UNIT], [10.0 * UNIT, 10.0 * UNIT])
        .color(1.0, 1.0, 1.0)
        .intensity(1.0);
    s.set_glow(60.0);
    let Some(p) = render(&[s], 0.0) else {
        eprintln!("no GPU adapter available — skipping");
        return;
    };
    assert!(px(&p, 45, 32)[0] > 0, "no halo outside a glowing shape");
    // The halo never out-burns the body it comes off.
    assert!(px(&p, 45, 32)[0] < px(&p, 32, 32)[0]);
}

/// The other kinds still draw — the star branch sits inside the same
/// fragment shader, and a bad early return there would black them out.
#[test]
fn the_older_kinds_still_render() {
    let cases: [(&str, Shape); 3] = [
        ("circle", Shape::circle([32.0, 32.0], 12.0)),
        ("box", Shape::rect([32.0, 32.0], [12.0, 12.0])),
        ("line", Shape::line([12.0, 32.0], [52.0, 32.0], 3.0)),
    ];
    for (name, shape) in cases {
        let Some(p) = render(&[shape.color(1.0, 1.0, 1.0).intensity(1.5)], 0.0) else {
            eprintln!("no GPU adapter available — skipping");
            return;
        };
        assert!(light_in(&p, 0, 0, 64, 64) > 0, "{name} drew nothing");
    }
}

/// Opacity zero is *gone*: no light, and — the half that premultiplied
/// alpha buys and a naive fade would get wrong — nothing occluded either.
/// A shape faded out that still punched a hole in what was behind it would
/// be a black shape, not an absent one.
#[test]
fn a_faded_shape_stops_occluding_as_fast_as_it_stops_emitting() {
    let solid = |c: [f32; 3]| {
        let mut s = Shape::rect([32.0 * UNIT, 32.0 * UNIT], [10.0 * UNIT, 10.0 * UNIT])
            .color(c[0], c[1], c[2])
            .intensity(1.0);
        s.set_glow(0.0);
        s
    };
    let back = solid([0.8, 0.0, 0.0]);
    let front = |o: f32| {
        let mut s = solid([1.0, 1.0, 1.0]);
        s.set_opacity(o);
        s
    };
    let (Some(gone), Some(half), Some(there)) = (
        render(&[back, front(0.0)], 0.0),
        render(&[back, front(0.5)], 0.0),
        render(&[back, front(1.0)], 0.0),
    ) else {
        eprintln!("no GPU adapter available — skipping");
        return;
    };
    let near = |got: [u8; 3], want: [f32; 3], what: &str| {
        let expect = want.map(srgb8);
        for c in 0..3 {
            assert!(
                (got[c] as i32 - expect[c] as i32).abs() <= 2,
                "{what}: read {got:?}, wanted {expect:?}"
            );
        }
    };
    near(
        px(&gone, 32, 32),
        [0.8, 0.0, 0.0],
        "opacity 0 hid the shape behind it",
    );
    near(
        px(&there, 32, 32),
        [1.0, 1.0, 1.0],
        "opacity 1 was not solid",
    );
    // src + dst*(1 - 0.5): half the white, half the red still showing.
    near(
        px(&half, 32, 32),
        [0.9, 0.5, 0.5],
        "half opacity did not blend half",
    );
}

/// On its own, a faded shape is simply dimmer — and by the amount asked
/// for, in light, not in some curve of it.
#[test]
fn half_opacity_is_half_the_light() {
    let want = [0.9, 0.2, 0.45];
    let mut s = Shape::rect([32.0 * UNIT, 32.0 * UNIT], [15.0 * UNIT, 15.0 * UNIT])
        .color(want[0], want[1], want[2])
        .intensity(1.0);
    s.set_glow(0.0);
    s.set_opacity(0.5);
    let Some(p) = render(&[s], 0.0) else {
        eprintln!("no GPU adapter available — skipping");
        return;
    };
    let got = px(&p, 32, 32);
    for c in 0..3 {
        let e = srgb8(want[c] * 0.5) as i32;
        assert!(
            (got[c] as i32 - e).abs() <= 2,
            "channel {c}: read {got:?}, wanted half of {want:?}"
        );
    }
}

/// The halo is part of the shape, so it goes with it. It composites at
/// alpha 0 — pure light — which is exactly the path a fade applied to
/// coverage alone would have missed.
#[test]
fn a_fade_takes_the_glow_with_it() {
    let lit = |o: f32| {
        let mut s = Shape::rect([32.0 * UNIT, 32.0 * UNIT], [10.0 * UNIT, 10.0 * UNIT])
            .color(1.0, 1.0, 1.0)
            .intensity(1.0);
        s.set_glow(60.0);
        s.set_opacity(o);
        s
    };
    let (Some(full), Some(dim), Some(gone)) = (
        render(&[lit(1.0)], 0.0),
        render(&[lit(0.5)], 0.0),
        render(&[lit(0.0)], 0.0),
    ) else {
        eprintln!("no GPU adapter available — skipping");
        return;
    };
    assert!(
        px(&full, 45, 32)[0] > 0,
        "no halo to fade in the first place"
    );
    assert!(
        px(&dim, 45, 32)[0] < px(&full, 45, 32)[0],
        "the halo ignored opacity"
    );
    assert_eq!(
        light_in(&gone, 0, 0, 64, 64),
        0,
        "a fully faded shape still lit the frame"
    );
}

/// A star field composites itself and returns from the fragment shader
/// early, so it is the one kind that could miss a fade applied at the end.
#[test]
fn a_faded_star_field_fades() {
    let mut gone = field(3.0);
    gone.set_opacity(0.0);
    let mut dim = field(3.0);
    dim.set_opacity(0.4);
    let (Some(g), Some(d), Some(f)) = (
        render(&[gone], 0.0),
        render(&[dim], 0.0),
        render(&[field(3.0)], 0.0),
    ) else {
        eprintln!("no GPU adapter available — skipping");
        return;
    };
    assert_eq!(
        light_in(&g, 0, 0, 64, 64),
        0,
        "a faded-out sky still had stars in it"
    );
    let (lit, part) = (light_in(&f, 0, 0, 64, 64), light_in(&d, 0, 0, 64, 64));
    assert!(
        part > 0 && part < lit,
        "40% opacity: {part} against {lit} at full"
    );
}

/// A comp saved before opacity existed is a comp where nothing had been
/// faded, because nothing could be. Reading the missing field as zero the
/// way every other field is read would open those files empty.
#[test]
fn a_shape_from_a_shorter_era_is_opaque() {
    let s = Shape::circle([100.0, 100.0], 20.0);
    let mut line = s.to_array();
    // As an 18-float star-fieldless line would arrive: tail zeroed.
    for v in line.iter_mut().skip(18) {
        *v = 0.0;
    }
    assert_eq!(Shape::from_short_array(line, 18).opacity(), 1.0);
    assert_eq!(Shape::from_short_array(line, 22).opacity(), 1.0);
    // ...but a line that *has* the field means what it says.
    line[22] = 0.25;
    assert_eq!(Shape::from_short_array(line, crate::FIELDS).opacity(), 0.25);
}
