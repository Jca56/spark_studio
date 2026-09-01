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

use crate::anim::{Ease, Key, Target, prop_value};
use crate::arrange::{ROW_STEP, TRACK_TEXT};
use crate::chrome::{Align, Label, UI_TEXT};
use crate::doc::ObjClip;
use crate::fx::Stack;
use crate::inspector::{ROWS, fmt_number, fmt_param, is_angle, style_specs};
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
/// How near the loop brace's end a press on the ruler grabs it.
pub const BRACE_GRAB: f32 = 14.0;
/// The keyed-glyph column at a row's left, and where its name starts.
pub const ROW_GLYPH: f32 = 16.0;
pub const ROW_TEXT_X: f32 = 40.0;
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
            curve: Vec::new(),
            keys: Vec::new(),
            target: inp.target,
            color: inp.color,
            is_light: inp.shape.is_light(),
            ruler: panel.ruler,
            loop_end_x,
            wash,
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
        let sel_key = match inp.sel {
            Some(Sel::Key { target: st, k }) if st == target => Some(k),
            _ => None,
        };
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

/// A local time as a musician reads it: `Bar 2.3`, one-based.
pub fn beat_label(t: f32, bpm: f32) -> String {
    let beat_s = 60.0 / bpm.max(1.0);
    let beats = (t.max(0.0) / beat_s + 1e-4).floor() as i64;
    format!("Bar {}.{}", beats / 4 + 1, beats % 4 + 1)
}

/// A setting's value as the object stands — what a row shows before it
/// has keys, and what a first key is planted at.
pub fn current_value(shape: &Shape, fx: &Stack, target: Target) -> Option<f32> {
    match target {
        Target::Shape(p) => prop_value(shape, p),
        Target::Effect { id, param } => fx.find(id).map(|e| e.get(param as usize)),
    }
}

/// Every setting the object can key, in the order the inspector shows
/// them and wearing the inspector's words: the transform strip's rows
/// (`X Y Z`, `Tilt Turn Rot`, `S W H`, `D`), the Style sliders, then
/// each effect's parameters (a one-parameter effect is just its name —
/// `Glow`; otherwise `React · Scale`). A setting the object lacks is
/// left out, the way the inspector leaves it out.
pub fn keyable_targets(shape: &Shape, fx: &Stack) -> Vec<(Target, String)> {
    let mut out = Vec::new();
    for row in ROWS {
        for &(p, cap) in row.iter() {
            // A light is aimed, not spun — the inspector's own rule.
            let present = match p {
                Prop::Rotation => !shape.is_light(),
                _ => prop_value(shape, p).is_some(),
            };
            if present {
                out.push((Target::Shape(p), cap.to_string()));
            }
        }
    }
    for (p, name) in style_specs(shape) {
        // Glow is the Glow effect's parameter; it lists with the effects.
        if p != Prop::Glow && prop_value(shape, p).is_some() {
            out.push((Target::Shape(p), name.to_string()));
        }
    }
    for e in &fx.effects {
        let specs = e.kind.params();
        for (k, spec) in specs.iter().enumerate() {
            let label = if specs.len() == 1 {
                e.kind.label().to_string()
            } else {
                format!("{} · {}", e.kind.label(), spec.name)
            };
            out.push((
                Target::Effect {
                    id: e.id,
                    param: k as u8,
                },
                label,
            ));
        }
    }
    out
}

/// What a target is called: the inspector's word for it on this object.
pub fn target_label(target: Target, shape: &Shape, fx: &Stack) -> String {
    keyable_targets(shape, fx)
        .into_iter()
        .find(|(t, _)| *t == target)
        .map(|(_, l)| l)
        .unwrap_or_else(|| match target {
            Target::Shape(p) => prop_name(p).to_string(),
            Target::Effect { id, param } => format!("effect {id}·{param}"),
        })
}

/// The inspector's word for a property, for a target its object no
/// longer carries.
pub fn prop_name(p: Prop) -> &'static str {
    match p {
        Prop::X => "X",
        Prop::Y => "Y",
        Prop::Z => "Z",
        Prop::Rotation => "Rot",
        Prop::Tilt => "Tilt",
        Prop::Turn => "Turn",
        Prop::Scale => "S",
        Prop::Width => "W",
        Prop::Height => "H",
        Prop::Glow => "Glow",
        Prop::Brightness => "Brightness",
        Prop::Opacity => "Opacity",
        Prop::Sides => "Sides",
        Prop::Thickness => "Thickness",
        Prop::Cone => "Cone",
        Prop::Rim => "Rim",
        Prop::Depth => "D",
        Prop::Density => "Density",
        Prop::Twinkle => "Twinkle",
        Prop::TwinkleRate => "Rate",
        Prop::Seed => "Seed",
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
