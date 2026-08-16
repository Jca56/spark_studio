//! Inspector panel geometry: labeled slider rows for the selected shape.
//! Pure layout + value mapping — rendering and drag state live in main.

use spark_render::{CANVAS_H, CANVAS_W, Viewport};

use crate::editor::{Prop, Props};

pub struct Row {
    pub prop: Prop,
    pub label: &'static str,
    /// Top-left of the label text (physical px).
    pub label_pos: [f32; 2],
    pub track: Viewport,
    /// Normalized slider position, 0..1.
    pub t: f32,
    pub value: String,
}

fn range(prop: Prop) -> (f32, f32) {
    match prop {
        Prop::X => (0.0, CANVAS_W),
        Prop::Y => (0.0, CANVAS_H),
        Prop::Rotation => (-std::f32::consts::PI, std::f32::consts::PI),
        Prop::Glow => (2.0, 300.0),
        Prop::Brightness => (0.05, 5.0),
        Prop::Sides => (3.0, 12.0),
    }
}

/// Map a normalized slider position back to a property value.
pub fn value_for(prop: Prop, t: f32) -> f32 {
    let (min, max) = range(prop);
    min + t.clamp(0.0, 1.0) * (max - min)
}

pub fn rows(panel: Viewport, scale: f32, props: &Props) -> Vec<Row> {
    let pad = 16.0 * scale;
    let row_h = 76.0 * scale;
    let mut y = panel.y + pad;
    let mut out = Vec::new();
    let mut push = |prop: Prop, label: &'static str, v: f32, value: String| {
        let (min, max) = range(prop);
        out.push(Row {
            prop,
            label,
            label_pos: [panel.x + pad, y],
            track: Viewport {
                x: panel.x + pad,
                y: y + 42.0 * scale,
                w: (panel.w - pad * 2.0).max(1.0),
                h: 11.0 * scale,
            },
            t: ((v - min) / (max - min)).clamp(0.0, 1.0),
            value,
        });
        y += row_h;
    };
    push(Prop::X, "X", props.x, format!("{:.0}", props.x));
    push(Prop::Y, "Y", props.y, format!("{:.0}", props.y));
    push(
        Prop::Rotation,
        "Rotation",
        props.rotation,
        format!("{:.0}\u{b0}", props.rotation.to_degrees()),
    );
    push(Prop::Glow, "Glow", props.glow, format!("{:.0}", props.glow));
    push(
        Prop::Brightness,
        "Brightness",
        props.brightness,
        format!("{:.1}", props.brightness),
    );
    if let Some(sides) = props.sides {
        push(Prop::Sides, "Sides", sides as f32, format!("{sides}"));
    }
    out
}

/// Hit test against the tracks (with a generous vertical grab zone).
/// Returns the row's prop and the normalized position of the click.
pub fn hit(rows: &[Row], px: f32, py: f32) -> Option<(Prop, f32)> {
    rows.iter()
        .find(|r| {
            px >= r.track.x
                && px <= r.track.x + r.track.w
                && (py - (r.track.y + r.track.h * 0.5)).abs() <= r.track.h * 2.0
        })
        .map(|r| (r.prop, ((px - r.track.x) / r.track.w).clamp(0.0, 1.0)))
}
