//! The clip view's words: the breadcrumb's name, each row's setting
//! and value, the value axis's ends, and the readout beside the picked
//! or hovered key. Split from `page` for the file budget; the layout
//! they ride is the page's.

use super::page::{Hit, KEY, Page, ROW_TEXT_X};
use super::words::{beat_label, fmt_target};
use crate::arrange::TRACK_TEXT;
use crate::chrome::{Align, Label, UI_TEXT};
use crate::fx::Stack;
use spark_render::Viewport;

/// The value-axis captions' size.
const AXIS_TEXT: f32 = 17.0;

impl Page {
    /// The words: the breadcrumb's name, each row's setting and value
    /// (withheld outside the rows' window — the text pass has no scissor
    /// of its own), the value axis's ends, and a readout beside the
    /// selected or hovered key.
    pub fn labels(&self, over: Option<Hit>, fx: &Stack, canvas: [f32; 2]) -> Vec<Label> {
        let t = spark_ui::theme();
        let s = self.scale;
        let line = spark_text::Text::line_height;
        let mut out = Vec::new();
        let hsize = UI_TEXT * s;
        // After the chevron.
        let hx = self.header.x + 44.0 * s;
        out.push(Label {
            text: self.title.clone(),
            size: hsize,
            pos: [hx, self.header.y + (self.header.h - line(hsize)) * 0.5],
            color: t.text,
            max_w: (self.header.x + self.header.w - hx - 8.0 * s).max(1.0),
            align: Align::Left,
        });
        let rsize = TRACK_TEXT * s;
        let fits = |c: Viewport| {
            c.y >= self.rows_clip.y - 1.0 && c.y + c.h <= self.rows_clip.y + self.rows_clip.h + 1.0
        };
        for r in &self.rows {
            if !fits(r.cell) {
                continue;
            }
            let y = r.cell.y + (r.cell.h - line(rsize)) * 0.5;
            let value_w = 96.0 * s;
            let x = r.cell.x + ROW_TEXT_X * s;
            out.push(Label {
                text: r.label.clone(),
                size: rsize,
                pos: [x, y],
                color: if r.selected {
                    t.text
                } else if r.keyed {
                    t.text_dim
                } else {
                    t.text_off
                },
                max_w: (r.cell.x + r.cell.w - x - value_w - 16.0 * s).max(1.0),
                align: Align::Left,
            });
            out.push(Label {
                text: r.value.clone(),
                size: rsize,
                pos: [r.cell.x + r.cell.w - 12.0 * s, y],
                color: if r.selected { t.accent } else { t.text_off },
                max_w: value_w,
                align: Align::Right,
            });
        }
        let Some(target) = self.target else {
            return out;
        };
        if self.curve.is_empty() {
            return out;
        }
        // The value axis: its top and bottom, in the setting's own units.
        let asize = AXIS_TEXT * s;
        let (lo, hi) = self.span;
        let (top, bottom) = self.band();
        for (v, y) in [(hi, top), (lo, bottom - line(asize))] {
            out.push(Label {
                text: fmt_target(target, v, fx, canvas, self.is_light),
                size: asize,
                pos: [self.graph.x + 8.0 * s, y],
                color: t.text_off,
                max_w: 160.0 * s,
                align: Align::Left,
            });
        }
        // The readout rides the selected key, or the one under the cursor.
        let shown = self
            .keys
            .iter()
            .find(|d| d.selected)
            .or_else(|| match over {
                Some(Hit::Key(k)) => self.keys.get(k),
                _ => None,
            });
        if let Some(d) = shown {
            let text = format!(
                "{} · {}",
                beat_label(d.t, self.bpm),
                fmt_target(d.target, d.v, fx, canvas, self.is_light)
            );
            let above = d.at[1] - KEY * s - line(hsize) * 0.4;
            let y = if above < self.graph.y {
                d.at[1] + KEY * s * 0.8
            } else {
                above
            };
            out.push(Label {
                text,
                size: hsize,
                pos: [d.at[0], y],
                color: t.accent,
                max_w: 300.0 * s,
                align: Align::Center,
            });
        }
        out
    }
}
