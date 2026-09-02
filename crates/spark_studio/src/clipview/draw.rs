//! The clip view's paint: the breadcrumb plate, the target rows, the
//! key strip, the curve and its diamonds. Geometry comes from `page`;
//! text is chrome's job. Same physics as the arrangement — rows are
//! lifted objects, the strip is a recess, the graph is ground.

use spark_render::Viewport;
use spark_ui::{ICON_CIRCLE, ICON_KEY, UiRect, surfaces, theme};

use super::page::{Hit, KEY, Page, ROW_GLYPH, STRIP_KEY};

/// The view's rects, by the batch that clips them: the sidebar's
/// header, the rows under it, everything on the time axis, and what
/// goes on the ruler (the loop brace's handle, lit under the cursor).
pub struct Rects {
    pub sidebar: Vec<UiRect>,
    pub rows: Vec<UiRect>,
    pub axis: Vec<UiRect>,
    pub ruler: Vec<UiRect>,
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
/// selected one, an accent face, and — on a *smooth* key — a soft round
/// core, since linear is the default now and wears the plain diamond.
fn diamond(
    out: &mut Vec<UiRect>,
    at: [f32; 2],
    side: f32,
    smooth: bool,
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
    if smooth {
        out.push(UiRect::icon_sized(
            square(at[0], at[1], side * 0.5),
            ICON_CIRCLE,
            0.0,
            t.well_deep,
            0.36,
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
    // Setting rows: cards like the arrangement's, the chosen one purple
    // under a gold ring, a diamond at the left of every one that has
    // keys on this clip.
    let card = m.card.at_radius(8.0);
    let mut rows = Vec::new();
    for (k, r) in page.rows.iter().enumerate() {
        rows.push(if r.selected {
            card.filled(t.accent_alt_bg)
                .rect(r.cell, s)
                .stroke_outer(2.0 * s, t.accent)
        } else if over == Some(Hit::Row(k)) {
            card.filled(t.button_hover).rect(r.cell, s)
        } else {
            card.rect(r.cell, s)
        });
        if r.keyed {
            rows.push(UiRect::icon_sized(
                square(
                    r.cell.x + 20.0 * s,
                    r.cell.y + r.cell.h * 0.5,
                    ROW_GLYPH * s,
                ),
                ICON_KEY,
                0.0,
                t.accent,
                0.5,
            ));
        }
    }
    // The loop brace's end on the ruler: a grip, gold when the cursor
    // is on it.
    let ruler = page
        .loop_end_x
        .map(|x| {
            let hot = over == Some(Hit::LoopEnd);
            let w = if hot { 6.0 } else { 4.0 } * s;
            vec![UiRect::region_rounded(
                Viewport {
                    x: x - w * 0.5,
                    y: page.ruler.y,
                    w,
                    h: page.ruler.h,
                },
                if hot {
                    t.accent
                } else {
                    [t.accent[0], t.accent[1], t.accent[2], 0.55]
                },
                2.0 * s,
            )]
        })
        .unwrap_or_default();
    // What never plays goes dark first, from the strip to the floor;
    // then the strip: a recess under the ruler, a diamond per moment.
    let mut axis: Vec<UiRect> = page
        .wash
        .iter()
        .map(|w| UiRect::region(*w, [0.0, 0.0, 0.0, 0.45]))
        .collect();
    axis.extend([
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
    ]);
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
    if page.curve.is_empty() {
        return Rects {
            sidebar,
            rows,
            axis,
            ruler,
        };
    }
    // Value rules across the graph, a round step apart — where a
    // dragged key's value lands with snap on. Zero reads a touch
    // brighter: the floor most things are measured from.
    for &v in &page.rules {
        let zero = v.abs() < 1e-6;
        axis.push(UiRect::region(
            Viewport {
                x: page.graph.x,
                y: page.y_of(v) - 0.5 * s,
                w: page.graph.w,
                h: 1.0 * s,
            },
            [1.0, 1.0, 1.0, if zero { 0.16 } else { 0.07 }],
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
            !d.linear,
            d.selected,
            over == Some(Hit::Key(k)),
        );
    }
    // The selection band: a gold wash with a hairline edge.
    if let Some(b) = page.band {
        let [r, g, bl, _] = t.accent;
        axis.push(UiRect::region(b, [r, g, bl, 0.10]));
        let e = 1.5 * s;
        for edge in [
            Viewport { x: b.x, y: b.y, w: b.w, h: e },
            Viewport { x: b.x, y: b.y + b.h - e, w: b.w, h: e },
            Viewport { x: b.x, y: b.y, w: e, h: b.h },
            Viewport { x: b.x + b.w - e, y: b.y, w: e, h: b.h },
        ] {
            axis.push(UiRect::region(edge, [r, g, bl, 0.7]));
        }
    }
    Rects {
        sidebar,
        rows,
        axis,
        ruler,
    }
}
