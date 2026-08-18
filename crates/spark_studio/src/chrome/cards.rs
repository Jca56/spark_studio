//! The right panel's text: folder headers and layer cards.
//!
//! Split out of the one `labels` pass that draws every word in the editor —
//! it had grown past the file budget, and this is the part of it with a
//! boundary of its own: everything here clips to the cards region under the
//! pinned color home, and nothing outside it does.

use spark_text::Text;

use super::{Scene, choice_labels, scrub_labels, toggle_labels};
use spark_ui::theme;

/// Every label on the folder headers and layer cards, clipped to the cards
/// region. `size` is the panel body size the caller already worked out.
pub(super) fn labels(text: &mut Text, scale: f32, size: f32, scene: &Scene, res: (u32, u32)) {
    let th = theme();
    let title_col = th.text;
    let header_col = th.text_dim;
    // Layer cards: name, scrub fields, and expanded detail — all
    // clipped to the cards region under the pinned color home.
    let clip = (scene.cards.y, scene.cards.y + scene.cards.h);
    let line = Text::line_height(size);
    let card_size = crate::layers::CARD_TEXT * scale;
    let card_line = Text::line_height(card_size);
    let vis = |y: f32, l: f32| y >= clip.0 && y + l <= clip.1;
    for f in scene.folders {
        if !vis(f.label_pos[1], line) || scene.renaming_folder == Some(f.id) {
            continue;
        }
        // Name plus a member count, so a collapsed folder still says how
        // much is inside it.
        let count = format!("{}", f.count);
        let cw = text.measure(&count, card_size);
        text.label(
            &f.label,
            size,
            f.label_pos[0],
            f.label_pos[1],
            if f.hidden {
                th.text_off
            } else if f.selected {
                title_col
            } else {
                theme().accent
            },
            (f.eye.x - cw - 16.0 * scale - f.label_pos[0]).max(40.0),
            res,
        );
        text.label(
            &count,
            card_size,
            f.eye.x - cw - 12.0 * scale,
            f.head.y + (f.head.h - card_line) * 0.5,
            header_col,
            cw + 2.0,
            res,
        );
        for sf in &f.scrubs {
            let y = sf.rect.y + (sf.rect.h - card_line) * 0.5;
            if !vis(y, card_line) {
                continue;
            }
            scrub_labels(
                text,
                sf,
                scene.editing == Some(crate::layers::EditField::Folder(f.id, sf.prop)),
                scene.edit_buf,
                (card_size, y, scale),
                (header_col, title_col),
                res,
            );
        }
        // The fade slider's name and readout, drawn like a card's.
        if vis(f.fade.label_pos[1], card_line) {
            text.label(
                f.fade.label,
                card_size,
                f.fade.label_pos[0],
                f.fade.label_pos[1],
                header_col,
                f.fade.track.w,
                res,
            );
            let w = text.measure(&f.fade.value, card_size);
            text.label(
                &f.fade.value,
                card_size,
                f.fade.value_right - w,
                f.fade.track.y + (f.fade.track.h - card_line) * 0.5,
                if f.fade.keyed {
                    theme().accent
                } else {
                    title_col
                },
                w + 2.0,
                res,
            );
        }
    }
    for lr in scene.layers {
        if vis(lr.label_pos[1], line) && scene.renaming != Some(lr.index) {
            text.label(
                &lr.label,
                size,
                lr.label_pos[0],
                lr.label_pos[1],
                if lr.hidden {
                    th.text_off
                } else if lr.selected {
                    title_col
                } else {
                    header_col
                },
                (lr.row.x + lr.row.w - lr.label_pos[0] - 50.0 * scale).max(40.0),
                res,
            );
        }
        for f in &lr.scrubs {
            let y = f.rect.y + (f.rect.h - card_line) * 0.5;
            if !vis(y, card_line) {
                continue;
            }
            scrub_labels(
                text,
                f,
                scene.editing == Some(crate::layers::EditField::Shape(lr.index, f.prop)),
                scene.edit_buf,
                (card_size, y, scale),
                (header_col, title_col),
                res,
            );
        }
        if let Some(d) = &lr.detail {
            // One label block per effect card.
            for row in &d.fx {
                if vis(row.label_pos[1], line) {
                    text.label(
                        row.label,
                        size,
                        row.label_pos[0],
                        row.label_pos[1],
                        if row.on { title_col } else { th.text_off },
                        (row.eye.x - row.label_pos[0]).max(20.0),
                        res,
                    );
                }
                // A single-parameter effect doesn't name its parameter:
                // "Glow / Radius" says the same thing twice, and the
                // card's own title is already the label. Effects with
                // several keep their names, or three sliders would be
                // indistinguishable.
                let named = row.params.len() > 1;
                for p in &row.params {
                    if !vis(p.label_pos[1], card_line) {
                        continue;
                    }
                    if named {
                        text.label(
                            p.label,
                            card_size,
                            p.label_pos[0],
                            p.label_pos[1],
                            header_col,
                            p.track.w,
                            res,
                        );
                    }
                    // Beside the track, centred on it — not stacked
                    // above, which spent a whole row on a number.
                    let w = text.measure(&p.value, card_size);
                    text.label(
                        &p.value,
                        card_size,
                        p.value_right - w,
                        p.track.y + (p.track.h - card_line) * 0.5,
                        if p.keyed { theme().accent } else { title_col },
                        w + 2.0,
                        res,
                    );
                }
            }
            for row in &d.sliders {
                if !vis(row.label_pos[1], card_line) {
                    continue;
                }
                text.label(
                    row.label,
                    card_size,
                    row.label_pos[0],
                    row.label_pos[1],
                    header_col,
                    row.track.w,
                    res,
                );
                let w = text.measure(&row.value, card_size);
                text.label(
                    &row.value,
                    card_size,
                    row.value_right - w,
                    row.track.y + (row.track.h - card_line) * 0.5,
                    if row.keyed { theme().accent } else { title_col },
                    w + 2.0,
                    res,
                );
            }
            if let Some(f) = &d.form {
                choice_labels(text, f, "Star", card_size, clip, res);
            }
            if let Some(st) = &d.style {
                toggle_labels(text, st, "Style", ["Fill", "Outline"], card_size, clip, res);
            }
            // The checkbox's label is the whole control's name — there is
            // no second state to name, which is the point of it.
            if let Some(b) = &d.blend
                && vis(b.check.label_pos[1], card_line)
            {
                text.label(
                    b.label,
                    card_size,
                    b.check.label_pos[0],
                    b.check.label_pos[1] + (b.check.square.h - card_line) * 0.5,
                    title_col,
                    b.check.row.w,
                    res,
                );
            }
        }
    }
}
