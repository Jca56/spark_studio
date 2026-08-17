//! What the playground paints and writes: its furniture and its text.
//! Split from `mod` so the knob model and the panel layout stay the
//! readable part of the module.

use spark_render::Viewport;
use spark_ui::{UiRect, surfaces, theme};

use super::{MATERIALS, Panel, TEXT, nth};

/// The panel's furniture. Painted from plain theme colors on purpose — see
/// the module header: a surface tuned into oblivion must never take the
/// panel that would fix it down with it.
pub fn rects(panel: &Panel, scale: f32, pick: usize) -> Vec<UiRect> {
    let t = theme();
    let live = surfaces();
    let mut out = Vec::new();
    for (i, &chip) in panel.chips.iter().enumerate() {
        // Each chip wears its own material, so the picker previews it.
        let s = nth(&live, i);
        out.push(if i == pick {
            s.ringed(chip, scale, 3.0, t.accent)
        } else {
            s.rect(chip, scale)
        });
    }
    for row in &panel.rows {
        out.extend(spark_ui::Slider::rects(row.track, row.t));
    }
    for (v, tint) in [(panel.print, t.accent), (panel.reset, t.card_border)] {
        out.push(UiRect::region_rounded(v, t.card, 10.0 * scale).stroke(2.5 * scale, tint));
    }
    out
}

/// The panel's text: chip names, knob labels and their live values, and the
/// two button captions. Clipped to the panel it scrolls inside, the same way
/// every other list in the editor handles overflow.
pub fn labels(
    text: &mut spark_text::Text,
    mp: &Panel,
    area: Viewport,
    scale: f32,
    res: (u32, u32),
) {
    use spark_text::Text;
    let th = theme();
    let (title_col, header_col, accent) = (th.text, th.text_dim, th.accent);
    // The material playground, clipped to the left panel it scrolls in.
    let ms = TEXT * scale;
    let mline = Text::line_height(ms);
    let clip = (area.y, area.y + area.h);
    let vis = |y: f32| y >= clip.0 && y + mline <= clip.1;
    for (i, (name, _, _)) in MATERIALS.iter().enumerate() {
        let pos = mp.labels[i];
        if vis(pos[1]) {
            text.label(name, ms, pos[0], pos[1], title_col, area.w, res);
        }
    }
    for row in &mp.rows {
        if !vis(row.label_pos[1]) {
            continue;
        }
        text.label(
            row.label,
            ms,
            row.label_pos[0],
            row.label_pos[1],
            header_col,
            row.track.w,
            res,
        );
        let w = text.measure(&row.value, ms);
        text.label(
            &row.value,
            ms,
            row.track.x + row.track.w - w,
            row.label_pos[1],
            // A knob that is doing nothing reads dim; one that is on
            // wears the accent, so the panel says at a glance what has
            // actually been changed.
            if row.t > 0.0 { accent } else { header_col },
            row.track.w,
            res,
        );
    }
    for (v, label) in [(mp.print, "Print"), (mp.reset, "Reset")] {
        let y = v.y + (v.h - mline) * 0.5;
        if !vis(y) {
            continue;
        }
        let w = text.measure(label, ms);
        text.label(label, ms, v.x + (v.w - w) * 0.5, y, title_col, v.w, res);
    }
}
