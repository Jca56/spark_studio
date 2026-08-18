//! The editor's vocabulary: tools, editable properties, the neon palette,
//! and the shape factory. Split from `editor` so the interaction state
//! machine stays readable.

use spark_render::{CANVAS_H, CANVAS_W, Shape};
use spark_ui::{ICON_ARROW, ICON_CIRCLE, ICON_LINE, ICON_PENTAGON, ICON_SQUARE, ICON_STARS};

pub const PALETTE: [[f32; 3]; 7] = [
    [1.00, 0.16, 0.85], // magenta
    [0.16, 0.75, 1.00], // cyan
    [0.55, 0.25, 1.00], // violet
    [1.00, 0.45, 0.10], // ember
    [0.10, 1.00, 0.55], // acid
    [1.00, 0.95, 0.30], // laser
    [1.00, 0.10, 0.12], // red
];
pub(crate) const PALETTE_NAMES: [&str; 7] =
    ["magenta", "cyan", "violet", "ember", "acid", "laser", "red"];

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Tool {
    Select,
    Circle,
    Box,
    Polygon,
    Line,
    /// Drag a region; it fills with scattered stars.
    Stars,
}

/// The tool strip, in display order: tool + icon glyph. Number keys pick
/// them in this same order — `1` is Select.
pub const TOOLS: [(Tool, f32); 6] = [
    (Tool::Select, ICON_ARROW),
    (Tool::Circle, ICON_CIRCLE),
    (Tool::Box, ICON_SQUARE),
    (Tool::Polygon, ICON_PENTAGON),
    (Tool::Line, ICON_LINE),
    (Tool::Stars, ICON_STARS),
];

/// An animatable/editable property of the selected shape. The React trio
/// are audio-reaction amounts (bass→scale, bass→glow, mid/onset→bright):
/// inspector-editable and saved, but never keyframed. Neither is `Seed`,
/// which picks *which* scatter a star field is — interpolating between two
/// skies is a re-roll every frame, not an animation.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Prop {
    X,
    Y,
    Rotation,
    Scale,
    Width,
    Height,
    Glow,
    Brightness,
    /// How much of the shape reaches the frame. Unlike brightness, which is
    /// how hard it burns, this is whether it is *there* — and it is on the
    /// shape rather than on the effects stack because fading in and out is
    /// not a look you add, it is the most ordinary thing an animation does.
    Opacity,
    Sides,
    Thickness,
    /// Star field: cells across the longer axis, one star each.
    Density,
    /// Star field: how hard the stars pulse.
    Twinkle,
    /// Star field: how fast they pulse, radians per second.
    TwinkleRate,
    Seed,
    ReactScale,
    ReactGlow,
    ReactBright,
}

/// Style settings carried by Ctrl+C / Ctrl+V between shapes — the look,
/// never the geometry.
#[derive(Clone)]
pub struct StyleClip {
    pub rgb: [f32; 3],
    pub intensity: f32,
    pub glow: f32,
    pub thickness: Option<f32>,
    pub outline: Option<bool>,
    pub additive: bool,
    /// Gradient fill: on/off and the end color.
    pub gradient: bool,
    pub rgb2: [f32; 3],
    /// A star field's look. `None` off a field, and the setters are no-ops
    /// off one too, so a field's style pastes harmlessly onto a circle and
    /// a circle's leaves a field's stars alone.
    pub density: Option<f32>,
    pub twinkle: Option<f32>,
    pub twinkle_rate: Option<f32>,
    pub star_form: Option<usize>,
}

/// Snapshot of the primary selection, for scrubbing, handle drags, and
/// the color home. The layer cards read their shapes directly.
pub struct Props {
    pub x: f32,
    pub y: f32,
    pub rotation: f32,
    pub size: f32,
    /// The shape's color (linear).
    pub rgb: [f32; 3],
    /// Two-color gradient fill enabled.
    pub grad: bool,
    /// The gradient's end color (linear).
    pub rgb2: [f32; 3],
}

/// Slider/scrub range per property.
pub fn range(prop: Prop) -> (f32, f32) {
    match prop {
        Prop::X => (0.0, CANVAS_W),
        Prop::Y => (0.0, CANVAS_H),
        // Never clamped — see `fit`. Here for the slider maths only.
        Prop::Rotation => (-std::f32::consts::PI, std::f32::consts::PI),
        Prop::Scale => (3.0, 900.0),
        Prop::Width => (6.0, CANVAS_W),
        Prop::Height => (6.0, CANVAS_H),
        // Glow starts at nothing. Brightness stops at 3 rather than 5: 1.0 is
        // now exactly the colour you picked, so everything above it is
        // overdrive, and a slider whose useful half is its first fifth is a
        // slider you can't aim.
        Prop::Glow => (0.0, 200.0),
        Prop::Brightness => (0.05, 3.0),
        // All the way to nothing: an effect that can't reach zero is a
        // fade-out you can only ever nearly do.
        Prop::Opacity => (0.0, 1.0),
        Prop::Sides => (3.0, 12.0),
        Prop::Thickness => (1.0, 30.0),
        Prop::Density => (2.0, 120.0),
        Prop::Twinkle => (0.0, 1.0),
        Prop::TwinkleRate => (0.0, 12.0),
        Prop::Seed => (0.0, 100.0),
        Prop::ReactScale | Prop::ReactGlow | Prop::ReactBright => (0.0, 2.0),
    }
}

/// Map a normalized slider position back to a property value.
pub fn value_for(prop: Prop, t: f32) -> f32 {
    let (min, max) = range(prop);
    min + t.clamp(0.0, 1.0) * (max - min)
}

/// Fit a hand-entered value into its property's range.
///
/// Rotation is exempt: it is not an angle in (-π, π], it is *how far the
/// shape has turned*, and it has to keep counting. Folding it made a
/// continuous spin impossible — stamp 0°, stamp 180°, then keep turning and
/// the third key came back as -170° instead of 190°, so the shape unwound
/// counter-clockwise to get there. Two full turns is 720 and means it.
pub fn fit(prop: Prop, v: f32) -> f32 {
    if prop == Prop::Rotation {
        return v;
    }
    let (min, max) = range(prop);
    v.clamp(min, max)
}

/// Where a stack index lands after `remove(from)` + `insert(to, _)`.
pub(crate) fn remap(s: usize, from: usize, to: usize) -> usize {
    if s == from {
        to
    } else if from < to && s > from && s <= to {
        s - 1
    } else if to < from && s >= to && s < from {
        s + 1
    } else {
        s
    }
}

pub(crate) fn dist(a: [f32; 2], b: [f32; 2]) -> f32 {
    ((b[0] - a[0]).powi(2) + (b[1] - a[1]).powi(2)).sqrt()
}

pub(crate) fn draw_shape(
    tool: Tool,
    press: [f32; 2],
    cursor: [f32; 2],
    sides: u32,
    rgb: [f32; 3],
) -> Shape {
    let d = dist(press, cursor).max(3.0);
    let half = [
        (cursor[0] - press[0]).abs().max(3.0),
        (cursor[1] - press[1]).abs().max(3.0),
    ];
    // A star field carries its own glow and star size, tuned so the first
    // drag already looks like a sky — only the color comes from the tool.
    if tool == Tool::Stars {
        return Shape::stars(press, half, seed_at(press))
            .color(rgb[0], rgb[1], rgb[2])
            .intensity(1.4);
    }
    let shape = match tool {
        Tool::Circle => Shape::circle(press, d).stroke(4.0),
        Tool::Box => Shape::rect(press, half).stroke(4.0),
        Tool::Polygon => Shape::ngon(press, d, sides).stroke(4.0),
        Tool::Line => Shape::line(press, cursor, 3.0),
        Tool::Select | Tool::Stars => unreachable!("handled above"),
    };
    // Plain by default: the colour you picked, at the brightness you picked,
    // with no halo. Glow is a thing you turn on (the Glow slider, or `A` /
    // `Z` on the keyboard) rather than a thing you spend the session turning
    // off.
    shape.color(rgb[0], rgb[1], rgb[2]).intensity(1.0).glow(0.0)
}

/// A star field's seed, from where it was drawn: two fields dragged in
/// different places get different skies, and re-dragging the same one keeps
/// its own while you size it.
pub(crate) fn seed_at(p: [f32; 2]) -> f32 {
    (p[0] * 7.13 + p[1] * 3.77).abs() % 100.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::{FRAC_PI_2, PI, TAU};

    /// Rotation counts turns; it does not fold. Folding it is what made a
    /// continuous spin impossible — the key past half a turn came back
    /// negative and the shape unwound to reach it.
    #[test]
    fn rotation_keeps_counting_past_a_full_turn() {
        for turns in [0.5f32, 1.0, 2.0, 5.0, -3.0] {
            let a = TAU * turns;
            assert!(
                (fit(Prop::Rotation, a) - a).abs() < 1e-4,
                "{turns} turns came back as {}",
                fit(Prop::Rotation, a) / TAU
            );
        }
        // And it still passes small angles through untouched.
        assert!((fit(Prop::Rotation, FRAC_PI_2) - FRAC_PI_2).abs() < 1e-6);
        assert!((fit(Prop::Rotation, PI + 0.1) - (PI + 0.1)).abs() < 1e-6);
    }

    /// The tool strip is a single row of square buttons across the top of
    /// the left panel, and the panel has a hard minimum width. Six buttons
    /// fit with about sixteen logical px to spare — the seventh will not, so
    /// this fails the moment a tool is added without making room for it.
    #[test]
    fn the_tool_strip_fits_the_narrowest_left_panel() {
        // IconBar's own metrics: 6px pad each end, square buttons the height
        // of the strip less that padding, 8px between.
        const PANEL_MIN: f32 = 380.0;
        const STRIP_H: f32 = 64.0;
        let side = STRIP_H - 12.0;
        let n = TOOLS.len() as f32;
        let needed = 12.0 + side * n + 8.0 * (n - 1.0);
        assert!(
            needed <= PANEL_MIN,
            "{} tools need {needed} logical px, panel gives {PANEL_MIN}",
            TOOLS.len()
        );
    }

    /// Every tool the strip offers has to be a tool the editor can draw
    /// with, and no tool may be listed twice.
    #[test]
    fn the_tool_strip_lists_each_tool_once() {
        for (i, (tool, _)) in TOOLS.iter().enumerate() {
            assert!(
                !TOOLS[..i].iter().any(|(t, _)| t == tool),
                "{tool:?} is in the strip twice"
            );
        }
        assert_eq!(TOOLS[0].0, Tool::Select, "Select leads, and `1` picks it");
    }

    /// Drawing a field is a drag over a region, like a box — and it must
    /// come out as a field, not as whatever the last kind added was.
    #[test]
    fn the_stars_tool_draws_a_field() {
        let s = draw_shape(Tool::Stars, [100.0, 100.0], [220.0, 180.0], 5, [1.0; 3]);
        assert_eq!(s.kind(), spark_render::ShapeKind::Stars);
        assert_eq!(s.center(), [100.0, 100.0]);
        assert_eq!(s.box_size(), Some([240.0, 160.0]), "the dragged region");
        assert!(s.density().is_some() && s.twinkle().is_some());
        // Two fields drawn in different places are different skies.
        let other = draw_shape(Tool::Stars, [700.0, 400.0], [820.0, 480.0], 5, [1.0; 3]);
        assert_ne!(s.seed(), other.seed());
    }

    #[test]
    fn other_props_still_clamp() {
        let (min, max) = range(Prop::Glow);
        assert_eq!(fit(Prop::Glow, max + 500.0), max);
        assert_eq!(fit(Prop::Glow, min - 500.0), min);
    }
}
