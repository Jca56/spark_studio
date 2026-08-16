//! Layer list geometry for the left panel: one row per shape, front-most
//! (last drawn) at the top. Pure layout — rendering and clicks live in main.

use spark_render::{Shape, ShapeKind, Viewport};
use spark_ui::{ICON_CIRCLE, ICON_LINE, ICON_PENTAGON, ICON_SQUARE};

use crate::chrome::UI_TEXT;

pub struct LayerRow {
    /// Index into the editor's shape list.
    pub index: usize,
    pub row: Viewport,
    pub chip: Viewport,
    pub icon: Viewport,
    pub icon_kind: f32,
    /// Top-left of the label text (physical px), for 20px × scale text.
    pub label_pos: [f32; 2],
    pub label: String,
    pub rgb: [f32; 3],
    pub selected: bool,
}

fn kind_parts(kind: ShapeKind) -> (f32, &'static str) {
    match kind {
        ShapeKind::Circle => (ICON_CIRCLE, "circle"),
        ShapeKind::Box => (ICON_SQUARE, "box"),
        ShapeKind::Ngon => (ICON_PENTAGON, "polygon"),
        ShapeKind::Line => (ICON_LINE, "line"),
    }
}

pub fn rows(
    panel: Viewport,
    scale: f32,
    shapes: &[Shape],
    selection: Option<usize>,
) -> Vec<LayerRow> {
    let pad = 12.0 * scale;
    let step = 68.0 * scale;
    let mut y = panel.y + pad;
    let mut out = Vec::new();
    for (index, shape) in shapes.iter().enumerate().rev() {
        // No scrolling yet — rows past the panel bottom are still reachable
        // by clicking the shape on the canvas.
        if y + step > panel.y + panel.h - pad {
            break;
        }
        let row = Viewport {
            x: panel.x + pad,
            y,
            w: panel.w - pad * 2.0,
            h: step - 10.0 * scale,
        };
        let chip_side = 28.0 * scale;
        let chip = Viewport {
            x: row.x + 14.0 * scale,
            y: row.y + (row.h - chip_side) * 0.5,
            w: chip_side,
            h: chip_side,
        };
        let icon = Viewport {
            x: chip.x + chip.w + 12.0 * scale,
            y: row.y,
            w: row.h,
            h: row.h,
        };
        let (icon_kind, name) = kind_parts(shape.kind());
        out.push(LayerRow {
            index,
            row,
            chip,
            icon,
            icon_kind,
            // Center the label's 1.2em line box in the card.
            label_pos: [
                icon.x + icon.w + 6.0 * scale,
                row.y + (row.h - UI_TEXT * 1.2 * scale) * 0.5,
            ],
            label: format!("{name} {}", index + 1),
            rgb: shape.rgb(),
            selected: selection == Some(index),
        });
        y += step;
    }
    out
}

pub fn hit(rows: &[LayerRow], px: f32, py: f32) -> Option<usize> {
    rows.iter()
        .find(|r| r.row.contains(px, py))
        .map(|r| r.index)
}
