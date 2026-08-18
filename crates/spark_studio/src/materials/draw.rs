//! What the playground paints and writes.
//!
//! Painted from **plain theme colors, never from the values being edited**:
//! a color dialled into invisibility must not take the panel that would undo
//! it down as well. The one exception is a swatch, which of course shows the
//! color it stands for.

use spark_render::Viewport;
use spark_ui::{Slider, UiRect, surfaces, theme};

use super::{MATERIALS, Panel, TABS, TEXT, Tab};

pub fn rects(p: &Panel, scale: f32, pick: usize) -> Vec<UiRect> {
    let t = theme();
    let mut out = Vec::new();
    for (v, (tab, _)) in p.tabs.iter().zip(TABS) {
        let live = tab == p.tab;
        out.push(
            UiRect::region_rounded(*v, if live { t.card } else { t.well }, 8.0 * scale)
                .stroke(2.0 * scale, if live { t.accent } else { t.card_border }),
        );
    }
    for (v, tint) in [(p.print, t.accent), (p.reset, t.card_border)] {
        out.push(UiRect::region_rounded(v, t.card, 8.0 * scale).stroke(2.0 * scale, tint));
    }
    match p.tab {
        Tab::Colors => {
            for c in &p.cells {
                // A translucent swatch is painted over a light-to-dark ramp,
                // so how much of the ramp survives *is* the alpha. A
                // checkerboard would read better still, but its squares
                // can't be clipped to a rounded corner in one quad.
                if c.color[3] < 1.0 {
                    out.push(
                        UiRect::region_rounded(c.swatch, t.checker[1], 5.0 * scale)
                            .gradient_h(t.checker[0]),
                    );
                }
                // The swatch is the one thing here that wears an edited
                // value; a light ring keeps a near-black one visible.
                out.push(
                    UiRect::region_rounded(c.swatch, c.color, 5.0 * scale).stroke(
                        1.5 * scale,
                        if c.editing { t.accent } else { t.card_border },
                    ),
                );
                // The code sits in a box, because a box is what says "type
                // in me". Without it the tab read as a list you could only
                // look at.
                out.push(UiRect::region_rounded(c.field, t.well, 6.0 * scale).stroke(
                    if c.editing { 2.5 * scale } else { 1.5 * scale },
                    if c.editing { t.accent } else { t.card_border },
                ));
            }
        }
        Tab::Depth => {
            for (i, v) in p.picks.iter().enumerate() {
                let live = i == pick;
                out.push(
                    UiRect::region_rounded(*v, if live { t.card } else { t.well }, 8.0 * scale)
                        .stroke(2.0 * scale, if live { t.accent } else { t.card_border }),
                );
            }
            for row in &p.rows {
                out.extend(Slider::rects(row.track, row.t));
            }
        }
    }
    let _ = surfaces();
    out
}

pub fn labels(text: &mut spark_text::Text, p: &Panel, area: Viewport, scale: f32, res: (u32, u32)) {
    use spark_text::Text;
    let th = theme();
    let size = TEXT * scale;
    let line = Text::line_height(size);
    let clip = (area.y, area.y + area.h);
    let vis = |y: f32| y >= clip.0 && y + line <= clip.1;
    let centred = |text: &mut Text, s: &str, v: Viewport, col: [f32; 4], res| {
        let w = text.measure(s, size);
        text.label(
            s,
            size,
            v.x + (v.w - w) * 0.5,
            v.y + (v.h - line) * 0.5,
            col,
            v.w,
            res,
        );
    };
    for (v, (tab, name)) in p.tabs.iter().zip(TABS) {
        let col = if tab == p.tab { th.accent } else { th.text_dim };
        centred(text, name, *v, col, res);
    }
    centred(text, "Print", p.print, th.text, res);
    centred(text, "Reset", p.reset, th.text, res);
    // What to actually do with the tab you are looking at. It read as an
    // inert list of colours until this said otherwise.
    let hint = match p.tab {
        Tab::Colors => {
            "Click a code and type a new one \u{2014} RRGGBB, or RRGGBBAA for transparency"
        }
        Tab::Depth => "Pick a material on the left, then drag its knobs",
    };
    let hx = p.tabs[1].x + p.tabs[1].w + 28.0 * scale;
    if hx + text.measure(hint, size) < p.print.x - 20.0 * scale {
        text.label(
            hint,
            size,
            hx,
            p.tabs[0].y + (p.tabs[0].h - line) * 0.5,
            th.text_dim,
            p.print.x - hx,
            res,
        );
    }

    for h in &p.heads {
        if vis(h.pos[1]) {
            text.label(
                h.label,
                size,
                h.pos[0],
                h.pos[1],
                th.accent,
                400.0 * scale,
                res,
            );
        }
    }
    for c in &p.cells {
        if !vis(c.label_pos[1]) {
            continue;
        }
        text.label(
            c.name,
            size,
            c.label_pos[0],
            c.label_pos[1],
            th.text_dim,
            c.label_w,
            res,
        );
        text.label(
            &c.hex,
            size,
            c.hex_pos[0],
            c.hex_pos[1],
            if c.editing { th.accent } else { th.text },
            120.0 * scale,
            res,
        );
    }
    for (i, pos) in p.pick_labels.iter().enumerate() {
        if vis(pos[1]) {
            text.label(
                MATERIALS[i].0,
                size,
                pos[0],
                pos[1],
                th.text,
                300.0 * scale,
                res,
            );
        }
    }
    for row in &p.rows {
        if !vis(row.label_pos[1]) {
            continue;
        }
        text.label(
            row.label,
            size,
            row.label_pos[0],
            row.label_pos[1],
            th.text_dim,
            row.track.w,
            res,
        );
        let w = text.measure(&row.value, size);
        text.label(
            &row.value,
            size,
            row.track.x + row.track.w - w,
            row.label_pos[1],
            // A knob doing nothing reads dim; one that is on wears the
            // accent, so the panel says what has actually been changed.
            if row.t > 0.0 { th.accent } else { th.text_dim },
            row.track.w,
            res,
        );
    }
}
