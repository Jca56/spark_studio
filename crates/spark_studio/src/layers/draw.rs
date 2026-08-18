//! Card furniture: the rects behind the layer list — card plates, borders,
//! folder headers, icons, cog, scrub wells, slider tracks, toggles, gradient
//! chips. Text is chrome's job.

use spark_render::Viewport;
use spark_ui::{UiRect, surfaces, theme};

use super::{CardHit, CardTab, Cards, EditField};

/// A number box, at rest or being typed into.
///
/// The focused one lifts a rung and takes an *inset* gold edge: the border
/// alone was the only feedback and read as nothing, and a ring hung outside
/// made the box look like a second, larger one had appeared on top of it.
fn field_well(v: spark_render::Viewport, scale: f32, focused: bool) -> UiRect {
    let well = surfaces().well;
    if focused {
        well.filled(theme().card)
            .edge(2.0, theme().accent)
            .rect(v, scale)
    } else {
        well.rect(v, scale)
    }
}

pub fn rects(
    cards: &Cards,
    scale: f32,
    grad_edit_b: bool,
    hover: Option<CardHit>,
    editing: Option<EditField>,
) -> Vec<UiRect> {
    let th = theme();
    let mut out = Vec::new();
    for f in &cards.folders {
        // Folder headers read as headers, not cards: a flatter plate with a
        // gold spine down the left, so the indented members below clearly
        // belong to it.
        let head = surfaces().header;
        out.push(if f.selected {
            head.edged(f.row, scale, th.accent)
        } else {
            head.rect(f.row, scale)
        });
        out.push(UiRect::region_rounded(
            Viewport {
                x: f.row.x,
                y: f.row.y + 6.0 * scale,
                w: 5.0 * scale,
                h: f.row.h - 12.0 * scale,
            },
            th.accent,
            2.5 * scale,
        ));
        // Disclosure: a minus when open, a plus when collapsed — plain bars,
        // the same trick the zoom bar uses, so no new shader glyph.
        out.push(UiRect::region_rounded(f.disclose, th.card, 8.0 * scale));
        let len = f.disclose.w * 0.46;
        let thick = 3.5 * scale;
        out.push(UiRect::region_rounded(
            Viewport {
                x: f.disclose.x + (f.disclose.w - len) * 0.5,
                y: f.disclose.y + (f.disclose.h - thick) * 0.5,
                w: len,
                h: thick,
            },
            th.icon_hover,
            thick * 0.5,
        ));
        if f.collapsed {
            out.push(UiRect::region_rounded(
                Viewport {
                    x: f.disclose.x + (f.disclose.w - thick) * 0.5,
                    y: f.disclose.y + (f.disclose.h - len) * 0.5,
                    w: thick,
                    h: len,
                },
                th.icon_hover,
                thick * 0.5,
            ));
        }
        out.push(UiRect::icon_sized(
            f.eye,
            if f.hidden {
                spark_ui::ICON_EYE_OFF
            } else {
                spark_ui::ICON_EYE
            },
            1.8 * scale,
            if f.hidden { th.text_off } else { th.icon },
            0.36,
        ));
        // The folder's own X/Y/R/S wells, same sunken look as a card's.
        for sf in &f.scrubs {
            out.push(field_well(
                sf.rect,
                scale,
                editing == Some(EditField::Folder(f.id, sf.prop)),
            ));
        }
    }
    for lr in &cards.rows {
        // Every card wears its own border so the rows read as separate
        // objects across the gaps; selection just swaps grey for gold.
        let card = surfaces().card;
        out.push(if lr.selected {
            card.edged(lr.row, scale, th.accent)
        } else {
            card.rect(lr.row, scale)
        });
        // The kind glyph wears the shape's color (dimmed when hidden).
        let mut icon_col = [lr.rgb[0], lr.rgb[1], lr.rgb[2], 1.0];
        if lr.hidden {
            icon_col = [
                icon_col[0] * 0.35,
                icon_col[1] * 0.35,
                icon_col[2] * 0.35,
                1.0,
            ];
        }
        let mut icon = UiRect::icon_sized(lr.icon, lr.icon_kind, 2.5 * scale, icon_col, 0.34);
        icon.icon[2] = lr.icon_sides;
        out.push(icon);
        // Wash first, glyph on top: these are painted in order, so a hover
        // pushed after the icon covers it up.
        if hover == Some(CardHit::Eye(lr.index)) {
            out.push(surfaces().hover.rect(lr.eye, scale));
        }
        out.push(UiRect::icon_sized(
            lr.eye,
            if lr.hidden {
                spark_ui::ICON_EYE_OFF
            } else {
                spark_ui::ICON_EYE
            },
            1.8 * scale,
            if lr.hidden { th.text_off } else { th.icon },
            0.36,
        ));
        if let Some(cog) = lr.cog {
            let open = lr
                .detail
                .as_ref()
                .is_some_and(|d| d.tab == CardTab::Settings);
            if open || hover == Some(CardHit::Cog(lr.index)) {
                out.push(surfaces().hover.rect(cog, scale));
            }
            out.push(UiRect::icon_sized(
                cog,
                spark_ui::ICON_GEAR,
                0.0,
                if open { th.accent } else { th.icon },
                0.40,
            ));
        }
        if let Some(tab) = lr.fx_tab {
            // Lit when its own tab is the one showing, so the pair of
            // buttons says which half of the card you're looking at.
            let live = lr
                .detail
                .as_ref()
                .is_some_and(|d| d.tab == CardTab::Effects);
            if live || hover == Some(CardHit::FxTab(lr.index)) {
                out.push(surfaces().hover.rect(tab, scale));
            }
            out.push(UiRect::icon_sized(
                tab,
                spark_ui::ICON_STARS,
                0.0,
                if live { th.accent } else { th.icon },
                0.40,
            ));
        }
        for f in &lr.scrubs {
            // A sunken well so each field reads as its own box; the one
            // A sunken box so each field reads as its own; the focused one
            // lifts a rung and takes an inset gold edge.
            out.push(field_well(
                f.rect,
                scale,
                editing == Some(EditField::Shape(lr.index, f.prop)),
            ));
        }
        if let Some(d) = &lr.detail {
            // The settings block is its own surface — a card inside a card.
            // First, so everything below is drawn *on* it.
            out.push(surfaces().card_inner.rect(d.panel, scale));
            for row in &d.sliders {
                out.extend(spark_ui::Slider::rects(row.track, row.t));
            }
            // One card per effect, its sliders inside it.
            for row in &d.fx {
                // A rung *up* from the block it sits on, the same way a
                // card sits on a panel: an effect is an object on the
                // settings surface, not a recess in it. It used to borrow
                // the folder-header grey, which is now the same value the
                // block itself carries — it would have vanished into it.
                out.push(surfaces().card.rect(row.card, scale));
                if hover == Some(CardHit::FxToggle(lr.index, row.id)) {
                    out.push(surfaces().hover.rect(row.eye, scale));
                }
                out.push(UiRect::icon_sized(
                    row.eye,
                    if row.on {
                        spark_ui::ICON_EYE
                    } else {
                        spark_ui::ICON_EYE_OFF
                    },
                    0.0,
                    if row.on { th.icon } else { th.text_off },
                    0.44,
                ));
                if hover == Some(CardHit::FxRemove(lr.index, row.id)) {
                    out.push(surfaces().hover.rect(row.remove, scale));
                }
                // Red: removing an effect is the one destructive control on
                // the card, and it should look like it.
                out.push(UiRect::icon_sized(
                    row.remove,
                    spark_ui::ICON_X,
                    0.0,
                    th.red,
                    0.38,
                ));
                for p in &row.params {
                    out.extend(spark_ui::Slider::rects(p.track, p.t));
                }
            }
            if let Some(f) = &d.form {
                out.extend(f.seg.rects(f.active));
            }
            for t in [d.style.as_ref(), d.blend.as_ref(), d.grad.as_ref()]
                .into_iter()
                .flatten()
            {
                out.extend(t.seg.rects(t.on as usize));
            }
            if let Some(chips) = &d.chips {
                for (k, c) in chips.iter().enumerate() {
                    let rgb = if k == 0 { lr.rgb } else { d.rgb2 };
                    // The armed endpoint (the color home's target) rings
                    // gold — outside the chip, so the color stays readable.
                    let chip =
                        UiRect::region_rounded(*c, [rgb[0], rgb[1], rgb[2], 1.0], 7.0 * scale);
                    out.push(if lr.selected && (k == 1) == grad_edit_b {
                        chip.stroke_outer(3.0 * scale, th.accent)
                    } else {
                        chip
                    });
                }
            }
        }
    }
    out
}
