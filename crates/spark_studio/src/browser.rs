//! The effects browser: the left panel's list of what you can add.
//!
//! A plain list, not cards. A card is for a thing that *exists* and has
//! state — a layer, an effect you already added. This is a menu of kinds,
//! and a menu row is a name you click, so it's a name you click.
//!
//! Drag a row onto a layer card to put that effect on that layer. The drag
//! names its target explicitly, which is what lets it work regardless of
//! what happens to be selected.

use spark_render::Viewport;
use spark_ui::{UiRect, surfaces, theme};

use crate::fx::{EffectKind, KINDS};

/// Row pitch, logical px. Tall enough to hit without aiming.
pub const ROW_H: f32 = 38.0;
/// The caption strip above the list.
pub const HEAD_H: f32 = 32.0;
/// Inset from the panel edge.
const PAD: f32 = 10.0;

pub struct Row {
    pub kind: EffectKind,
    pub row: Viewport,
    pub label_pos: [f32; 2],
}

pub struct Browser {
    pub caption_pos: [f32; 2],
    pub rows: Vec<Row>,
}

pub fn build(panel: Viewport, scale: f32) -> Browser {
    let x = panel.x + PAD * scale;
    let w = (panel.w - PAD * 2.0 * scale).max(1.0);
    let mut y = panel.y + PAD * scale;
    let caption_pos = [x, y + 6.0 * scale];
    y += HEAD_H * scale;
    let rows = KINDS
        .into_iter()
        .map(|kind| {
            let row = Viewport {
                x,
                y,
                w,
                h: ROW_H * scale,
            };
            y += ROW_H * scale;
            Row {
                kind,
                row,
                label_pos: [x + 10.0 * scale, row.y + 8.0 * scale],
            }
        })
        .collect();
    Browser { caption_pos, rows }
}

/// The kind under the cursor, if any.
pub fn hit(b: &Browser, px: f32, py: f32) -> Option<EffectKind> {
    b.rows
        .iter()
        .find(|r| r.row.contains(px, py))
        .map(|r| r.kind)
}

/// The list. The row being hovered or dragged takes the hover wash — the
/// dragged one stays lit so it's clear what's in flight.
pub fn rects(b: &Browser, scale: f32, lit: Option<EffectKind>) -> Vec<UiRect> {
    b.rows
        .iter()
        .filter(|r| lit == Some(r.kind))
        .map(|r| surfaces().hover.rect(r.row, scale))
        .collect()
}

/// A gold outline around the card a dragged effect would land on. Drag
/// with no target feedback is a guess, so this isn't optional.
pub fn drop_rect(card: Viewport, scale: f32) -> UiRect {
    UiRect::region_rounded(card, [0.0, 0.0, 0.0, 0.0], 12.0 * scale)
        .stroke(2.5 * scale, theme().accent)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn panel() -> Viewport {
        Viewport {
            x: 0.0,
            y: 100.0,
            w: 380.0,
            h: 900.0,
        }
    }

    /// Every kind the editor knows is offered, in the order `KINDS` lists
    /// them — a browser that hides a kind is a kind nobody can reach.
    #[test]
    fn every_kind_is_listed() {
        let b = build(panel(), 1.0);
        let offered: Vec<EffectKind> = b.rows.iter().map(|r| r.kind).collect();
        assert_eq!(offered, KINDS.to_vec());
    }

    /// Rows stack without overlapping and stay inside the panel.
    #[test]
    fn rows_stack_inside_the_panel() {
        for scale in [1.0f32, 1.4] {
            let p = panel();
            let b = build(p, scale);
            let mut bottom = p.y;
            for r in &b.rows {
                assert!(r.row.y >= bottom - 0.5, "{:?} overlaps above", r.kind);
                assert!(r.row.x >= p.x && r.row.x + r.row.w <= p.x + p.w + 0.5);
                assert!(r.row.y + r.row.h <= p.y + p.h, "{:?} ran off", r.kind);
                bottom = r.row.y + r.row.h;
            }
        }
    }

    /// The browser must not overlap the tool strip above it. Both live in
    /// the left column, and nobody who can run this can look.
    #[test]
    fn the_list_clears_the_tool_strip() {
        for scale in [1.0f32, 1.4] {
            let l = spark_ui::Layout::compute(3840, 2160, scale, 360.0);
            let b = build(l.left, scale);
            let strip_bottom = l.tools.y + l.tools.h;
            for r in &b.rows {
                assert!(
                    r.row.y >= strip_bottom,
                    "scale {scale}: {:?} is under the tool strip",
                    r.kind
                );
            }
            assert!(
                b.caption_pos[1] >= strip_bottom,
                "scale {scale}: the caption is under the tool strip"
            );
        }
    }

    /// A click lands on the row it looks like it landed on, and misses
    /// everywhere there isn't one.
    #[test]
    fn hit_testing_matches_the_rows() {
        let b = build(panel(), 1.0);
        for r in &b.rows {
            let mid = (r.row.x + r.row.w * 0.5, r.row.y + r.row.h * 0.5);
            assert_eq!(hit(&b, mid.0, mid.1), Some(r.kind));
        }
        assert_eq!(hit(&b, 5.0, 105.0), None, "the caption is not a row");
        let last = b.rows.last().unwrap().row;
        assert_eq!(
            hit(&b, last.x, last.y + last.h + 20.0),
            None,
            "below the list"
        );
    }
}
