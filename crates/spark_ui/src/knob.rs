//! The dial, ported from Lantern Mix (`lmx_ui/knob.rs`, itself from the
//! Lantern VST plugins): a groove lit from above; a value arc that heats
//! toward the pointer over its own glow; a cap floating on a drop shadow
//! with a specular catch and a rim highlight; a chicken-head pointer with
//! its own shadow that retracts to the rim as a readout fades in. No
//! graduation ticks — Alva took them off at the source.
//!
//! Alva's call, carried over: the knobs are the one place the UI glows.
//!
//! Pure geometry — this returns rects; the caller owns the value, the
//! hover crossfade (0..1), and draws any readout/label text itself
//! ([`Dial`] hands back the spots). Angular gradients (the lit groove,
//! the heat sweep) are CPU-segmented arcs: the shader speaks linear and
//! radial, and a dozen short arcs per knob is nothing.

use spark_render::Viewport;

use crate::rect::UiRect;
use crate::surface::{darken, lighten};
use crate::theme::theme;

/// Track start, in turns clockwise from straight up: bottom-left.
pub const A0: f32 = 0.625;
/// Track sweep: 270° clockwise to bottom-right.
pub const SWEEP: f32 = 0.75;
/// Vertical drag (logical px) for the full range — for the caller's input.
pub const DRAG_PX: f32 = 200.0;

/// A dial's geometry, all derived from the radius so a big knob is a
/// magnified small knob, not a thin-ringed stranger. `radius` is the
/// track's centerline (physical px); the cap turns inside it.
#[derive(Clone, Copy, Debug)]
pub struct Dial {
    pub radius: f32,
    pub track_w: f32,
    /// The cap — the part that turns — ends here.
    pub cap_r: f32,
    /// The track's outer edge (the pointer's tip reaches just past it).
    pub outer: f32,
}

impl Dial {
    pub fn new(radius: f32, scale: f32) -> Self {
        let track_w = (radius * 0.13).clamp(4.0 * scale, 14.0 * scale);
        let gap = (radius * 0.10).clamp(4.0 * scale, 9.0 * scale);
        Self {
            radius,
            track_w,
            cap_r: radius - track_w - gap,
            outer: radius + track_w * 0.5,
        }
    }

    /// The biggest dial that fits in a `size`-wide cell.
    pub fn fit(size: f32, scale: f32) -> Self {
        Self::new((size * 0.5 - 6.0 * scale).max(15.0 * scale).floor(), scale)
    }

    /// Where a readout centers (inside the cap) and a label sits (under
    /// the dial), for the caller's text pass.
    pub fn readout_center(&self, center: [f32; 2]) -> [f32; 2] {
        center
    }

    pub fn label_top(&self, center: [f32; 2], pad: f32) -> f32 {
        center[1] + self.outer + pad
    }
}

/// What a knob shows besides its value.
#[derive(Clone, Copy, Debug)]
pub struct Knob {
    /// Arc color at its start …
    pub color: [f32; 4],
    /// … and toward the pointer.
    pub hot: [f32; 4],
    /// The arc grows from the center instead of from the start.
    pub bipolar: bool,
}

/// A circle as a quad: a rounded box at full radius.
fn circle(center: [f32; 2], r: f32, color: [f32; 4]) -> UiRect {
    UiRect::region_rounded(
        Viewport {
            x: center[0] - r,
            y: center[1] - r,
            w: r * 2.0,
            h: r * 2.0,
        },
        color,
        r,
    )
}

/// A soft radial blob — shadows and glows without a second field sample.
fn blob(center: [f32; 2], r: f32, color: [f32; 4]) -> UiRect {
    let mut out = [color[0], color[1], color[2], 0.0];
    out[3] = 0.0;
    circle(center, r, color).gradient_radial(out)
}

fn lerp4(a: [f32; 4], b: [f32; 4], t: f32) -> [f32; 4] {
    [
        a[0] + (b[0] - a[0]) * t,
        a[1] + (b[1] - a[1]) * t,
        a[2] + (b[2] - a[2]) * t,
        a[3] + (b[3] - a[3]) * t,
    ]
}

fn alpha(c: [f32; 4], a: f32) -> [f32; 4] {
    [c[0], c[1], c[2], a]
}

/// An arc of the dial's track, `start`..`start+sweep` in turns, colored by
/// `shade(mid-angle)` per segment — the CPU's angular gradient.
fn segmented_arc(
    out: &mut Vec<UiRect>,
    center: [f32; 2],
    radius: f32,
    width: f32,
    start: f32,
    sweep: f32,
    shade: impl Fn(f32) -> [f32; 4],
) {
    let quad = Viewport {
        x: center[0] - radius - width,
        y: center[1] - radius - width,
        w: (radius + width) * 2.0,
        h: (radius + width) * 2.0,
    };
    let segs = ((sweep * 48.0).ceil() as usize).max(2);
    let step = sweep / segs as f32;
    for k in 0..segs {
        let a = start + step * k as f32;
        // A hair of overlap keeps the round caps from reading as beads.
        let s = step + (step * 0.35).min(0.004);
        out.push(UiRect::arc(
            quad,
            a,
            s,
            radius / (quad.w * 0.5),
            width,
            shade(a + step * 0.5),
        ));
    }
}

/// The dial's rects, back to front. `v` is the value 0..1, `hover` the
/// caller's 0..1 crossfade (readout fading in, pointer retracting), `held`
/// while a drag is on it. Text is the caller's.
pub fn knob_rects(
    center: [f32; 2],
    radius: f32,
    scale: f32,
    v: f32,
    hover: f32,
    held: bool,
    k: &Knob,
) -> Vec<UiRect> {
    let th = theme();
    let d = Dial::new(radius, scale);
    let v = v.clamp(0.0, 1.0);
    let mut out = Vec::with_capacity(40);
    let (cx, cy) = (center[0], center[1]);

    // Soft halo when engaged — a whisper of light, the one glow.
    if hover > 0.02 {
        let halo = radius * 2.6;
        let (c, a) = if held {
            (k.hot, 0.05)
        } else {
            (lighten(k.hot, 0.5), 0.028)
        };
        out.push(blob(center, halo, alpha(c, hover * a)));
    }

    // The track is a groove lit from above: its top in shadow, its bottom
    // catching light.
    let (g_dark, g_light) = (darken(th.knob_track, 0.55), lighten(th.knob_track, 0.08));
    segmented_arc(&mut out, center, radius, d.track_w * 0.5, A0, SWEEP, |a| {
        // Brightness by height around the ring: 0 at top, 1 at bottom.
        let t = 0.5 - 0.5 * (a * std::f32::consts::TAU).cos();
        lerp4(g_dark, g_light, t)
    });

    // The value arc: cool at its start, heating toward the pointer, over a
    // soft glow that brightens under the cursor.
    let mid = A0 + SWEEP * 0.5;
    let at = A0 + SWEEP * v;
    let (a0, a1, frac, pointer_at_end) = if k.bipolar {
        let frac = (v - 0.5).abs() * 2.0;
        if v >= 0.5 {
            (mid, at, frac, true)
        } else {
            (at, mid, frac, false)
        }
    } else {
        (A0, at, v, true)
    };
    if frac > 0.001 {
        let (cool, hot) = if held {
            (k.hot, lighten(k.hot, 0.35))
        } else {
            (k.color, lerp4(k.color, k.hot, frac * 0.8))
        };
        let (from, to) = if pointer_at_end { (cool, hot) } else { (hot, cool) };
        // The under-glow: one arc wearing a halo.
        let quad = Viewport {
            x: cx - d.outer,
            y: cy - d.outer,
            w: d.outer * 2.0,
            h: d.outer * 2.0,
        };
        out.push(
            UiRect::arc(
                quad,
                a0,
                a1 - a0,
                radius / d.outer,
                d.track_w * 0.5,
                alpha(hot, 0.16 + 0.12 * hover),
            )
            .glow(d.track_w * 0.9, alpha(hot, 0.16 + 0.12 * hover)),
        );
        let span = a1 - a0;
        segmented_arc(&mut out, center, radius, d.track_w * 0.5, a0, span, |a| {
            lerp4(from, to, ((a - a0) / span.max(1e-4)).clamp(0.0, 1.0))
        });
    }

    // The cap floats above the panel: a drop shadow across the groove, a
    // face lit from above, a specular catch high on the left, a rim
    // highlight along the lit edge fading out below.
    let lift = (radius * 0.06).clamp(2.0 * scale, 4.0 * scale);
    let soft = (radius * 0.10).clamp(3.0 * scale, 7.0 * scale);
    out.push(blob([cx, cy + lift], d.cap_r + soft, [0.0, 0.0, 0.0, 0.6]));
    out.push(circle(center, d.cap_r, th.knob_cap_hi).gradient_v(th.knob_cap_lo));
    out.push(blob(
        [cx - d.cap_r * 0.3, cy - d.cap_r * 0.5],
        d.cap_r * 0.8,
        [1.0, 1.0, 1.0, 0.08],
    ));
    let rim = (radius * 0.04).clamp(1.5 * scale, 2.5 * scale);
    out.push(
        circle(center, d.cap_r, [0.0; 4])
            .stroke(rim, [1.0, 1.0, 1.0, 0.30])
            .gradient_v([1.0, 1.0, 1.0, 0.0]),
    );

    // Chicken-head pointer: a wedge, wide at the cap and pointed at the
    // rim, casting its own soft shadow. It retracts toward the rim (and
    // slims) as the readout fades in, so pointer and number never fight.
    let inner = 3.0 * scale + (radius - d.track_w * 0.5 - 10.0 * scale) * hover;
    let tip = d.outer + 1.0 * scale;
    let halfw = (radius * 0.085).clamp(3.5 * scale, 9.0 * scale) * (1.0 - hover * 0.5);
    let drop = (radius * 0.04).clamp(1.5 * scale, 3.0 * scale);
    let quad = Viewport {
        x: cx - tip,
        y: cy - tip,
        w: tip * 2.0,
        h: tip * 2.0,
    };
    out.push(
        UiRect::wedge(quad, inner / tip, halfw, th.icon_hover)
            .rotate(at - 0.25)
            .shadow([0.0, drop], drop, 0.0, [0.0, 0.0, 0.0, 0.55]),
    );
    if hover < 0.98 {
        let hub = (radius * 0.10).clamp(4.5 * scale, 8.0 * scale);
        out.push(blob(
            [cx, cy + drop],
            hub + drop,
            [0.0, 0.0, 0.0, 0.55 * (1.0 - hover)],
        ));
        out.push(circle(center, hub, alpha(th.icon_hover, 1.0 - hover)));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dial_scales_with_radius_and_fits_its_cell() {
        let small = Dial::new(30.0, 1.0);
        let big = Dial::new(60.0, 1.0);
        assert!(big.track_w > small.track_w && big.cap_r > small.cap_r);
        assert!(small.cap_r < small.radius - small.track_w);
        let d = Dial::fit(104.0, 1.0);
        assert_eq!(d.radius, 46.0);
        assert!(d.outer * 2.0 <= 104.0);
    }

    /// The groove spans exactly the dial's 270°, and the value arc ends at
    /// the value — segmentation must not shorten or overshoot the sweep.
    #[test]
    fn the_arcs_cover_their_sweeps() {
        let k = Knob {
            color: [0.2, 0.5, 1.0, 1.0],
            hot: [1.0, 0.5, 0.2, 1.0],
            bipolar: false,
        };
        let rects = knob_rects([200.0, 200.0], 45.0, 1.0, 1.0, 0.0, false, &k);
        // Every arc rect's [start, start+sweep] stays inside the track.
        let arcs: Vec<&UiRect> = rects
            .iter()
            .filter(|r| r.icon[0] == crate::rect::ICON_ARC)
            .collect();
        assert!(arcs.len() > 10, "segmented arcs expected, got {}", arcs.len());
        let lo = arcs.iter().map(|r| r.radii[0]).fold(f32::MAX, f32::min);
        let hi = arcs
            .iter()
            .map(|r| r.radii[0] + r.radii[1])
            .fold(f32::MIN, f32::max);
        assert!((lo - A0).abs() < 1e-3, "track starts at A0, got {lo}");
        assert!(
            hi <= A0 + SWEEP + 0.01,
            "nothing sweeps past the track's end: {hi}"
        );
    }

    /// The pointer aims at the value: at 0 it points along the track
    /// start, at 1 along its end, and the wedge kind carries the rotation.
    #[test]
    fn the_pointer_tracks_the_value() {
        let k = Knob {
            color: [1.0; 4],
            hot: [1.0; 4],
            bipolar: false,
        };
        let wedge_turns = |v: f32| {
            let rects = knob_rects([0.0, 0.0], 45.0, 1.0, v, 0.0, false, &k);
            rects
                .iter()
                .find(|r| r.icon[0] == crate::rect::ICON_WEDGE)
                .expect("a pointer")
                .xform[0]
        };
        assert!((wedge_turns(0.0) - (A0 - 0.25)).abs() < 1e-4);
        assert!((wedge_turns(1.0) - (A0 + SWEEP - 0.25)).abs() < 1e-4);
    }
}
