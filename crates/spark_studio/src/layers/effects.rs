//! The effect stack, as rows on a layer card.
//!
//! A shape's own settings are what it *is*; everything below the EFFECTS
//! header is what you chose to add to it. Nothing appears here that wasn't
//! asked for — that is the entire payoff of effects being a list rather
//! than a permanent set of fields.
//!
//! Parameters are always visible. There are only ever as many rows as
//! effects you added, so there is little to hide, and one less click
//! between the knob and the hand.

use spark_render::Viewport;

use crate::fx::{EffectKind, KINDS, Stack};

/// Header row height (the EFFECTS caption and its `+`), logical px.
pub const HEAD_H: f32 = 40.0;
/// One effect's title row — name, eye, remove.
pub const ROW_H: f32 = 40.0;
/// One parameter row under an effect.
pub const PARAM_H: f32 = 50.0;
/// One row of the inline "add an effect" picker.
pub const PICK_H: f32 = 40.0;

/// One parameter of one effect, laid out.
pub struct FxParam {
    /// Which effect it belongs to, and which of its parameters.
    pub id: u32,
    pub param: u8,
    pub label: &'static str,
    pub label_pos: [f32; 2],
    pub track: Viewport,
    /// Normalized slider position.
    pub t: f32,
    pub value: String,
    /// A curve drives this parameter — the readout goes gold.
    pub keyed: bool,
}

/// One effect on the layer.
pub struct FxRow {
    pub id: u32,
    pub label: &'static str,
    pub label_pos: [f32; 2],
    /// The title strip — name, eye, remove.
    pub head: Viewport,
    pub eye: Viewport,
    pub remove: Viewport,
    /// Turned off keeps its settings; the eye says which.
    pub on: bool,
    pub params: Vec<FxParam>,
}

/// The whole EFFECTS section of one card.
pub struct FxBlock {
    /// The header strip carrying the caption and the `+`.
    pub head: Viewport,
    pub caption_pos: [f32; 2],
    /// The `+` that opens the inline picker.
    pub add: Viewport,
    pub rows: Vec<FxRow>,
    /// Kinds the layer hasn't got yet, listed while the picker is open.
    /// Inline rather than a floating menu: it scrolls with the card it
    /// belongs to and needs no separate overlay layer.
    pub picks: Vec<(EffectKind, Viewport, [f32; 2])>,
}

/// Lay the section out, advancing `cy` past it.
pub fn block(
    stack: &Stack,
    keyed: &dyn Fn(u32, u8) -> bool,
    x: f32,
    w: f32,
    scale: f32,
    picking: bool,
    cy: &mut f32,
) -> FxBlock {
    let head = Viewport {
        x,
        y: *cy,
        w,
        h: HEAD_H * scale,
    };
    let btn = 28.0 * scale;
    let add = Viewport {
        x: x + w - btn,
        y: *cy + (HEAD_H * scale - btn) * 0.5,
        w: btn,
        h: btn,
    };
    let caption_pos = [x, *cy + 9.0 * scale];
    *cy += HEAD_H * scale;

    let mut rows = Vec::new();
    for e in &stack.effects {
        let row = Viewport {
            x,
            y: *cy,
            w,
            h: ROW_H * scale,
        };
        let side = 26.0 * scale;
        let gap = 8.0 * scale;
        let remove = Viewport {
            x: x + w - side,
            y: *cy + (ROW_H * scale - side) * 0.5,
            w: side,
            h: side,
        };
        let eye = Viewport {
            x: remove.x - side - gap,
            ..remove
        };
        let label_pos = [x + 4.0 * scale, *cy + 9.0 * scale];
        *cy += ROW_H * scale;

        let specs = e.kind.params();
        let params = specs
            .iter()
            .enumerate()
            .map(|(k, spec)| {
                let p = FxParam {
                    id: e.id,
                    param: k as u8,
                    label: spec.name,
                    label_pos: [x + 14.0 * scale, *cy],
                    track: Viewport {
                        x: x + 14.0 * scale,
                        y: *cy + 28.0 * scale,
                        w: (w - 28.0 * scale).max(1.0),
                        h: 10.0 * scale,
                    },
                    t: ((e.get(k) - spec.min) / (spec.max - spec.min).max(1e-6)).clamp(0.0, 1.0),
                    value: format!("{:.2}", e.get(k)),
                    keyed: keyed(e.id, k as u8),
                };
                *cy += PARAM_H * scale;
                p
            })
            .collect();
        rows.push(FxRow {
            id: e.id,
            label: e.kind.label(),
            label_pos,
            head: row,
            eye,
            remove,
            on: e.on,
            params,
        });
    }

    // The picker lists only what the layer hasn't got — offering to add a
    // second Glow would be offering something that can't happen.
    let mut picks = Vec::new();
    if picking {
        for kind in KINDS {
            if stack.find_kind(kind).is_some() {
                continue;
            }
            let row = Viewport {
                x,
                y: *cy,
                w,
                h: PICK_H * scale,
            };
            picks.push((kind, row, [x + 14.0 * scale, *cy + 9.0 * scale]));
            *cy += PICK_H * scale;
        }
    }
    FxBlock {
        head,
        caption_pos,
        add,
        rows,
        picks,
    }
}

impl FxBlock {
    /// The total height a block of this shape occupies, for the card's own
    /// height before anything is laid out. Used by the layout test, and by
    /// the browser's drop preview when that lands.
    #[allow(dead_code)]
    pub fn height(stack: &Stack, scale: f32, picking: bool) -> f32 {
        let params: usize = stack.effects.iter().map(|e| e.kind.params().len()).sum();
        let picks = if picking {
            KINDS
                .into_iter()
                .filter(|&k| stack.find_kind(k).is_none())
                .count()
        } else {
            0
        };
        (HEAD_H
            + ROW_H * stack.effects.len() as f32
            + PARAM_H * params as f32
            + PICK_H * picks as f32)
            * scale
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stacked() -> Stack {
        let mut s = Stack::default();
        let g = s.add(EffectKind::Glow, s.next_id());
        s.find_mut(g).unwrap().set(0, 100.0);
        s.add(EffectKind::Gradient, s.next_id());
        s
    }

    fn laid(stack: &Stack, picking: bool) -> (FxBlock, f32) {
        let mut cy = 0.0;
        let b = block(stack, &|_, _| false, 0.0, 400.0, 1.0, picking, &mut cy);
        (b, cy)
    }

    /// The layout has to agree with the height the card reserved for it, or
    /// rows draw outside the card they belong to.
    #[test]
    fn the_reserved_height_matches_what_is_laid_out() {
        for picking in [false, true] {
            for stack in [Stack::default(), stacked()] {
                let (_, used) = laid(&stack, picking);
                assert!(
                    (used - FxBlock::height(&stack, 1.0, picking)).abs() < 0.5,
                    "reserved {} but used {used}",
                    FxBlock::height(&stack, 1.0, picking)
                );
            }
        }
    }

    /// An empty stack is still a header with a `+` — otherwise there is no
    /// way to add the first effect from the card.
    #[test]
    fn an_empty_stack_still_offers_the_add_button() {
        let (b, _) = laid(&Stack::default(), false);
        assert!(b.rows.is_empty());
        assert!(b.add.w > 0.0 && b.add.h > 0.0);
    }

    /// Every effect brings exactly its own declared parameters, so a new
    /// kind needs no layout code of its own.
    #[test]
    fn each_effect_lays_out_its_own_parameters() {
        let stack = stacked();
        let (b, _) = laid(&stack, false);
        assert_eq!(b.rows.len(), 2);
        for (row, e) in b.rows.iter().zip(&stack.effects) {
            assert_eq!(row.params.len(), e.kind.params().len(), "{}", row.label);
            for (p, spec) in row.params.iter().zip(e.kind.params()) {
                assert_eq!(p.label, spec.name);
                assert_eq!(p.id, e.id, "a parameter points at the wrong effect");
            }
        }
        // Glow at 100 of 0..200 sits halfway along its track.
        assert!((b.rows[0].params[0].t - 0.5).abs() < 1e-3);
    }

    /// The picker offers what you haven't got. Listing a kind the layer
    /// already carries would offer something that can't happen.
    #[test]
    fn the_picker_hides_kinds_already_on_the_layer() {
        let (b, _) = laid(&stacked(), true);
        let offered: Vec<EffectKind> = b.picks.iter().map(|&(k, _, _)| k).collect();
        assert!(!offered.contains(&EffectKind::Glow));
        assert!(!offered.contains(&EffectKind::Gradient));
        assert!(offered.contains(&EffectKind::Additive));
    }

    /// Rows never overlap, and every control stays inside its own row.
    #[test]
    fn rows_and_their_controls_do_not_overlap() {
        let (b, _) = laid(&stacked(), true);
        let mut bottom = b.head.y + b.head.h;
        for row in &b.rows {
            assert!(row.head.y >= bottom - 0.5, "{} overlaps above", row.label);
            assert!(
                row.eye.x + row.eye.w <= row.remove.x + 0.5,
                "eye hits remove"
            );
            for ctl in [row.eye, row.remove] {
                assert!(
                    ctl.y >= row.head.y - 0.5 && ctl.y + ctl.h <= row.head.y + row.head.h + 0.5,
                    "{} control escapes its row",
                    row.label
                );
            }
            bottom = row.head.y + row.head.h;
            for p in &row.params {
                assert!(p.track.y >= bottom - 0.5, "{} slider overlaps", p.label);
                bottom = bottom.max(p.track.y + p.track.h);
            }
        }
    }
}
