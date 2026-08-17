//! Card furniture: the rects behind the layer list — card plates, borders,
//! folder headers, icons, cog, scrub wells, slider tracks, toggles, gradient
//! chips. Text is chrome's job.

use spark_render::Viewport;
use spark_ui::{UiRect, surfaces, theme};

use super::{Cards, Prop};

pub fn rects(
    cards: &Cards,
    scale: f32,
    grad_edit_b: bool,
    cog_hover: Option<usize>,
    editing: Option<(usize, Prop)>,
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
            out.push(surfaces().well.rect(sf.rect, scale));
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
            let open = lr.detail.is_some();
            if open || cog_hover == Some(lr.index) {
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
        for f in &lr.scrubs {
            // A sunken well so each field reads as its own box; the one
            // being text-edited rings gold. The ring sits outside the well
            // so it never crops the number inside.
            let well = surfaces().well;
            out.push(if editing == Some((lr.index, f.prop)) {
                well.ringed(f.rect, scale, 2.0, th.accent)
            } else {
                well.rect(f.rect, scale)
            });
        }
        if let Some(d) = &lr.detail {
            for row in &d.sliders {
                out.extend(spark_ui::Slider::rects(row.track, row.t));
            }
            for t in [d.style.as_ref(), Some(&d.blend), Some(&d.grad)]
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
