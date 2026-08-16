//! The editor's vocabulary: tools, editable properties, the neon palette,
//! and the shape factory. Split from `editor` so the interaction state
//! machine stays readable.

use spark_render::Shape;

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

/// An animatable/editable property of the selected shape.
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
}

/// Snapshot of the primary selection's properties for the inspector.
pub struct Props {
    pub x: f32,
    pub y: f32,
    pub rotation: f32,
    pub size: f32,
    pub glow: f32,
    pub brightness: f32,
    pub sides: Option<u32>,
    /// Full box dimensions; `None` for non-boxes.
    pub box_size: Option<[f32; 2]>,
    /// Stroke half-width; `None` for filled shapes.
    pub thickness: Option<f32>,
    /// The shape's color (linear), for the picker's preview chip.
    pub rgb: [f32; 3],
    /// Which palette entry the shape's color matches, if any.
    pub palette: Option<usize>,
    /// `None` for lines — no fill/outline distinction.
    pub outline: Option<bool>,
    /// Composites as pure light instead of occluding.
    pub additive: bool,
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
