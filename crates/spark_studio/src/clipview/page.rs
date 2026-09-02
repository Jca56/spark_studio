//! The clip view's layout — Ableton's clip envelopes, Alva's pick
//! (2026-08-31): the track sidebar lists the settings this clip keys —
//! and the ones picked from the inspector to key next — in the
//! inspector's order and wearing the inspector's words, dim until they
//! have keys; the chosen one's curve fills the whole axis
//! area; a **key strip** under the ruler carries every key time across
//! every track, for retiming a moment as one. Time is clip-local: zero
//! at the content's start, bars counted from there; what never plays
//! (past the loop, outside a trimmed span) is washed dark.
//!
//! Pure geometry — rects, words and hit tests from the panel, the clip
//! and the view state. Nothing here touches the document; the drags
//! live in `super`.

use spark_render::{Shape, Viewport};

use super::snap::{self, RULE_PX};
use super::words::{bounded, current_value, fmt_target, value_span};
use crate::anim::{Ease, Key, Target};
use crate::arrange::ROW_STEP;
use crate::doc::ObjClip;
use crate::fx::Stack;
use crate::timeline::{Panel, TimeView};

/// A row's box inside its `ROW_STEP` pitch, logical px — the
/// arrangement's own.
pub const ROW_H: f32 = 52.0;
/// The key strip's height under the ruler.
pub const STRIP_H: f32 = 36.0;
/// A key diamond's square on the graph, and on the strip.
pub const KEY: f32 = 24.0;
pub const STRIP_KEY: f32 = 20.0;
/// How near a press must land to a diamond to grab it.
pub const GRAB: f32 = 18.0;
/// How near the loop brace's end a press on the ruler grabs it.
pub const BRACE_GRAB: f32 = 14.0;
/// The keyed-glyph column at a row's left, and where its name starts.
pub const ROW_GLYPH: f32 = 16.0;
pub const ROW_TEXT_X: f32 = 40.0;
/// Air between the strip, the graph and the panel's floor.
const GRAPH_PAD: f32 = 16.0;
/// How often the curve is sampled across the axis.
const CURVE_STEP: f32 = 2.0;

/// What is picked in the view: one key on the shown curve, one moment
/// on the strip — every key at that time, across every track — or a
/// set of keys on the shown curve (Shift-clicks, a band, Ctrl+A).
#[derive(Clone, Debug, PartialEq)]
pub enum Sel {
    Key { target: Target, k: usize },
    Time(f32),
    Keys(Vec<(Target, usize)>),
}

impl Sel {
    /// Whether key `k` of `target` is among the picked.
    pub fn has(&self, target: Target, k: usize) -> bool {
        match self {
            Sel::Key { target: st, k: sk } => *st == target && *sk == k,
            Sel::Keys(set) => set.contains(&(target, k)),
            Sel::Time(_) => false,
        }
    }

    /// The picked keys as a set — a single key is a set of one, a
    /// moment is none (its keys live on every track).
    pub fn set(&self) -> Vec<(Target, usize)> {
        match self {
            Sel::Key { target, k } => vec![(*target, *k)],
            Sel::Keys(set) => set.clone(),
            Sel::Time(_) => Vec::new(),
        }
    }
}

/// What a press lands on.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Hit {
    /// The breadcrumb plate at the top of the sidebar.
    Back,
    /// A setting's row in the sidebar.
    Row(usize),
    /// A key diamond on the graph, by index into [`Page::keys`].
    Key(usize),
    /// A moment on the strip, by index into [`Page::strip_dots`].
    StripKey(usize),
    /// The strip's air.
    Strip,
    /// The graph's air.
    Graph,
    /// The loop brace's end on the ruler — drag it to set how much
    /// repeats.
    LoopEnd,
}

/// One keyable setting's row in the sidebar.
pub struct Row {
    pub target: Target,
    pub cell: Viewport,
    pub label: String,
    /// The curve's value where the playhead is (or at local zero) — or,
    /// with no curve yet, the object's value as it stands.
    pub value: String,
    /// Whether it has keys on this clip.
    pub keyed: bool,
    pub selected: bool,
}

/// One key on the shown curve, placed.
pub struct KeyDot {
    pub target: Target,
    pub k: usize,
    pub at: [f32; 2],
    pub t: f32,
    pub v: f32,
    pub linear: bool,
    pub selected: bool,
}

/// One moment on the strip.
pub struct StripDot {
    pub t: f32,
    pub x: f32,
    pub selected: bool,
}

/// What the page is built from.
pub struct Input<'a> {
    pub clip: &'a ObjClip,
    pub name: &'a str,
    pub color: [f32; 3],
    pub fx: &'a Stack,
    pub canvas: [f32; 2],
    /// The object's working copy: which settings it has, and their
    /// values as they stand.
    pub shape: &'a Shape,
    /// The settings to list, in order, with their words — the keyed
    /// ones and the armed ones (see [`keyable_targets`]).
    pub listed: &'a [(Target, String)],
    pub bpm: f32,
    pub target: Option<Target>,
    pub sel: Option<Sel>,
    /// How far the rows are scrolled up, physical px.
    pub scroll: f32,
    /// The playhead in clip-local time, while the song is inside the clip.
    pub playhead: Option<f32>,
    /// A value span held still through a drag, so the key being dragged
    /// doesn't rescale the graph under itself.
    pub frozen: Option<(f32, f32)>,
    /// The selection band being dragged across the graph, if one is.
    pub band: Option<Viewport>,
}

pub struct Page {
    pub scale: f32,
    pub bpm: f32,
    /// The breadcrumb plate, and the name on it.
    pub header: Viewport,
    pub title: String,
    /// Where rows draw: the sidebar under the header.
    pub rows_clip: Viewport,
    pub rows: Vec<Row>,
    /// The rows' total height, physical px — the scroll's limit.
    pub content_h: f32,
    pub strip: Viewport,
    pub strip_dots: Vec<StripDot>,
    pub graph: Viewport,
    /// The value range mapped onto the graph's height, bottom to top.
    pub span: (f32, f32),
    /// The value rules across the graph — a round step apart — and the
    /// step itself: where a dragged key's value snaps (see `snap`).
    pub rules: Vec<f32>,
    pub step: f32,
    /// Values a dragged key is drawn to from a little way off: the
    /// setting's floor and ceiling where it has them, and zero.
    pub magnets: Vec<f32>,
    /// The curve as segments, with whether each lies between the first
    /// and last key (outside, the curve only holds — and a setting with
    /// no keys yet is one flat hold at its value).
    pub curve: Vec<([f32; 2], [f32; 2], bool)>,
    pub keys: Vec<KeyDot>,
    pub target: Option<Target>,
    pub color: [f32; 3],
    pub is_light: bool,
    /// The ruler, and where the loop brace ends on it while looping.
    pub ruler: Viewport,
    pub loop_end_x: Option<f32>,
    /// The parts of the axis that never play: past the loop's end, or
    /// outside a non-looping clip's trimmed span.
    pub wash: Vec<Viewport>,
    /// The selection band, while one is being dragged.
    pub band: Option<Viewport>,
}

impl Page {
    pub fn build(panel: &Panel, view: &TimeView, scale: f32, inp: &Input) -> Self {
        let s = scale;
        let (ax, aw) = panel.axis;
        let cell_at = |y: f32| Viewport {
            x: panel.names_box.x + 6.0 * s,
            y: y + 2.0 * s,
            w: panel.names_box.w - 12.0 * s,
            h: ROW_H * s - 4.0 * s,
        };
        let header = cell_at(panel.lanes.y);
        let rows_clip = Viewport {
            x: panel.names_box.x,
            y: panel.lanes.y + ROW_STEP * s,
            w: panel.names_box.w,
            h: (panel.lanes.h - ROW_STEP * s).max(0.0),
        };
        let anim = &inp.clip.anim;
        let at_t = inp.playhead.unwrap_or(0.0);
        let current = |target: Target| current_value(inp.shape, inp.fx, target);
        let keyed_track = |target: Target| anim.track(target).filter(|tr| !tr.keys.is_empty());
        let rows: Vec<Row> = inp
            .listed
            .iter()
            .enumerate()
            .map(|(k, (target, label))| {
                let target = *target;
                let track = keyed_track(target);
                let value = match track {
                    Some(tr) => tr.sample(at_t),
                    None => current(target),
                };
                Row {
                    target,
                    cell: cell_at(rows_clip.y - inp.scroll + k as f32 * ROW_STEP * s),
                    label: label.clone(),
                    value: value
                        .map(|v| fmt_target(target, v, inp.fx, inp.canvas, inp.shape.is_light()))
                        .unwrap_or_default(),
                    keyed: track.is_some(),
                    selected: inp.target == Some(target),
                }
            })
            .collect();
        let content_h = rows.len() as f32 * ROW_STEP * s;
        let strip = Viewport {
            x: ax,
            y: panel.lanes.y,
            w: aw,
            h: STRIP_H * s,
        };
        let sel_time = match &inp.sel {
            Some(Sel::Time(t)) => Some(*t),
            _ => None,
        };
        let strip_dots: Vec<StripDot> = anim
            .key_times()
            .into_iter()
            .map(|(t, _)| StripDot {
                t,
                x: view.x_of(t, panel.axis),
                selected: sel_time.is_some_and(|st| (st - t).abs() < crate::anim::KEY_EPS),
            })
            .filter(|d| d.x >= ax - KEY * s && d.x <= ax + aw + KEY * s)
            .collect();
        let gy = strip.y + strip.h + GRAPH_PAD * s;
        let graph = Viewport {
            x: ax,
            y: gy,
            w: aw,
            h: (panel.axis_y.1 - GRAPH_PAD * s - gy).max(1.0),
        };
        // What never plays, washed dark from the strip to the floor.
        let clip = inp.clip;
        let band = |a: f32, b: f32| -> Option<Viewport> {
            let a = a.max(ax);
            let b = b.min(ax + aw);
            (b > a + 0.5).then_some(Viewport {
                x: a,
                y: strip.y,
                w: b - a,
                h: (panel.axis_y.1 - strip.y).max(1.0),
            })
        };
        let mut wash = Vec::new();
        if clip.loop_on {
            wash.extend(band(view.x_of(clip.loop_len, panel.axis), ax + aw));
        } else {
            wash.extend(band(ax, view.x_of(clip.offset, panel.axis)));
            wash.extend(band(view.x_of(clip.offset + clip.len, panel.axis), ax + aw));
        }
        let loop_end_x = clip
            .loop_on
            .then(|| view.x_of(clip.loop_len, panel.axis))
            .filter(|x| *x >= ax && *x <= ax + aw);
        let mut page = Self {
            scale,
            bpm: inp.bpm,
            header,
            title: inp.name.to_string(),
            rows_clip,
            rows,
            content_h,
            strip,
            strip_dots,
            graph,
            span: (0.0, 1.0),
            rules: Vec::new(),
            step: 0.0,
            magnets: Vec::new(),
            curve: Vec::new(),
            keys: Vec::new(),
            target: inp.target,
            color: inp.color,
            is_light: inp.shape.is_light(),
            ruler: panel.ruler,
            loop_end_x,
            wash,
            band: inp.band,
        };
        let Some(target) = inp.target else {
            return page;
        };
        // The chosen setting's curve — or, with no keys yet, one flat
        // hold at its value, where a double-click plants the first key.
        let flat;
        let track = match keyed_track(target) {
            Some(tr) => tr,
            None => {
                let Some(v) = current(target) else {
                    return page;
                };
                flat = crate::anim::Track {
                    target,
                    keys: vec![Key {
                        t: 0.0,
                        v,
                        ease: Ease::Smooth,
                    }],
                };
                &flat
            }
        };
        let has_keys = keyed_track(target).is_some();
        page.span = inp
            .frozen
            .unwrap_or_else(|| value_span(track.target, &track.keys, inp.fx, inp.canvas));
        let (top, bottom) = page.band();
        page.step = snap::value_step(target, page.span, bottom - top, RULE_PX * s);
        page.rules = snap::rules(page.span, page.step);
        let (lo, hi) = page.span;
        let mut magnets = match target {
            Target::Shape(p) if bounded(p) => {
                let (a, b) = crate::props::range(p, inp.canvas);
                vec![a, b]
            }
            Target::Shape(_) => Vec::new(),
            Target::Effect { id, param } => inp
                .fx
                .find(id)
                .and_then(|e| e.kind.params().get(param as usize))
                .map(|sp| vec![sp.min, sp.max])
                .unwrap_or_default(),
        };
        if lo <= 0.0 && hi >= 0.0 {
            magnets.push(0.0);
        }
        page.magnets = magnets;
        // The curve, sampled every couple of pixels across the axis.
        let step = CURVE_STEP * s;
        let (first, last) = match (track.keys.first(), track.keys.last()) {
            (Some(a), Some(b)) => (a.t, b.t),
            _ => (0.0, 0.0),
        };
        let cols = (aw / step).max(1.0) as usize;
        let mut prev: Option<[f32; 2]> = None;
        for col in 0..=cols {
            let x = ax + (col as f32 * step).min(aw);
            let t = view.t_at(x, panel.axis);
            let Some(v) = track.sample(t) else { break };
            let p = [x, page.y_of(v)];
            if let Some(a) = prev {
                let inside = has_keys && t >= first - 1e-4 && t <= last + 1e-4;
                page.curve.push((a, p, inside));
            }
            prev = Some(p);
        }
        if !has_keys {
            return page;
        }
        page.keys = track
            .keys
            .iter()
            .enumerate()
            .map(|(k, key)| KeyDot {
                target,
                k,
                at: [view.x_of(key.t, panel.axis), page.y_of(key.v)],
                t: key.t,
                v: key.v,
                linear: key.ease == Ease::Linear,
                selected: inp.sel.as_ref().is_some_and(|s| s.has(target, k)),
            })
            .filter(|d| d.at[0] >= ax - KEY * s && d.at[0] <= ax + aw + KEY * s)
            .collect();
        page
    }

    /// The graph's inner band: diamonds at the extremes stay whole.
    pub(super) fn band(&self) -> (f32, f32) {
        let pad = KEY * 0.5 * self.scale;
        (self.graph.y + pad, self.graph.y + self.graph.h - pad)
    }

    /// Where a value sits on the graph.
    pub fn y_of(&self, v: f32) -> f32 {
        let (lo, hi) = self.span;
        let (top, bottom) = self.band();
        let f = ((v - lo) / (hi - lo).max(1e-6)).clamp(-0.5, 1.5);
        bottom - f * (bottom - top)
    }

    /// How tall one unit of value is on the graph, px.
    pub fn px_per_unit(&self) -> f32 {
        let (lo, hi) = self.span;
        let (top, bottom) = self.band();
        (bottom - top) / (hi - lo).abs().max(1e-6)
    }

    /// The value at a height on the graph — the inverse of [`Page::y_of`].
    pub fn value_at(&self, y: f32) -> f32 {
        let (lo, hi) = self.span;
        let (top, bottom) = self.band();
        let f = (bottom - y) / (bottom - top).max(1.0);
        lo + f * (hi - lo)
    }

    pub fn max_scroll(&self) -> f32 {
        (self.content_h - self.rows_clip.h).max(0.0)
    }

    /// The shown curve's keys whose diamonds sit inside `r` — what a
    /// band picks.
    pub fn keys_in(&self, r: Viewport) -> Vec<(Target, usize)> {
        self.keys
            .iter()
            .filter(|d| r.contains(d.at[0], d.at[1]))
            .map(|d| (d.target, d.k))
            .collect()
    }

    /// What a press lands on: the loop brace's end on the ruler, the
    /// breadcrumb, a row, the nearest diamond on the graph or the strip,
    /// then the strip's or the graph's air.
    pub fn hit(&self, x: f32, y: f32) -> Option<Hit> {
        if self.ruler.contains(x, y) {
            return self
                .loop_end_x
                .filter(|lx| (lx - x).abs() <= BRACE_GRAB * self.scale)
                .map(|_| Hit::LoopEnd);
        }
        if self.header.contains(x, y) {
            return Some(Hit::Back);
        }
        if self.rows_clip.contains(x, y)
            && let Some(k) = self.rows.iter().position(|r| r.cell.contains(x, y))
        {
            return Some(Hit::Row(k));
        }
        let grab = GRAB * self.scale;
        let near = |px: f32, py: f32| ((px - x).powi(2) + (py - y).powi(2)).sqrt();
        if let Some((k, _)) = self
            .keys
            .iter()
            .enumerate()
            .map(|(k, d)| (k, near(d.at[0], d.at[1])))
            .filter(|(_, d)| *d <= grab)
            .min_by(|a, b| a.1.total_cmp(&b.1))
        {
            return Some(Hit::Key(k));
        }
        let sy = self.strip.y + self.strip.h * 0.5;
        if let Some((k, _)) = self
            .strip_dots
            .iter()
            .enumerate()
            .map(|(k, d)| (k, near(d.x, sy)))
            .filter(|(_, d)| *d <= grab)
            .min_by(|a, b| a.1.total_cmp(&b.1))
        {
            return Some(Hit::StripKey(k));
        }
        if self.strip.contains(x, y) {
            return Some(Hit::Strip);
        }
        if self.graph.contains(x, y) {
            return Some(Hit::Graph);
        }
        None
    }
}
