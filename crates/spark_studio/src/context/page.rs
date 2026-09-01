//! The panel body: a tool's draw-defaults page — a Fill|Outline or star-
//! form switch and a stack of sliders, one per number — or Home: the
//! verbs for what was under the cursor, with their shortcuts down the
//! right. The panel is one fixed rectangle whatever the page (Alva:
//! "the menu keeps changing sizes!"); a short page leaves air below.
//!
//! Sliders, not knobs (Alva's call, 2026-08-31 — the dial is kept for
//! elsewhere): a label and a live readout on one line, the track under
//! them, the whole band the grab.
//!
//! Pure geometry: built from a snapshot of state, it hands back rects,
//! hit tests, and the words for the text pass, and never touches the
//! editor. The frame and the input path build the same `Page` from the
//! same inputs, so what lights is what clicks.

use spark_render::Viewport;
use spark_ui::{Segmented, Slider, UiRect, surfaces, theme};

use super::Drag;
use super::home::{Tone, Verb};
use crate::chrome::{MENU_TEXT, UI_TEXT};
use crate::defaults::{self, SliderSpec, Switch, ToolDefaults};
use crate::editor::Tool;

/// Inset from the panel's edges, logical px.
pub const PAD: f32 = 18.0;
/// The title row, including the air under it.
const TITLE_H: f32 = 40.0;
/// The segmented switch's height.
const SWITCH_H: f32 = 46.0;
/// A slider row: its label line, the air under it, the thumb's band,
/// and the air before the next row.
const SLIDER_LABEL_H: f32 = 28.0;
const SLIDER_TRACK_H: f32 = 20.0;
const SLIDER_ROW_H: f32 = 88.0;
/// A verb row's height and its shortcut's font size.
const ROW_H: f32 = 52.0;
const KEY_TEXT: f32 = 19.0;
/// Air between rows of things.
const GAP: f32 = 12.0;

/// A widget on the page, by position where there are several.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hit {
    Slider(usize),
    Segment(usize),
    Verb(usize),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Align {
    Left,
    Center,
    /// `pos.x` is the right edge.
    Right,
}

/// A word for the text pass: physical px throughout.
#[derive(Clone, Debug, PartialEq)]
pub struct Label {
    pub text: String,
    pub size: f32,
    pub pos: [f32; 2],
    pub color: [f32; 4],
    pub max_w: f32,
    pub align: Align,
}

/// One slider, laid out.
#[derive(Clone, Debug, PartialEq)]
pub struct SliderSlot {
    pub spec: SliderSpec,
    /// The track the thumb rides.
    pub track: Viewport,
    /// Where a press grabs it: the full-width band the thumb spans.
    pub hit: Viewport,
    /// Where its label line sits.
    pub label_y: f32,
    /// Whether it moves anything right now (a fill's thickness doesn't).
    pub live: bool,
    /// Normalized value, 0..1.
    pub v: f32,
    pub readout: String,
}

/// One Home row, as the table and the editor's state say it is now.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Row {
    pub verb: Verb,
    pub label: &'static str,
    pub key: &'static str,
    pub tone: Tone,
    pub enabled: bool,
}

/// A Home row, laid out.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VerbRow {
    pub rect: Viewport,
    pub row: Row,
}

pub struct Page {
    pub panel: Viewport,
    pub scale: f32,
    pub title: String,
    /// A tool page's title wears the accent; Home's is plain.
    pub title_accent: bool,
    pub sliders: Vec<SliderSlot>,
    pub switch: Option<(Switch, Segmented, usize)>,
    pub verbs: Vec<VerbRow>,
}

impl Page {
    /// A tool's draw-defaults page.
    pub fn tool(
        panel: Viewport,
        scale: f32,
        tool: Tool,
        title: &str,
        d: &ToolDefaults,
        canvas: [f32; 2],
    ) -> Self {
        let s = scale;
        let pad = PAD * s;
        let gap = GAP * s;
        let x0 = panel.x + pad;
        let w = panel.w - pad * 2.0;
        let mut y = panel.y + pad + TITLE_H * s;

        let switch = Switch::for_tool(tool).map(|sw| {
            let track = Viewport {
                x: x0,
                y,
                w,
                h: SWITCH_H * s,
            };
            y += SWITCH_H * s + gap;
            let n = sw.labels().len();
            (sw, Segmented::new(track, n, s), sw.active(d))
        });

        let track_h = SLIDER_TRACK_H * s;
        let sliders = defaults::sliders(tool)
            .iter()
            .enumerate()
            .map(|(k, spec)| {
                let top = y + SLIDER_ROW_H * s * k as f32;
                // The thumb is taller than the track; the band it spans
                // is the grab, and the track is centred in it.
                let thumb = Slider::thumb_side(Viewport {
                    x: 0.0,
                    y: 0.0,
                    w: 1.0,
                    h: track_h,
                });
                let band_y = top + SLIDER_LABEL_H * s;
                let track = Viewport {
                    x: x0,
                    y: band_y + (thumb - track_h) * 0.5,
                    w,
                    h: track_h,
                };
                let value = d.get(spec.prop);
                let (lo, hi) = crate::props::range(spec.prop, canvas);
                SliderSlot {
                    spec: *spec,
                    track,
                    hit: Viewport {
                        x: x0,
                        y: band_y,
                        w,
                        h: thumb,
                    },
                    label_y: top,
                    live: d.slider_live(tool, spec.prop),
                    v: ((value - lo) / (hi - lo).max(1e-6)).clamp(0.0, 1.0),
                    readout: defaults::readout(spec.prop, value),
                }
            })
            .collect();

        Self {
            panel,
            scale,
            title: title.to_string(),
            title_accent: true,
            sliders,
            switch,
            verbs: Vec::new(),
        }
    }

    /// Home: the target's rows.
    pub fn home(panel: Viewport, scale: f32, title: &str, rows: &[Row]) -> Self {
        let s = scale;
        let pad = PAD * s;
        let x0 = panel.x + pad;
        let w = panel.w - pad * 2.0;
        let y = panel.y + pad + TITLE_H * s;
        let verbs = rows
            .iter()
            .enumerate()
            .map(|(i, row)| VerbRow {
                rect: Viewport {
                    x: x0,
                    y: y + ROW_H * s * i as f32,
                    w,
                    h: ROW_H * s,
                },
                row: *row,
            })
            .collect();
        Self {
            panel,
            scale,
            title: title.to_string(),
            title_accent: false,
            sliders: Vec::new(),
            switch: None,
            verbs,
        }
    }

    /// The widget under a point, if it can be clicked: a dimmed slider
    /// and a disabled verb are not.
    pub fn hit(&self, x: f32, y: f32) -> Option<Hit> {
        if let Some(k) = self.sliders.iter().position(|k| k.hit.contains(x, y)) {
            return self.sliders[k].live.then_some(Hit::Slider(k));
        }
        if let Some((_, seg, _)) = &self.switch
            && let Some(i) = seg.hit(x, y)
        {
            return Some(Hit::Segment(i));
        }
        if let Some(i) = self.verbs.iter().position(|r| r.rect.contains(x, y)) {
            return self.verbs[i].row.enabled.then_some(Hit::Verb(i));
        }
        None
    }

    /// The page's chrome, drawn after the panel it sits on.
    pub fn rects(&self, over: Option<Hit>, _drag: Option<Drag>) -> Vec<UiRect> {
        let s = self.scale;
        let mut out = Vec::new();
        if let Some((_, seg, active)) = &self.switch {
            out.extend(seg.rects(*active));
        }
        for slot in &self.sliders {
            out.extend(Slider::rects(slot.track, slot.v));
            if !slot.live {
                // A slider that moves nothing right now sinks under a
                // wash of the panel — still there, plainly not for moving.
                let mut wash = surfaces().float.fill;
                wash[3] = 0.72;
                out.push(UiRect::region_rounded(slot.hit, wash, slot.hit.h * 0.5));
            }
        }
        for (i, v) in self.verbs.iter().enumerate() {
            if v.row.enabled && over == Some(Hit::Verb(i)) {
                out.push(surfaces().hover.rect(v.rect, s));
            }
        }
        out
    }

    /// The page's words. A slider's readout goes gold while it is under
    /// the cursor or being dragged; a danger row reads red while lit.
    pub fn labels(&self, over: Option<Hit>, drag: Option<Drag>) -> Vec<Label> {
        let t = theme();
        let s = self.scale;
        let pad = PAD * s;
        let x0 = self.panel.x + pad;
        let w = self.panel.w - pad * 2.0;
        let mut out = Vec::new();
        if !self.title.is_empty() {
            out.push(Label {
                text: self.title.clone(),
                size: MENU_TEXT * s,
                pos: [x0, self.panel.y + 14.0 * s],
                color: if self.title_accent { t.accent } else { t.text },
                max_w: w,
                align: Align::Left,
            });
        }
        let size = UI_TEXT * s;
        if let Some((sw, seg, active)) = &self.switch {
            for (i, (name, r)) in sw.labels().iter().zip(&seg.segments).enumerate() {
                out.push(Label {
                    text: name.to_string(),
                    size,
                    pos: [r.x + r.w * 0.5, r.y + (r.h - line_h(size)) * 0.5],
                    color: if i == *active { t.accent } else { t.text_dim },
                    max_w: r.w,
                    align: Align::Center,
                });
            }
        }
        for (k, slot) in self.sliders.iter().enumerate() {
            let engaged = over == Some(Hit::Slider(k)) || drag == Some(Drag::Slider(k));
            let (label_col, value_col) = if !slot.live {
                (t.text_off, t.text_off)
            } else if engaged {
                (t.text, t.accent)
            } else {
                (t.text_dim, t.text)
            };
            let y = slot.label_y + (SLIDER_LABEL_H * s - line_h(size)) * 0.5;
            out.push(Label {
                text: slot.spec.label.to_string(),
                size,
                pos: [x0, y],
                color: label_col,
                max_w: w * 0.6,
                align: Align::Left,
            });
            out.push(Label {
                text: slot.readout.clone(),
                size,
                pos: [x0 + w, y],
                color: value_col,
                max_w: w * 0.4,
                align: Align::Right,
            });
        }
        let key_size = KEY_TEXT * s;
        for v in &self.verbs {
            let r = v.rect;
            let col = match (v.row.enabled, v.row.tone) {
                (false, _) => t.text_off,
                (true, Tone::Danger) => t.red,
                (true, Tone::Normal) => t.text,
            };
            out.push(Label {
                text: v.row.label.to_string(),
                size,
                pos: [r.x + 16.0 * s, r.y + (r.h - line_h(size)) * 0.5],
                color: col,
                max_w: r.w * 0.6,
                align: Align::Left,
            });
            if !v.row.key.is_empty() {
                out.push(Label {
                    text: v.row.key.to_string(),
                    size: key_size,
                    pos: [r.x + r.w - 16.0 * s, r.y + (r.h - line_h(key_size)) * 0.5],
                    color: if v.row.enabled { t.text_dim } else { t.text_off },
                    max_w: r.w * 0.5,
                    align: Align::Right,
                });
            }
        }
        out
    }

    /// The property a slider slot moves, for callers that only have the
    /// slot.
    #[cfg(test)]
    pub fn slider_prop(&self, k: usize) -> Option<crate::props::Prop> {
        self.sliders.get(k).map(|s| s.spec.prop)
    }
}

/// One line box at `size` — lntrn-text lays out at 1.2em, and the label
/// pass needs the same number to centre a word vertically.
fn line_h(size: f32) -> f32 {
    spark_text::Text::line_height(size)
}
