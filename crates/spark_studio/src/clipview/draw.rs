//! The clip view's paint: the breadcrumb plate, the target rows, the
//! key strip, the curve and its diamonds. Geometry comes from `page`;
//! text is chrome's job. Same physics as the arrangement — rows are
//! lifted objects, the strip is a recess, the graph is ground.

use spark_render::Viewport;
use spark_ui::{ICON_KEY, UiRect, surfaces, theme};

use super::page::{Hit, KEY, Page, STRIP_KEY};

/// The view's rects, by the batch that clips them: the sidebar's
/// header, the rows under it, and everything on the time axis.
pub struct Rects {
    pub sidebar: Vec<UiRect>,
    pub rows: Vec<UiRect>,
    pub axis: Vec<UiRect>,
}

fn square(cx: f32, cy: f32, side: f32) -> Viewport {
    Viewport {
        x: cx - side * 0.5,
        y: cy - side * 0.5,
        w: side,
        h: side,
    }
}

/// A keyframe diamond centred on a point: a white ring behind the
/// selected one, an accent face, and a dark core on a linear key so it
/// reads hollow — the lane marker's old language.
fn diamond(
    out: &mut Vec<UiRect>,
    at: [f32; 2],
    side: f32,
    linear: bool,
    selected: bool,
    hot: bool,
) {
    let t = theme();
    if selected {
        out.push(UiRect::icon_sized(
            square(at[0], at[1], side * 1.45),
            ICON_KEY,
            0.0,
            t.text,
            0.5,
        ));
    }
    let face = if hot { t.icon_hover } else { t.accent };
    let grow = if hot && !selected { 1.15 } else { 1.0 };
    out.push(UiRect::icon_sized(
        square(at[0], at[1], side * grow),
        ICON_KEY,
        0.0,
        face,
        0.5,
    ));
    if linear {
        out.push(UiRect::icon_sized(
            square(at[0], at[1], side * 0.5),
            ICON_KEY,
            0.0,
            t.well_deep,
            0.5,
        ));
    }
}

pub fn rects(page: &Page, over: Option<Hit>) -> Rects {
    let t = theme();
    let m = surfaces();
    let s = page.scale;
    // The breadcrumb: a plate with a left-pointing chevron and the
    // object's name — the way back.
    let plate = m.plate.at_radius(8.0);
    let mut sidebar = vec![if over == Some(Hit::Back) {
        plate.filled(t.button_hover).rect(page.header, s)
    } else {
        plate.rect(page.header, s)
    }];
    let side = 30.0 * s;
    sidebar.push(
        UiRect::chevron(
            Viewport {
                x: page.header.x + 8.0 * s,
                y: page.header.y + (page.header.h - side) * 0.5,
                w: side,
                h: side,
            },
            3.0 * s,
            t.icon_hover,
            0.32,
        )
        .rotate(0.25),
    );
    // Target rows: cards like the arrangement's, the chosen one purple
    // under a gold ring.
    let card = m.card.at_radius(8.0);
    let rows = page
        .rows
        .iter()
        .enumerate()
        .map(|(k, r)| {
            if r.selected {
                card.filled(t.accent_alt_bg)
                    .rect(r.cell, s)
                    .stroke_outer(2.0 * s, t.accent)
            } else if over == Some(Hit::Row(k)) {
                card.filled(t.button_hover).rect(r.cell, s)
            } else {
                card.rect(r.cell, s)
            }
        })
        .collect();
    // The strip: a recess under the ruler, a diamond per moment.
    let mut axis = vec![
        m.well
            .filled(t.well_deep)
            .at_radius(0.0)
            .edge(0.0, [0.0; 4])
            .rect(page.strip, s),
        UiRect::region(
            Viewport {
                x: page.strip.x,
                y: page.strip.y + page.strip.h - 1.5 * s,
                w: page.strip.w,
                h: 1.5 * s,
            },
            [1.0, 1.0, 1.0, 0.10],
        ),
    ];
    let sy = page.strip.y + page.strip.h * 0.5;
    for (k, d) in page.strip_dots.iter().enumerate() {
        diamond(
            &mut axis,
            [d.x, sy],
            STRIP_KEY * s,
            false,
            d.selected,
            over == Some(Hit::StripKey(k)),
        );
    }
    if page.target.is_none() {
        return Rects {
            sidebar,
            rows,
            axis,
        };
    }
    // Value rules across the graph: its top, middle and bottom.
    let (lo, hi) = page.span;
    for v in [hi, (lo + hi) * 0.5, lo] {
        axis.push(UiRect::region(
            Viewport {
                x: page.graph.x,
                y: page.y_of(v) - 0.5 * s,
                w: page.graph.w,
                h: 1.0 * s,
            },
            [1.0, 1.0, 1.0, 0.07],
        ));
    }
    // The curve, in the object's own colour; where it only holds
    // (before the first key, after the last) it fades.
    let [r, g, b] = page.color;
    for &(a, p, inside) in &page.curve {
        axis.push(UiRect::line(
            a,
            p,
            3.0 * s,
            [r, g, b, if inside { 1.0 } else { 0.35 }],
        ));
    }
    for (k, d) in page.keys.iter().enumerate() {
        diamond(
            &mut axis,
            d.at,
            KEY * s,
            d.linear,
            d.selected,
            over == Some(Hit::Key(k)),
        );
    }
    Rects {
        sidebar,
        rows,
        axis,
    }
}
