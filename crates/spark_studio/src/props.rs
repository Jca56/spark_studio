//! The editor's vocabulary: tools, editable properties, the neon palette,
//! and the shape factory. Split from `editor` so the interaction state
//! machine stays readable.

use spark_render::{CANVAS_H, CANVAS_W, Shape};

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
}

/// An animatable/editable property of the selected shape. The React trio
/// are audio-reaction amounts (bass→scale, bass→glow, mid/onset→bright):
/// inspector-editable and saved, but never keyframed.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Prop {
    X,
    Y,
    Rotation,
    Scale,
    Width,
    Height,
    Glow,
    Brightness,
    Sides,
    Thickness,
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
        Prop::Rotation => (-std::f32::consts::PI, std::f32::consts::PI),
        Prop::Scale => (3.0, 900.0),
        Prop::Width => (6.0, CANVAS_W),
        Prop::Height => (6.0, CANVAS_H),
        Prop::Glow => (2.0, 300.0),
        Prop::Brightness => (0.05, 5.0),
        Prop::Sides => (3.0, 12.0),
        Prop::Thickness => (1.0, 30.0),
        Prop::ReactScale | Prop::ReactGlow | Prop::ReactBright => (0.0, 2.0),
    }
}

/// Map a normalized slider position back to a property value.
pub fn value_for(prop: Prop, t: f32) -> f32 {
    let (min, max) = range(prop);
    min + t.clamp(0.0, 1.0) * (max - min)
}

/// Fit a hand-entered value into its property's range. Rotation is an angle,
/// so it wraps — scrubbing past 180° rolls over instead of jamming. Only
/// input wraps: keyframed rotation stays unbounded so a curve can spin a
/// shape through 720° without folding back on itself.
pub fn fit(prop: Prop, v: f32) -> f32 {
    if prop == Prop::Rotation {
        return wrap_angle(v);
    }
    let (min, max) = range(prop);
    v.clamp(min, max)
}

/// Fold an angle into (-π, π].
pub fn wrap_angle(a: f32) -> f32 {
    use std::f32::consts::{PI, TAU};
    let r = a.rem_euclid(TAU);
    if r > PI { r - TAU } else { r }
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
    let shape = match tool {
        Tool::Circle => Shape::circle(press, d).stroke(4.0),
        Tool::Box => Shape::rect(
            press,
            [
                (cursor[0] - press[0]).abs().max(3.0),
                (cursor[1] - press[1]).abs().max(3.0),
            ],
        )
        .stroke(4.0),
        Tool::Polygon => Shape::ngon(press, d, sides).stroke(4.0),
        Tool::Line => Shape::line(press, cursor, 3.0),
        Tool::Select => unreachable!("draw_shape is never called with Select"),
    };
    shape
        .color(rgb[0], rgb[1], rgb[2])
        .intensity(1.4)
        .glow(if tool == Tool::Line { 24.0 } else { 30.0 })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::{FRAC_PI_2, PI, TAU};

    #[test]
    fn rotation_wraps_instead_of_clamping() {
        // Scrubbing past 180° rolls over to the far side, not a dead stop.
        let just_over = PI + 0.1;
        assert!((fit(Prop::Rotation, just_over) - (-PI + 0.1)).abs() < 1e-4);
        assert!((fit(Prop::Rotation, -PI - 0.1) - (PI - 0.1)).abs() < 1e-4);
        // A full turn is the identity; multi-turn input folds back in.
        assert!(fit(Prop::Rotation, TAU).abs() < 1e-4);
        assert!((fit(Prop::Rotation, TAU + FRAC_PI_2) - FRAC_PI_2).abs() < 1e-4);
        // Everything inside the range passes through untouched.
        assert!((fit(Prop::Rotation, FRAC_PI_2) - FRAC_PI_2).abs() < 1e-6);
    }

    #[test]
    fn wrapped_angles_stay_in_range() {
        for i in -40..40 {
            let a = i as f32 * 0.37;
            let w = wrap_angle(a);
            assert!(w > -PI - 1e-5 && w <= PI + 1e-5, "{a} wrapped to {w}");
            // Wrapping preserves the angle modulo a full turn.
            assert!(((a - w) / TAU - ((a - w) / TAU).round()).abs() < 1e-4);
        }
    }

    #[test]
    fn other_props_still_clamp() {
        let (min, max) = range(Prop::Glow);
        assert_eq!(fit(Prop::Glow, max + 500.0), max);
        assert_eq!(fit(Prop::Glow, min - 500.0), min);
    }
}
