//! The panel body: a tool's draw-defaults page — a Fill|Outline or star-
//! form switch and a row or two of knobs — or Home: the verbs for what
//! was under the cursor, with their shortcuts down the right. The panel
//! is as tall as its page (never shorter than the rail beside it).
//!
//! Pure geometry: built from a snapshot of state, it hands back rects,
//! hit tests, and the words for the text pass, and never touches the
//! editor. The frame and the input path build the same `Page` from the
//! same inputs, so what lights is what clicks.

use spark_render::Viewport;
use spark_ui::{Dial, Knob, Segmented, UiRect, knob_rects, surfaces, theme};

use super::Drag;
use super::home::{Tone, Verb};
use crate::chrome::{MENU_TEXT, UI_TEXT};
use crate::defaults::{self, KnobSpec, Switch, ToolDefaults};
use crate::editor::Tool;

/// Inset from the panel's edges, logical px.
pub const PAD: f32 = 18.0;
/// The title row, including the air under it.
const TITLE_H: f32 = 40.0;
/// The segmented switch's height.
const SWITCH_H: f32 = 46.0;
/// A knob cell's side: three across the panel's inner width.
pub const CELL: f32 = (super::PANEL_W - 2.0 * PAD) / 3.0;
/// The knob label's font size, and the room a knob row leaves for it.
const KNOB_LABEL: f32 = 21.0;
const KNOB_LABEL_ROOM: f32 = 34.0;
/// The readout's font size, inside the cap.
const READOUT: f32 = 22.0;
/// A verb row's height and its shortcut's font size.
const ROW_H: f32 = 52.0;
const KEY_TEXT: f32 = 19.0;
/// Air between rows of things.
const GAP: f32 = 12.0;
/// Knobs per row.
const COLS: usize = 3;

/// A widget on the page, by position where there are several.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hit {
    Knob(usize),
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

/// One knob, laid out.
#[derive(Clone, Debug, PartialEq)]
pub struct KnobSlot {
    pub spec: KnobSpec,
    pub center: [f32; 2],
    /// The track's centreline radius, physical px.
    pub radius: f32,
    /// Where a press grabs it: the whole cell.
    pub hit: Viewport,
    /// Whether it turns anything right now (a fill's thickness doesn't).
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
    pub knobs: Vec<KnobSlot>,
    pub switch: Option<(Switch, Segmented, usize)>,
    pub verbs: Vec<VerbRow>,
}

/// How many knob rows a tool's page has.
fn knob_rows(tool: Tool) -> usize {
    defaults::knobs(tool).len().div_ceil(COLS)
}

/// How tall a tool's page is, physical px: title, its switch if it has
/// one, its knob rows.
pub fn tool_height(tool: Tool, scale: f32) -> f32 {
    let switch = if Switch::for_tool(tool).is_some() {
        SWITCH_H + GAP
    } else {
        0.0
    };
    (PAD + TITLE_H + switch + knob_rows(tool) as f32 * (CELL + KNOB_LABEL_ROOM) + PAD) * scale
}

/// How tall Home is with `n` rows, physical px.
pub fn rows_height(n: usize, scale: f32) -> f32 {
    (PAD + TITLE_H + n as f32 * ROW_H + PAD) * scale
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

        let cell = CELL * s;
        let row_h = cell + KNOB_LABEL_ROOM * s;
        let dial = Dial::fit(cell, s);
        let knobs = defaults::knobs(tool)
            .iter()
            .enumerate()
            .map(|(k, spec)| {
                let (col, row) = (k % COLS, k / COLS);
                let center = [
                    x0 + cell * (col as f32 + 0.5),
                    y + row_h * row as f32 + cell * 0.5,
                ];
                // The whole cell is the grab target — a knob is a thing
                // you reach for, not a thing you aim at — and cells can't
                // overlap, so neither can grabs.
                let grab = cell * 0.5;
                let value = d.get(spec.prop);
                let (lo, hi) = crate::props::range(spec.prop, canvas);
                KnobSlot {
                    spec: *spec,
                    center,
                    radius: dial.radius,
                    hit: Viewport {
                        x: center[0] - grab,
                        y: center[1] - grab,
                        w: grab * 2.0,
                        h: grab * 2.0,
                    },
                    live: d.knob_live(tool, spec.prop),
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
            knobs,
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
            knobs: Vec::new(),
            switch: None,
            verbs,
        }
    }

    /// The widget under a point, if it can be clicked: a dimmed knob and
    /// a disabled verb are not.
    pub fn hit(&self, x: f32, y: f32) -> Option<Hit> {
        if let Some(k) = self.knobs.iter().position(|k| k.hit.contains(x, y)) {
            return self.knobs[k].live.then_some(Hit::Knob(k));
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

    /// The page's chrome, drawn after the panel it sits on. `fade` is the
    /// knobs' hover crossfade, one per slot.
    pub fn rects(&self, over: Option<Hit>, drag: Option<Drag>, fade: &[f32]) -> Vec<UiRect> {
        let t = theme();
        let s = self.scale;
        let mut out = Vec::new();
        if let Some((_, seg, active)) = &self.switch {
            out.extend(seg.rects(*active));
        }
        // Purple cool end heating to gold at the pointer — the slider's
        // ramp, on a dial.
        let look = Knob {
            color: t.accent_alt,
            hot: t.accent,
            bipolar: false,
        };
        for (k, slot) in self.knobs.iter().enumerate() {
            let held = matches!(drag, Some(Drag::Knob { slot: d, .. }) if d == k);
            let hover = fade.get(k).copied().unwrap_or(0.0);
            out.extend(knob_rects(
                slot.center,
                slot.radius,
                s,
                slot.v,
                hover,
                held,
                &look,
            ));
            if !slot.live {
                // A knob that turns nothing right now sinks under a wash
                // of the panel — still there, plainly not for turning.
                let r = slot.hit.w * 0.5;
                let mut wash = surfaces().float.fill;
                wash[3] = 0.72;
                out.push(UiRect::region_rounded(slot.hit, wash, r));
            }
        }
        for (i, v) in self.verbs.iter().enumerate() {
            if v.row.enabled && over == Some(Hit::Verb(i)) {
                out.push(surfaces().hover.rect(v.rect, s));
            }
        }
        out
    }

    /// The page's words. Knob readouts ride their fade: a resting knob
    /// shows its pointer, an engaged one its number. A danger row reads
    /// red while it is lit.
    pub fn labels(&self, fade: &[f32]) -> Vec<Label> {
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
        if let Some((sw, seg, active)) = &self.switch {
            let size = UI_TEXT * s;
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
        let dial_size = KNOB_LABEL * s;
        let readout_size = READOUT * s;
        for (k, slot) in self.knobs.iter().enumerate() {
            let d = Dial::new(slot.radius, s);
            out.push(Label {
                text: slot.spec.label.to_string(),
                size: dial_size,
                pos: [slot.center[0], d.label_top(slot.center, 6.0 * s)],
                color: if slot.live { t.text_dim } else { t.text_off },
                max_w: slot.hit.w + 20.0 * s,
                align: Align::Center,
            });
            let f = fade.get(k).copied().unwrap_or(0.0);
            if f > 0.01 {
                let mut col = t.text;
                col[3] = f;
                out.push(Label {
                    text: slot.readout.clone(),
                    size: readout_size,
                    pos: [slot.center[0], slot.center[1] - line_h(readout_size) * 0.5],
                    color: col,
                    max_w: d.cap_r * 2.0,
                    align: Align::Center,
                });
            }
        }
        let row_size = UI_TEXT * s;
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
                size: row_size,
                pos: [r.x + 16.0 * s, r.y + (r.h - line_h(row_size)) * 0.5],
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

    /// The property a knob slot turns, for callers that only have the slot.
    #[cfg(test)]
    pub fn knob_prop(&self, k: usize) -> Option<crate::props::Prop> {
        self.knobs.get(k).map(|s| s.spec.prop)
    }
}

/// One line box at `size` — lntrn-text lays out at 1.2em, and the label
/// pass needs the same number to centre a word vertically.
fn line_h(size: f32) -> f32 {
    spark_text::Text::line_height(size)
}
