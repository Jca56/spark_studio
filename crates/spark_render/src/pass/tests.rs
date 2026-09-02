//! Shape-pipeline tests. Beyond compiling the shader, these render into an
//! offscreen target and read the pixels back — which is the only way anyone
//! here can check a star field at all, since the person who can see the
//! screen isn't the one who can run the tests. The harness — device,
//! target, readback — is `harness.rs`.

use super::harness::*;
use super::*;

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

pub(super) fn px(pixels: &[u8], x: u32, y: u32) -> [u8; 3] {
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
pub(super) fn field(seed: f32) -> Shape {
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

/// A bolt across the frame: light along its line, none far off it.
pub(super) fn bolt(seed: f32) -> Shape {
    let mut s = Shape::bolt([8.0 * UNIT, 32.0 * UNIT], [56.0 * UNIT, 32.0 * UNIT], seed)
        .color(1.0, 1.0, 1.0)
        .intensity(1.5);
    s.set_glow(8.0);
    s.set_thickness(12.0);
    s.set_jag(60.0);
    s.set_branches(2.0);
    s.set_strike_rate(0.0);
    s
}

/// Lightning lands between its ends and wanders no further than its jag:
/// the band across the middle lights up, the top and bottom stay dark.
#[test]
fn a_bolt_runs_between_its_ends() {
    let Some(p) = render(&[bolt(3.0)], 0.0) else {
        eprintln!("no GPU adapter available — skipping");
        return;
    };
    assert!(light_in(&p, 8, 22, 56, 42) > 0, "no bolt along the line");
    assert_eq!(light_in(&p, 0, 0, 64, 12), 0, "light far above the bolt");
    assert_eq!(light_in(&p, 0, 52, 64, 64), 0, "light far below the bolt");
    // It is not a straight line: the seed picks a wander, and two seeds
    // wander differently.
    let straight = {
        let mut s = bolt(3.0);
        s.set_jag(0.0);
        s.set_branches(0.0);
        s
    };
    let (Some(s0), Some(other)) = (render(&[straight], 0.0), render(&[bolt(4.0)], 0.0)) else {
        return;
    };
    assert_ne!(p, s0, "jag 60 drew the same as jag 0");
    assert_ne!(p, other, "two seeds drew the same bolt");
    let Some(again) = render(&[bolt(3.0)], 0.0) else { return };
    assert_eq!(p, again, "the same bolt didn't render identically");
}

/// A bolt with a strike rate re-rolls on its clock; at rate 0 it holds
/// still whatever the clock says.
#[test]
fn a_bolt_crackles_on_its_clock() {
    let held = bolt(5.0);
    let mut live = bolt(5.0);
    live.set_strike_rate(10.0);
    let (Some(h0), Some(h1)) = (render(&[held], 0.0), render(&[held], 0.37)) else {
        eprintln!("no GPU adapter available — skipping");
        return;
    };
    assert_eq!(h0, h1, "rate 0 moved with the clock");
    let (Some(l0), Some(l1)) = (render(&[live], 0.0), render(&[live], 0.37)) else {
        return;
    };
    assert_ne!(l0, l1, "rate 10 held still across the clock");
}

/// Every shape runs on its own clock, not the frame's: two identical
/// fields handed different clocks at one frame time render differently,
/// and a clock equal to the frame time renders as the frame time did.
#[test]
fn each_shape_keeps_its_own_clock() {
    let mut alive = field(5.0);
    alive.set_twinkle(1.0);
    alive.set_twinkle_rate(6.0);
    let (Some(at_frame), Some(clocked)) = (
        render(&[alive], 0.4),
        render_clocked(&[alive], &[0.4], 0.0),
    ) else {
        eprintln!("no GPU adapter available — skipping");
        return;
    };
    assert_eq!(at_frame, clocked, "a clock of 0.4 differs from a frame at 0.4");
    let Some(other) = render_clocked(&[alive], &[0.0], 0.4) else { return };
    assert_ne!(at_frame, other, "the clock was ignored for the frame time");
}

/// A vortex filling the middle of the frame: the disk's radius is 24 px,
/// so the void (a third of it) covers the centre and the ring sits
/// about 10 px out.
pub(super) fn vortex(seed: f32) -> Shape {
    let mut s = Shape::vortex([32.0 * UNIT, 32.0 * UNIT], [24.0 * UNIT, 24.0 * UNIT], seed)
        .color(1.0, 0.6, 0.2)
        .intensity(1.5);
    s.set_spin(0.0);
    s
}

/// The disk lights the frame, the void in its middle stays black, and
/// nothing reaches outside the region.
#[test]
fn a_vortex_has_a_ring_and_a_void() {
    let Some(p) = render(&[vortex(3.0)], 0.0) else {
        eprintln!("no GPU adapter available — skipping");
        return;
    };
    assert!(light_in(&p, 8, 8, 56, 56) > 0, "the disk drew nothing");
    // The void: a black hole a third of the disk across at the centre.
    assert_eq!(light_in(&p, 29, 29, 35, 35), 0, "the void isn't black");
    // The ring, about ten px out from the centre, is the brightest band.
    let ring = light_in(&p, 40, 30, 46, 34);
    let edge = light_in(&p, 52, 30, 56, 34);
    assert!(ring > edge, "the ring ({ring}) is dimmer than the edge ({edge})");
    assert_eq!(light_in(&p, 0, 0, 64, 6), 0, "light above the region");
    assert_eq!(light_in(&p, 0, 0, 6, 64), 0, "light left of the region");
    // Two seeds are two different skies of streaks; one seed is one.
    let (Some(other), Some(again)) = (render(&[vortex(4.0)], 0.0), render(&[vortex(3.0)], 0.0)) else {
        return;
    };
    assert_ne!(p, other, "two seeds drew the same streaks");
    assert_eq!(p, again, "the same vortex didn't render identically");
}

/// A spinning vortex turns on its clock; at spin 0 it holds still.
#[test]
fn a_vortex_turns_on_its_clock() {
    let held = vortex(5.0);
    let mut live = vortex(5.0);
    live.set_spin(2.0);
    let (Some(h0), Some(h1)) = (render(&[held], 0.0), render(&[held], 0.5)) else {
        eprintln!("no GPU adapter available — skipping");
        return;
    };
    assert_eq!(h0, h1, "spin 0 moved with the clock");
    let (Some(l0), Some(l1)) = (render(&[live], 0.0), render(&[live], 0.5)) else {
        return;
    };
    assert_ne!(l0, l1, "spin 2 held still across the clock");
    // A bigger hole swallows the ring's old place.
    let mut wide = vortex(5.0);
    wide.set_hole(0.8);
    let Some(w) = render(&[wide], 0.0) else { return };
    assert_eq!(light_in(&p_of(&w), 40, 30, 46, 34), 0, "hole 0.8 left light at 10 px out");
}

fn p_of(p: &[u8]) -> Vec<u8> {
    p.to_vec()
}

