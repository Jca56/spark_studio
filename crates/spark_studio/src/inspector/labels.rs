//! The inspector's words, placed by the page for the text pass. Body
//! words scrolled out of the window are not emitted — the text pass has
//! no scissor of its own, so the page withholds them.

use spark_ui::theme;

use super::field;
use super::page::{CAPTION_H, CAPTION_TEXT, Hit, PAD, Page, SLIDER_LABEL_H, TITLE_H};
use crate::chrome::{Align, Label, MENU_TEXT, UI_TEXT};

impl Page {
    /// Every word on the panel. `over` lights the field or slider under
    /// the cursor; `dragging` is the slider being dragged, whose readout
    /// stays gold while the cursor wanders off its band.
    pub fn labels(&self, over: Option<Hit>, dragging: Option<usize>) -> Vec<Label> {
        let t = theme();
        let s = self.scale;
        let mut out = Vec::new();
        let size = UI_TEXT * s;
        let cap = CAPTION_TEXT * s;
        let line = |sz: f32| spark_text::Text::line_height(sz);
        let in_body = |y: f32, h: f32| y + h > self.body.y && y < self.body.y + self.body.h;
        if let Some((title, _, _)) = &self.title {
            let th = TITLE_H * s;
            let y = self.title_y + (th - line(MENU_TEXT * s)) * 0.5;
            if in_body(y, line(MENU_TEXT * s)) {
                out.push(Label {
                    text: title.clone(),
                    size: MENU_TEXT * s,
                    pos: [self.panel.x + PAD * s + th * 0.7 + 10.0 * s, y],
                    color: t.text,
                    max_w: self.panel.w - PAD * 2.0 * s - th,
                    align: Align::Left,
                });
            }
        }
        for (k, f) in self.fields.iter().enumerate() {
            let cy = f.rect.y - CAPTION_H * s;
            if in_body(cy, CAPTION_H * s) {
                // Red, green, blue across the row — the gizmo's own.
                out.push(Label {
                    text: f.caption.to_string(),
                    size: cap,
                    pos: [f.rect.x + 4.0 * s, cy + (CAPTION_H * s - line(cap)) * 0.5],
                    color: field::column_colour(f.col),
                    max_w: f.rect.w,
                    align: Align::Left,
                });
            }
            if !in_body(f.rect.y, f.rect.h) {
                continue;
            }
            let y = f.rect.y + (f.rect.h - line(size)) * 0.5;
            match &self.edit {
                Some((slot, tb)) if *slot == k => out.push(Label {
                    text: tb.text().to_string(),
                    size,
                    pos: [f.rect.x + 14.0 * s, y],
                    color: t.text,
                    max_w: f.rect.w - 20.0 * s,
                    align: Align::Left,
                }),
                _ => out.push(Label {
                    text: f.text.clone(),
                    size,
                    pos: [f.rect.x + f.rect.w * 0.5, y],
                    color: if over == Some(Hit::Field(k)) {
                        t.accent
                    } else {
                        t.text
                    },
                    max_w: f.rect.w - 8.0 * s,
                    align: Align::Center,
                }),
            }
        }
        for (k, sl) in self.sliders.iter().enumerate() {
            if !in_body(sl.label_y, SLIDER_LABEL_H * s) {
                continue;
            }
            let engaged = over == Some(Hit::Slider(k)) || dragging == Some(k);
            let y = sl.label_y + (SLIDER_LABEL_H * s - line(size)) * 0.5;
            out.push(Label {
                text: sl.label.to_string(),
                size,
                pos: [sl.hit.x, y],
                color: if engaged { t.text } else { t.text_dim },
                max_w: sl.hit.w * 0.6,
                align: Align::Left,
            });
            out.push(Label {
                text: sl.readout.clone(),
                size,
                pos: [sl.hit.x + sl.hit.w, y],
                color: if engaged { t.accent } else { t.text },
                max_w: sl.hit.w * 0.4,
                align: Align::Right,
            });
        }
        for sw in &self.switches {
            for (i, (name, r)) in sw.labels.iter().zip(&sw.seg.segments).enumerate() {
                if !in_body(r.y, r.h) {
                    continue;
                }
                out.push(Label {
                    text: name.to_string(),
                    size,
                    pos: [r.x + r.w * 0.5, r.y + (r.h - line(size)) * 0.5],
                    color: if i == sw.active { t.accent } else { t.text_dim },
                    max_w: r.w,
                    align: Align::Center,
                });
            }
        }
        for c in &self.checks {
            let r = c.check.row;
            if !in_body(r.y, r.h) {
                continue;
            }
            out.push(Label {
                text: c.label.to_string(),
                size,
                pos: [c.check.label_pos[0], r.y + (r.h - line(size)) * 0.5],
                color: t.text,
                max_w: r.w,
                align: Align::Left,
            });
        }
        out
    }
}
