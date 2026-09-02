//! The arrangement's paint: sidebar row chrome and clip bars. Geometry
//! comes from `build`; text is chrome's job.

use spark_render::Viewport;
use spark_ui::{ICON_CHEVRON, ICON_EYE, ICON_EYE_OFF, UiRect, surfaces, theme};

use super::ArrangeScene;

/// The arrangement's rects: sidebar rows for the lanes batch, clip bars
/// for the axis batch (which clips them to the time axis).
pub fn rects(sc: &ArrangeScene, scale: f32) -> (Vec<UiRect>, Vec<UiRect>) {
    let t = theme();
    let mut lanes_ui = Vec::new();
    // The dragged row draws last, over the rows it passes.
    let mut lifted = Vec::new();
    for (k, tr) in sc.rows.iter().enumerate() {
        let out = if sc.dragged == Some(k) {
            &mut lifted
        } else {
            &mut lanes_ui
        };
        // Track rows are cards — the raised material, selection swapping
        // the face purple under a gold ring; the one being dragged floats.
        let card = surfaces().card.at_radius(8.0);
        out.push(if sc.dragged == Some(k) {
            surfaces()
                .float
                .at_radius(8.0)
                .filled(t.accent_alt_bg)
                .rect(tr.cell, scale)
        } else if tr.selected {
            card.filled(t.accent_alt_bg)
                .rect(tr.cell, scale)
                .stroke_outer(2.0 * scale, t.accent)
        } else {
            card.rect(tr.cell, scale)
        });
        if let Some(d) = tr.disclose {
            out.push(UiRect::icon_sized(d, ICON_CHEVRON, 0.0, t.icon, 0.4));
        }
        if let Some((g, icon, rgb)) = tr.glyph {
            let col = if tr.dim {
                [rgb[0], rgb[1], rgb[2], 0.35]
            } else {
                [rgb[0], rgb[1], rgb[2], 1.0]
            };
            out.push(UiRect::icon_sized(g, icon, 0.0, col, 0.5));
        }
        if let Some(e) = tr.eye {
            let icon = if tr.hidden { ICON_EYE_OFF } else { ICON_EYE };
            let col = if tr.hidden { t.icon } else { t.icon_hover };
            out.push(UiRect::icon_sized(e, icon, 0.0, col, 0.5));
        }
        // An audio row's volume box: a well, like the inspector's fields
        // — drag it up and down; its reading is chrome's to print.
        if let Some((vb, _)) = &tr.volume {
            out.push(surfaces().well.rect(*vb, scale));
        }
    }
    // Where the dragged row will land: a gold line across the sidebar
    // at the seam, then the row itself on top of everything.
    if let (Some(y), Some(tr)) = (sc.drop_y, sc.rows.first()) {
        lanes_ui.push(UiRect::region_rounded(
            Viewport {
                x: tr.cell.x,
                y: y - 1.5 * scale,
                w: tr.cell.w,
                h: 3.0 * scale,
            },
            t.accent,
            1.5 * scale,
        ));
    }
    lanes_ui.extend(lifted);
    let mut axis_ui = Vec::new();
    for c in &sc.clips {
        let r = 8.0 * scale;
        let (fill, edge) = match (c.missing, c.color) {
            (true, _) => (
                [t.red[0] * 0.4, t.red[1] * 0.15, t.red[2] * 0.15, 0.9],
                [t.red[0], t.red[1], t.red[2], 0.8],
            ),
            (false, Some(rgb)) => (
                [rgb[0] * 0.45, rgb[1] * 0.45, rgb[2] * 0.45, 0.55],
                [rgb[0], rgb[1], rgb[2], 0.8],
            ),
            (false, None) => (
                [t.red[0] * 0.55, t.red[1] * 0.35, t.red[2] * 0.35, 0.55],
                [t.red[0], t.red[1], t.red[2], 0.8],
            ),
        };
        let bar = UiRect::region_rounded(c.bar, fill, r);
        axis_ui.push(if c.selected {
            bar.stroke_outer(2.5 * scale, t.accent)
        } else {
            bar.stroke_outer(1.5 * scale, edge)
        });
        for &x in &c.loop_xs {
            axis_ui.push(UiRect::region(
                Viewport {
                    x: x - 0.75 * scale,
                    y: c.bar.y + 3.0 * scale,
                    w: 1.5 * scale,
                    h: c.bar.h - 6.0 * scale,
                },
                [1.0, 1.0, 1.0, 0.30],
            ));
        }
    }
    (lanes_ui, axis_ui)
}

