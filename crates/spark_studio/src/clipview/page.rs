//! The clip view's layout — Ableton's clip envelopes, Alva's pick
//! (2026-08-31): the track sidebar lists the clip's keyed targets, one
//! row each; the chosen target's curve fills the whole axis area; a
//! **key strip** under the ruler carries every key time across every
//! track, for retiming a moment as one. Time is clip-local: zero at the
//! content's start, bars counted from there.
//!
//! Pure geometry — rects, words and hit tests from the panel, the clip
//! and the view state. Nothing here touches the document; the drags
//! live in `super`.

use spark_render::Viewport;

use crate::anim::{Ease, Key, Target};
use crate::arrange::{ROW_STEP, TRACK_TEXT};
use crate::chrome::{Align, Label, UI_TEXT};
use crate::doc::ObjClip;
use crate::fx::Stack;
use crate::inspector::{fmt_number, fmt_param, is_angle};
use crate::props::Prop;
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
/// Air between the strip, the graph and the panel's floor.
const GRAPH_PAD: f32 = 16.0;
/// How often the curve is sampled across the axis.
const CURVE_STEP: f32 = 2.0;
/// The value-axis captions' size.
const AXIS_TEXT: f32 = 17.0;

/// What is picked in the view: one key on the shown curve, or one
/// moment on the strip — every key at that time, across every track.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Sel {
    Key { target: Target, k: usize },
    Time(f32),
}

/// What a press lands on.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Hit {
    /// The breadcrumb plate at the top of the sidebar.
    Back,
    /// A target's row in the sidebar.
    Row(usize),
    /// A key diamond on the graph, by index into [`Page::keys`].
    Key(usize),
    /// A moment on the strip, by index into [`Page::strip_dots`].
    StripKey(usize),
    /// The strip's air.
    Strip,
    /// The graph's air.
    Graph,
}

/// One keyed target's row in the sidebar.
pub struct Row {
    pub target: Target,
    pub cell: Viewport,
    pub label: String,
    /// The curve's value where the playhead is (or at local zero).
    pub value: String,
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
    pub is_light: bool,
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
    /// The curve as segments, with whether each lies between the first
    /// and last key (outside, the curve only holds).
    pub curve: Vec<([f32; 2], [f32; 2], bool)>,
    pub keys: Vec<KeyDot>,
    pub target: Option<Target>,
    pub color: [f32; 3],
    /// The value shown beside a key: the selected one, or the hovered.
    pub is_light: bool,
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
            h: (panel.lanes.y + panel.lanes.h - panel.lanes.y - ROW_STEP * s).max(0.0),
        };
        let anim = &inp.clip.anim;
        let at_t = inp.playhead.unwrap_or(0.0);
        let rows: Vec<Row> = anim
            .tracks
            .iter()
            .enumerate()
            .map(|(k, tr)| Row {
                target: tr.target,
                cell: cell_at(rows_clip.y - inp.scroll + k as f32 * ROW_STEP * s),
                label: target_label(tr.target, inp.fx),
                value: tr
                    .sample(at_t)
                    .map(|v| fmt_target(tr.target, v, inp.fx, inp.canvas, inp.is_light))
                    .unwrap_or_default(),
                selected: inp.target == Some(tr.target),
            })
            .collect();
        let content_h = anim.tracks.len() as f32 * ROW_STEP * s;
        let strip = Viewport {
            x: ax,
            y: panel.lanes.y,
            w: aw,
            h: STRIP_H * s,
        };
        let sel_time = match inp.sel {
            Some(Sel::Time(t)) => Some(t),
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
            curve: Vec::new(),
            keys: Vec::new(),
            target: inp.target,
            color: inp.color,
            is_light: inp.is_light,
        };
        let Some(track) = inp.target.and_then(|tg| anim.track(tg)) else {
            return page;
        };
        page.span = inp
            .frozen
            .unwrap_or_else(|| value_span(track.target, &track.keys, inp.fx, inp.canvas));
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
                let inside = t >= first - 1e-4 && t <= last + 1e-4;
                page.curve.push((a, p, inside));
            }
            prev = Some(p);
        }
        let sel_key = match inp.sel {
            Some(Sel::Key { target, k }) if target == track.target => Some(k),
            _ => None,
        };
        page.keys = track
            .keys
            .iter()
            .enumerate()
            .map(|(k, key)| KeyDot {
                target: track.target,
                k,
                at: [view.x_of(key.t, panel.axis), page.y_of(key.v)],
                t: key.t,
                v: key.v,
                linear: key.ease == Ease::Linear,
                selected: sel_key == Some(k),
            })
            .filter(|d| d.at[0] >= ax - KEY * s && d.at[0] <= ax + aw + KEY * s)
            .collect();
        page
    }

    /// The graph's inner band: diamonds at the extremes stay whole.
    fn band(&self) -> (f32, f32) {
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

    /// What a press lands on: the breadcrumb, a row, the nearest diamond
    /// on the graph or the strip, then the strip's or the graph's air.
    pub fn hit(&self, x: f32, y: f32) -> Option<Hit> {
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

    /// The words: the breadcrumb's name, each row's target and value
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
            out.push(Label {
                text: r.label.clone(),
                size: rsize,
                pos: [r.cell.x + 14.0 * s, y],
                color: if r.selected { t.text } else { t.text_dim },
                max_w: (r.cell.w - value_w - 28.0 * s).max(1.0),
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
        if self.target.is_none() {
            return out;
        }
        // The value axis: its top and bottom, in the target's own units.
        let target = self.target.unwrap_or(Target::Shape(Prop::X));
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

/// A local time as a musician reads it: `Bar 2.3`, one-based.
pub fn beat_label(t: f32, bpm: f32) -> String {
    let beat_s = 60.0 / bpm.max(1.0);
    let beats = (t.max(0.0) / beat_s + 1e-4).floor() as i64;
    format!("Bar {}.{}", beats / 4 + 1, beats % 4 + 1)
}

/// What a property is called on a row.
pub fn prop_name(p: Prop) -> &'static str {
    match p {
        Prop::X => "X",
        Prop::Y => "Y",
        Prop::Z => "Z",
        Prop::Rotation => "Rotation",
        Prop::Tilt => "Tilt",
        Prop::Turn => "Turn",
        Prop::Scale => "Size",
        Prop::Width => "Width",
        Prop::Height => "Height",
        Prop::Glow => "Glow",
        Prop::Brightness => "Brightness",
        Prop::Opacity => "Opacity",
        Prop::Sides => "Sides",
        Prop::Thickness => "Thickness",
        Prop::Cone => "Cone",
        Prop::Rim => "Rim",
        Prop::Depth => "Depth",
        Prop::Density => "Density",
        Prop::Twinkle => "Twinkle",
        Prop::TwinkleRate => "Rate",
        Prop::Seed => "Seed",
    }
}

/// What a target is called on a row: the property, or the effect and
/// its parameter.
pub fn target_label(target: Target, fx: &Stack) -> String {
    match target {
        Target::Shape(p) => prop_name(p).to_string(),
        Target::Effect { id, param } => match fx
            .find(id)
            .and_then(|e| e.kind.params().get(param as usize).map(|s| (e.kind, s)))
        {
            Some((kind, spec)) => format!("{} · {}", kind.label(), spec.name),
            None => format!("effect {id}·{param}"),
        },
    }
}

/// A target's value the way the inspector would print it: angles in
/// degrees, a size as the full extent the S field speaks, an effect
/// parameter to its own precision.
pub fn fmt_target(target: Target, v: f32, fx: &Stack, canvas: [f32; 2], is_light: bool) -> String {
    match target {
        Target::Shape(p) if is_angle(p) => format!("{}°", fmt_number(v.to_degrees())),
        Target::Shape(Prop::Scale) => fmt_number(if is_light { v } else { v * 2.0 }),
        Target::Shape(p) => {
            let (lo, hi) = crate::props::range(p, canvas);
            if hi - lo <= 5.0 {
                format!("{v:.2}")
            } else {
                fmt_number(v)
            }
        }
        Target::Effect { id, param } => fx
            .find(id)
            .and_then(|e| e.kind.params().get(param as usize))
            .map(|spec| fmt_param(v, spec))
            .unwrap_or_else(|| fmt_number(v)),
    }
}

/// Whether a property's range is a real ceiling and floor (the graph can
/// stand on it) rather than a slider's reach.
fn bounded(p: Prop) -> bool {
    !matches!(
        p,
        Prop::Rotation
            | Prop::Tilt
            | Prop::Turn
            | Prop::Scale
            | Prop::Width
            | Prop::Height
            | Prop::Depth
            | Prop::Z
    )
}

/// A flat curve's window: how far either side of its one value the
/// graph opens, in the target's own units.
fn unit(target: Target, v: f32) -> f32 {
    match target {
        Target::Shape(p) if is_angle(p) => std::f32::consts::FRAC_PI_2,
        Target::Shape(Prop::Z) => 500.0,
        Target::Shape(_) => (v.abs() * 0.5).max(100.0),
        Target::Effect { .. } => 1.0,
    }
}

/// The value range the graph maps: a bounded property stands on its own
/// range (widened if a key sits outside it — X off the canvas), a free
/// one on its keys' reach with a quarter of air either side, and a flat
/// curve on a window around its value so it draws mid-graph.
pub fn value_span(target: Target, keys: &[Key], fx: &Stack, canvas: [f32; 2]) -> (f32, f32) {
    let (kmin, kmax) = keys
        .iter()
        .fold((f32::MAX, f32::MIN), |(a, b), k| (a.min(k.v), b.max(k.v)));
    let (kmin, kmax) = if keys.is_empty() {
        (0.0, 0.0)
    } else {
        (kmin, kmax)
    };
    let base = match target {
        Target::Shape(p) if bounded(p) => Some(crate::props::range(p, canvas)),
        Target::Shape(_) => None,
        Target::Effect { id, param } => fx
            .find(id)
            .and_then(|e| e.kind.params().get(param as usize))
            .map(|s| (s.min, s.max)),
    };
    let (lo, hi) = match base {
        Some((lo, hi)) => (lo.min(kmin), hi.max(kmax)),
        None => {
            let reach = kmax - kmin;
            if reach < 1e-4 {
                let d = unit(target, kmin);
                (kmin - d, kmax + d)
            } else {
                (kmin - reach * 0.25, kmax + reach * 0.25)
            }
        }
    };
    if hi - lo < 1e-6 {
        (lo - 1.0, hi + 1.0)
    } else {
        (lo, hi)
    }
}
