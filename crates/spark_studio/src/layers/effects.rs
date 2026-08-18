//! The effect stack, as cards on a layer's Effects tab.
//!
//! A shape's own settings are what it *is*; the Effects tab is what you
//! chose to add to it. Nothing appears here that wasn't asked for — that is
//! the entire payoff of effects being a list rather than a permanent set of
//! fields.
//!
//! One card per effect, its parameters *inside* the card rather than
//! floating under a thin title strip, so an effect reads as one object you
//! can point at.

use spark_render::Viewport;

use crate::fx::Stack;

/// Padding inside an effect card, logical px.
const PAD: f32 = 10.0;
/// The title strip inside a card — name, eye, remove.
const HEAD_H: f32 = 38.0;
/// One parameter: its label row plus the slider under it.
const PARAM_H: f32 = 58.0;
/// Between one effect card and the next.
const GAP: f32 = 10.0;

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
    /// Where the readout's right edge sits — beside the track, not above.
    pub value_right: f32,
    /// A curve drives this parameter — the readout goes gold.
    pub keyed: bool,
}

/// One effect on the layer, as a card.
pub struct FxRow {
    pub id: u32,
    pub label: &'static str,
    pub label_pos: [f32; 2],
    /// The whole card, parameters included.
    pub card: Viewport,
    pub eye: Viewport,
    pub remove: Viewport,
    /// Turned off keeps its settings; the eye says which.
    pub on: bool,
    pub params: Vec<FxParam>,
}

/// The height one effect's card needs.
fn card_h(params: usize, scale: f32) -> f32 {
    (PAD * 2.0 + HEAD_H + PARAM_H * params as f32) * scale
}

/// Lay the tab out, advancing `cy` past it.
pub fn block(
    stack: &Stack,
    keyed: &dyn Fn(u32, u8) -> bool,
    x: f32,
    w: f32,
    scale: f32,
    cy: &mut f32,
) -> Vec<FxRow> {
    let mut rows = Vec::new();
    for e in &stack.effects {
        let specs = e.kind.params();
        let card = Viewport {
            x,
            y: *cy,
            w,
            h: card_h(specs.len(), scale),
        };
        let inner_x = x + PAD * scale;
        let inner_w = (w - PAD * 2.0 * scale).max(1.0);
        let side = 26.0 * scale;
        let gap = 8.0 * scale;
        let head_y = *cy + PAD * scale;
        let remove = Viewport {
            x: inner_x + inner_w - side,
            y: head_y + (HEAD_H * scale - side) * 0.5,
            w: side,
            h: side,
        };
        let eye = Viewport {
            x: remove.x - side - gap,
            ..remove
        };
        let label_pos = [inner_x, head_y + 8.0 * scale];

        let mut py = head_y + HEAD_H * scale;
        let params = specs
            .iter()
            .enumerate()
            .map(|(k, spec)| {
                let p = FxParam {
                    id: e.id,
                    param: k as u8,
                    label: spec.name,
                    label_pos: [inner_x, py],
                    track: Viewport {
                        x: inner_x,
                        y: py + 32.0 * scale,
                        w: (inner_w - (super::VALUE_W + super::VALUE_GAP) * scale).max(1.0),
                        h: 10.0 * scale,
                    },
                    t: ((e.get(k) - spec.min) / (spec.max - spec.min).max(1e-6)).clamp(0.0, 1.0),
                    value: format!("{:.2}", e.get(k)),
                    value_right: inner_x + inner_w,
                    keyed: keyed(e.id, k as u8),
                };
                py += PARAM_H * scale;
                p
            })
            .collect();
        rows.push(FxRow {
            id: e.id,
            label: e.kind.label(),
            label_pos,
            card,
            eye,
            remove,
            on: e.on,
            params,
        });
        *cy += card.h + GAP * scale;
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fx::EffectKind;

    fn stacked() -> Stack {
        let mut s = Stack::default();
        let g = s.add(EffectKind::Glow, s.next_id());
        s.find_mut(g).unwrap().set(0, 100.0);
        s.add(EffectKind::Gradient, s.next_id());
        s
    }

    fn laid(stack: &Stack) -> (Vec<FxRow>, f32) {
        let mut cy = 0.0;
        let r = block(stack, &|_, _| false, 0.0, 400.0, 1.0, &mut cy);
        (r, cy)
    }

    /// An empty stack lays out nothing and takes no height — an Effects tab
    /// with no effects is genuinely empty, not a block of dead space.
    #[test]
    fn an_empty_stack_takes_no_room() {
        let (rows, used) = laid(&Stack::default());
        assert!(rows.is_empty());
        assert_eq!(used, 0.0);
    }

    /// Every effect brings exactly its own declared parameters, so a new
    /// kind needs no layout code of its own.
    #[test]
    fn each_effect_lays_out_its_own_parameters() {
        let stack = stacked();
        let (rows, _) = laid(&stack);
        assert_eq!(rows.len(), 2);
        for (row, e) in rows.iter().zip(&stack.effects) {
            assert_eq!(row.params.len(), e.kind.params().len(), "{}", row.label);
            for (p, spec) in row.params.iter().zip(e.kind.params()) {
                assert_eq!(p.label, spec.name);
                assert_eq!(p.id, e.id, "a parameter points at the wrong effect");
            }
        }
        // Glow at 100 of 0..200 sits halfway along its track.
        assert!((rows[0].params[0].t - 0.5).abs() < 1e-3);
    }

    /// The parameters live *inside* the effect's own card — a slider that
    /// escapes its card reads as belonging to the next effect down.
    #[test]
    fn parameters_stay_inside_their_card() {
        let (rows, _) = laid(&stacked());
        for row in &rows {
            for ctl in [row.eye, row.remove] {
                assert!(
                    ctl.y >= row.card.y && ctl.y + ctl.h <= row.card.y + row.card.h,
                    "{}: a header control escaped the card",
                    row.label
                );
            }
            assert!(
                row.eye.x + row.eye.w <= row.remove.x + 0.5,
                "eye hits remove"
            );
            for p in &row.params {
                assert!(
                    p.label_pos[1] >= row.card.y
                        && p.track.y + p.track.h <= row.card.y + row.card.h,
                    "{}: {} escaped the card",
                    row.label,
                    p.label
                );
                assert!(
                    p.track.x >= row.card.x && p.track.x + p.track.w <= row.card.x + row.card.w,
                    "{}: {} runs past the card's edge",
                    row.label,
                    p.label
                );
            }
        }
    }

    /// Cards don't overlap each other.
    #[test]
    fn effect_cards_do_not_overlap() {
        let (rows, _) = laid(&stacked());
        for pair in rows.windows(2) {
            assert!(
                pair[1].card.y >= pair[0].card.y + pair[0].card.h - 0.5,
                "{} overlaps {}",
                pair[1].label,
                pair[0].label
            );
        }
    }
}
