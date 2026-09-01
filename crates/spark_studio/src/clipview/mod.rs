//! The clip curve view — ④ of the object/clip build order (Alva's
//! spec, 2026-08-31): double-click an object's clip and the bottom
//! panel becomes that clip's editor, the piano-roll analog. Ableton's
//! clip envelopes, Alva's pick: the sidebar lists the clip's keyed
//! settings — **and whatever you touch in the inspector while the view
//! is open**, which is how you pick what to keyframe (Alva's second
//! call, after the first cut listed only what was already keyed and
//! the last delete left nothing to key) — the chosen one's curve fills
//! the axis, a key strip under the ruler carries every moment across
//! every track. Time is clip-local. The breadcrumb plate or Esc goes
//! back.
//!
//! Gestures: a row shows its curve (a listed setting with no keys yet is
//! a flat line at its value — double-click it to plant the first key);
//! drag a diamond to move a key in time and value (snap rides the
//! playhead-snap toggle); drag a strip diamond to retime every key at
//! that moment; double-click the graph to add a key on the line; Delete
//! removes what is picked (or, with nothing picked, takes an unkeyed
//! row off the list); right-click a key to flip it between smooth and
//! linear; drag the loop brace's end on the ruler to set how much of
//! the clip repeats; the ruler scrubs the song through the clip;
//! Ctrl+wheel zooms, Shift+wheel pans, the wheel over the sidebar
//! scrolls the rows. `K` still stamps into the active clip at the
//! playhead — new keys arrive on the graph live.

mod draw;
mod page;
#[cfg(test)]
mod tests;

pub use page::{Hit, Input, Page, Sel, beat_label, fmt_target, keyable_targets, target_label};

use std::time::Instant;

use spark_audio::BeatGrid;
use spark_render::Viewport;
use spark_ui::UiRect;

use crate::Studio;
use crate::anim::{KEY_EPS, Target};
use crate::arrange::ClipRef;
use crate::chrome::Label;
use crate::doc::ObjClip;
use crate::timeline::{Panel, TimeView};

/// A second press this soon and this near the first is a double-click.
const DOUBLE_MS: u128 = 400;
const DOUBLE_PX: f32 = 8.0;

/// What a drag is moving.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum DragKind {
    Key {
        target: Target,
        k: usize,
    },
    /// Every key at a moment; `t` follows them as they move.
    Time {
        t: f32,
    },
    /// The loop brace's end: how much of the clip repeats.
    Loop,
}

/// A key drag in progress.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Drag {
    pub kind: DragKind,
    /// The graph's value span at the press, held still through the drag.
    pub span: (f32, f32),
    /// Where inside the key the cursor grabbed it, so it doesn't jump.
    pub grab_dt: f32,
    pub grab_dv: f32,
    pub moved: bool,
}

/// The view's own state, on the studio while it is open.
pub struct State {
    /// The clip, by its object's id and its index on that object.
    pub obj: u32,
    pub c: usize,
    /// Settings listed without keys yet — picked from the inspector
    /// while the view was open. Session state; a key makes it real.
    pub armed: Vec<Target>,
    /// Whose curve the graph shows.
    pub target: Option<Target>,
    /// The visible slice of clip-local time.
    pub view: TimeView,
    pub sel: Option<Sel>,
    pub drag: Option<Drag>,
    pub over: Option<Hit>,
    /// How far the rows are scrolled up, physical px.
    pub scroll: f32,
    last_press: Option<(Instant, [f32; 2])>,
}

/// What the frame draws for the view, by batch, and what the shared
/// timeline painters need to draw the axis in clip-local time.
pub struct Frame {
    pub sidebar: Vec<UiRect>,
    pub rows: Vec<UiRect>,
    pub rows_clip: Viewport,
    pub axis: Vec<UiRect>,
    /// On the ruler, over the brace: the loop end's grip.
    pub ruler: Vec<UiRect>,
    pub labels: Vec<Label>,
    pub marks: Vec<(f32, String)>,
    pub view: TimeView,
    pub grid: BeatGrid,
    pub span: f32,
    /// The playhead in local time, while the song is inside the clip.
    pub playhead: Option<f32>,
    /// The brace on the ruler: the loop, lit; or what plays, dimmed.
    pub brace: ((f32, f32), bool),
}

/// How much local time the view can show: the clip's whole span (its
/// loop too, if that runs longer) or the last key, whichever is later,
/// plus a bar of air.
pub fn content_span(clip: &ObjClip, bar_s: f32) -> f32 {
    let plays = if clip.loop_on {
        clip.loop_len.max(clip.offset + clip.len)
    } else {
        clip.offset + clip.len
    };
    let last = clip.anim.key_times().last().map(|(t, _)| *t).unwrap_or(0.0);
    plays.max(last) + bar_s.max(0.5)
}

/// The song time at which the clip plays local time `lt` — the first
/// pass that lands inside the clip, clamped to it. The inverse of
/// [`ObjClip::local`], for scrubbing the song from the clip's ruler.
pub fn song_time_for(clip: &ObjClip, lt: f32) -> f32 {
    let mut t = clip.start - clip.offset + lt;
    if clip.loop_on && t < clip.start {
        let p = clip.loop_len.max(0.001);
        t += ((clip.start - t) / p).ceil() * p;
    }
    t.clamp(clip.start, (clip.end() - 1e-3).max(clip.start))
}

/// Whether a track carries keys on this clip.
fn keyed(clip: &ObjClip, target: Target) -> bool {
    clip.anim
        .track(target)
        .is_some_and(|tr| !tr.keys.is_empty())
}

impl Studio {
    /// The viewed clip, resolved fresh: its object's index and itself.
    /// `None` once either is gone — the view closes on the next tick.
    fn clip_view_clip(&self) -> Option<(usize, &ObjClip)> {
        let cv = self.clip_view.as_ref()?;
        let i = self.editor.index_of(cv.obj)?;
        let clip = self.editor.obj_clips(i).get(cv.c)?;
        Some((i, clip))
    }

    /// The rows the view lists: every keyed setting plus the armed ones,
    /// in the inspector's order and wearing its words.
    fn clip_view_listed(&self) -> Vec<(Target, String)> {
        let (Some(cv), Some((i, clip))) = (self.clip_view.as_ref(), self.clip_view_clip()) else {
            return Vec::new();
        };
        keyable_targets(&self.editor.shapes()[i], self.editor.fx_of(i))
            .into_iter()
            .filter(|(t, _)| keyed(clip, *t) || cv.armed.contains(t))
            .collect()
    }

    /// The clip's own grid: the comp's tempo, bar one at local zero.
    fn local_grid(&self) -> BeatGrid {
        BeatGrid {
            bpm: self.grid().bpm,
            first_bar: 0.0,
        }
    }

    /// Beat quantization in local time, while playhead snap is on.
    fn snap_local(&self, t: f32) -> f32 {
        if !self.snap_playhead {
            return t;
        }
        let beat_s = 60.0 / self.grid().bpm.max(1.0);
        (t / beat_s).round() * beat_s
    }

    /// Double-clicking an object clip: the bottom panel becomes its
    /// curve view, opened on the whole clip, showing its first keyed
    /// setting. Nothing keyed: an empty list, and the inspector picks.
    pub(crate) fn open_clip_view(&mut self, obj: u32, c: usize) {
        let Some(i) = self.editor.index_of(obj) else {
            return;
        };
        let Some(clip) = self.editor.obj_clips(i).get(c) else {
            return;
        };
        let span = content_span(clip, self.editor.bar_s);
        let target = clip
            .anim
            .tracks
            .iter()
            .find(|t| !t.keys.is_empty())
            .map(|t| t.target);
        self.clip_view = Some(State {
            obj,
            c,
            armed: Vec::new(),
            target,
            view: TimeView::new(0.0, span),
            sel: None,
            drag: None,
            over: None,
            scroll: 0.0,
            last_press: None,
        });
        self.selected_clip = Some(ClipRef::Obj { obj, c });
        self.clip_drag = None;
    }

    /// Back to the arrangement. True when there was a view to close.
    pub(crate) fn close_clip_view(&mut self) -> bool {
        self.clip_view.take().is_some()
    }

    /// The inspector picks what the view lists: a press on one of its
    /// fields or sliders while the view is open adds that setting as a
    /// row and shows its curve — flat at its value until a key lands.
    /// False when no view is open.
    pub(crate) fn clip_view_arm(&mut self, target: Target) -> bool {
        let Some(cv) = self.clip_view.as_mut() else {
            return false;
        };
        if !cv.armed.contains(&target) {
            cv.armed.push(target);
        }
        cv.target = Some(target);
        cv.sel = None;
        true
    }

    /// Housekeeping before a frame: the view closes if its clip is gone
    /// (deleted, undone away, another project); an armed setting the
    /// object lost (its effect removed) drops off the list; the shown
    /// setting falls back to the first listed when its own left; a stale
    /// key pick is dropped; and the time window and the scroll stay
    /// inside the content.
    pub(crate) fn clip_view_tick(&mut self, panel: &Panel, scale: f32) {
        let facts = self.clip_view_clip().map(|(i, clip)| {
            let cv = self.clip_view.as_ref().expect("open");
            let sel_ok = match cv.sel {
                Some(Sel::Key { target, k }) => {
                    clip.anim.track(target).is_some_and(|tr| k < tr.keys.len())
                }
                Some(Sel::Time(t)) => clip
                    .anim
                    .key_times()
                    .iter()
                    .any(|(kt, _)| (kt - t).abs() < KEY_EPS),
                None => true,
            };
            let keyable: Vec<Target> =
                keyable_targets(&self.editor.shapes()[i], self.editor.fx_of(i))
                    .into_iter()
                    .map(|(t, _)| t)
                    .collect();
            let listed: Vec<Target> = keyable
                .iter()
                .copied()
                .filter(|t| keyed(clip, *t) || cv.armed.contains(t))
                .collect();
            (
                content_span(clip, self.editor.bar_s),
                keyable,
                listed,
                sel_ok,
            )
        });
        let Some((span, keyable, listed, sel_ok)) = facts else {
            self.clip_view = None;
            return;
        };
        let max_scroll = self
            .clip_view_page(panel, scale)
            .map(|p| p.max_scroll())
            .unwrap_or(0.0);
        let Some(cv) = self.clip_view.as_mut() else {
            return;
        };
        cv.armed.retain(|t| keyable.contains(t));
        if cv.target.is_none_or(|t| !listed.contains(&t)) {
            cv.target = listed.first().copied();
        }
        if !sel_ok {
            cv.sel = None;
        }
        cv.view.zoom(1.0, cv.view.t0, span);
        cv.scroll = cv.scroll.clamp(0.0, max_scroll);
    }

    /// The page for this frame's layout and state — the paired builder
    /// the hit tests and the paint both read.
    fn clip_view_page(&self, panel: &Panel, scale: f32) -> Option<Page> {
        let cv = self.clip_view.as_ref()?;
        let (i, clip) = self.clip_view_clip()?;
        let name = self.editor.display_name(i);
        let listed = self.clip_view_listed();
        let shape = &self.editor.shapes()[i];
        let t = self.editor.time();
        let inp = Input {
            clip,
            name: &name,
            color: shape.rgb(),
            fx: self.editor.fx_of(i),
            canvas: self.editor.canvas(),
            shape,
            listed: &listed,
            bpm: self.grid().bpm,
            target: cv.target,
            sel: cv.sel,
            scroll: cv.scroll,
            playhead: clip.contains(t).then(|| clip.local(t)),
            frozen: cv.drag.as_ref().map(|d| d.span),
        };
        Some(Page::build(panel, &cv.view, scale, &inp))
    }

    /// Everything the frame draws for the view, while it is open.
    pub(crate) fn clip_view_frame(&self, panel: &Panel, scale: f32) -> Option<Frame> {
        let cv = self.clip_view.as_ref()?;
        let (i, clip) = self.clip_view_clip()?;
        let page = self.clip_view_page(panel, scale)?;
        let r = draw::rects(&page, cv.over);
        let grid = self.local_grid();
        let span = content_span(clip, self.editor.bar_s);
        let t = self.editor.time();
        let brace = if clip.loop_on {
            ((0.0, clip.loop_len), true)
        } else {
            ((clip.offset, clip.offset + clip.len), false)
        };
        Some(Frame {
            sidebar: r.sidebar,
            rows: r.rows,
            rows_clip: page.rows_clip,
            axis: r.axis,
            ruler: r.ruler,
            labels: page.labels(cv.over, self.editor.fx_of(i), self.editor.canvas()),
            marks: crate::timeline::ruler_marks(panel, &cv.view, scale, &grid, span),
            view: cv.view,
            grid,
            span,
            playhead: clip.contains(t).then(|| clip.local(t)),
            brace,
        })
    }

    /// The status strip's line while the view is open: the picked key
    /// and its numbers, the picked moment, or the clip and its key
    /// count — and, with nothing listed, where the settings come from.
    pub(crate) fn clip_view_status(&self) -> Option<String> {
        let cv = self.clip_view.as_ref()?;
        let (i, clip) = self.clip_view_clip()?;
        let fx = self.editor.fx_of(i);
        let canvas = self.editor.canvas();
        let shape = &self.editor.shapes()[i];
        let bpm = self.grid().bpm;
        Some(match cv.sel {
            Some(Sel::Key { target, k }) => {
                let key = clip.anim.track(target)?.keys.get(k)?;
                format!(
                    "{} · {} · {}",
                    target_label(target, shape, fx),
                    beat_label(key.t, bpm),
                    fmt_target(target, key.v, fx, canvas, shape.is_light())
                )
            }
            Some(Sel::Time(t)) => {
                let n = clip
                    .anim
                    .tracks
                    .iter()
                    .filter(|tr| tr.keys.iter().any(|k| (k.t - t).abs() < KEY_EPS))
                    .count();
                format!("{n} keys · {}", beat_label(t, bpm))
            }
            None => {
                let n: usize = clip.anim.tracks.iter().map(|t| t.keys.len()).sum();
                if n == 0 && cv.armed.is_empty() {
                    format!(
                        "{} · clip {} — touch a setting in the inspector to list it here",
                        self.editor.display_name(i),
                        cv.c + 1
                    )
                } else {
                    format!(
                        "{} · clip {} · {n} keys",
                        self.editor.display_name(i),
                        cv.c + 1
                    )
                }
            }
        })
    }

    /// Scrub the song to the local time under `x` on the clip's ruler.
    pub(crate) fn clip_scrub_x(&mut self, panel: &Panel, x: f32) {
        let Some((_, clip)) = self.clip_view_clip() else {
            return;
        };
        let clip = clip.clone();
        let Some(cv) = self.clip_view.as_ref() else {
            return;
        };
        let lt = self.snap_local(cv.view.t_at(x, panel.axis)).max(0.0);
        let t = song_time_for(&clip, lt).clamp(self.grid().first_bar, self.duration());
        self.seek(t);
    }

    /// A left press on the bottom panel while the view is open. The
    /// loop brace's end starts its drag; the rest of the ruler scrubs;
    /// the breadcrumb goes back; a row shows its curve; a diamond picks
    /// its key (or moment) and starts a drag; a double-click on the
    /// graph adds a key on the line; the air drops the pick. True
    /// whenever the press was on the panel — nothing falls through to
    /// the arrangement underneath.
    pub(crate) fn clip_view_press(&mut self, panel: &Panel, scale: f32, cx: f32, cy: f32) -> bool {
        if self.clip_view.is_none() {
            return false;
        }
        let on_panel = panel.ruler.contains(cx, cy)
            || panel.lanes.contains(cx, cy)
            || panel.names_box.contains(cx, cy)
            || panel.gutter.contains(cx, cy);
        if !on_panel {
            return false;
        }
        let Some(page) = self.clip_view_page(panel, scale) else {
            return true;
        };
        let hit = page.hit(cx, cy);
        if hit == Some(Hit::LoopEnd) {
            if let Some(cv) = self.clip_view.as_mut() {
                cv.drag = Some(Drag {
                    kind: DragKind::Loop,
                    span: page.span,
                    grab_dt: 0.0,
                    grab_dv: 0.0,
                    moved: false,
                });
            }
            self.request_redraw();
            return true;
        }
        if panel.ruler.contains(cx, cy) {
            self.clip_scrub_x(panel, cx);
            self.timeline_scrub = true;
            self.request_redraw();
            return true;
        }
        let now = Instant::now();
        let (double, t_cursor, c) = {
            let cv = self.clip_view.as_ref().expect("open");
            let double = cv.last_press.is_some_and(|(t0, p)| {
                now.duration_since(t0).as_millis() < DOUBLE_MS
                    && (p[0] - cx).abs() < DOUBLE_PX * scale
                    && (p[1] - cy).abs() < DOUBLE_PX * scale
            });
            (double, cv.view.t_at(cx, panel.axis), cv.c)
        };
        let i = self.clip_view_clip().map(|(i, _)| i);
        match hit {
            Some(Hit::Back) => {
                self.close_clip_view();
                self.request_redraw();
                return true;
            }
            Some(Hit::Row(k)) => {
                if let (Some(row), Some(cv)) = (page.rows.get(k), self.clip_view.as_mut()) {
                    cv.target = Some(row.target);
                    cv.sel = None;
                }
            }
            Some(Hit::Key(k)) => {
                let v_cursor = page.value_at(cy);
                if let (Some(d), Some(cv)) = (page.keys.get(k), self.clip_view.as_mut()) {
                    cv.sel = Some(Sel::Key {
                        target: d.target,
                        k: d.k,
                    });
                    cv.drag = Some(Drag {
                        kind: DragKind::Key {
                            target: d.target,
                            k: d.k,
                        },
                        span: page.span,
                        grab_dt: t_cursor - d.t,
                        grab_dv: v_cursor - d.v,
                        moved: false,
                    });
                }
            }
            Some(Hit::StripKey(k)) => {
                if let (Some(d), Some(cv)) = (page.strip_dots.get(k), self.clip_view.as_mut()) {
                    cv.sel = Some(Sel::Time(d.t));
                    cv.drag = Some(Drag {
                        kind: DragKind::Time { t: d.t },
                        span: page.span,
                        grab_dt: t_cursor - d.t,
                        grab_dv: 0.0,
                        moved: false,
                    });
                }
            }
            Some(Hit::Graph) => {
                let added = match (double, page.target, i) {
                    (true, Some(target), Some(i)) => {
                        let t = self.snap_local(t_cursor).max(0.0);
                        self.editor
                            .add_key(i, c, target, t)
                            .map(|k| Sel::Key { target, k })
                    }
                    _ => None,
                };
                if let Some(cv) = self.clip_view.as_mut() {
                    cv.sel = added;
                }
            }
            Some(Hit::Strip) | Some(Hit::LoopEnd) | None => {
                if let Some(cv) = self.clip_view.as_mut() {
                    cv.sel = None;
                }
            }
        }
        if let Some(cv) = self.clip_view.as_mut() {
            cv.last_press = Some((now, [cx, cy]));
        }
        self.request_redraw();
        true
    }

    /// A right press while the view is open: on a key, flips its ease
    /// between smooth and linear. True when the press was on the panel.
    pub(crate) fn clip_view_right_press(&mut self, cx: f32, cy: f32) -> bool {
        if self.clip_view.is_none() {
            return false;
        }
        let Some(layout) = self.layout() else {
            return false;
        };
        if !layout.timeline.contains(cx, cy) {
            return false;
        }
        let scale = self.scale();
        let panel = crate::timeline::panel(layout.timeline, scale);
        let Some(page) = self.clip_view_page(&panel, scale) else {
            return true;
        };
        if let Some(Hit::Key(k)) = page.hit(cx, cy)
            && let Some(d) = page.keys.get(k)
            && let Some(i) = self.clip_view_clip().map(|(i, _)| i)
        {
            let c = self.clip_view.as_ref().map(|cv| cv.c).unwrap_or(0);
            if self.editor.toggle_key_ease(i, c, d.target, d.k) {
                self.export_note = Some(format!(
                    "{} key is {}",
                    target_label(d.target, &self.editor.shapes()[i], self.editor.fx_of(i)),
                    if d.linear { "smooth" } else { "linear" }
                ));
                self.request_redraw();
            }
        }
        true
    }

    /// The cursor moved: a held diamond follows it — a key in time and
    /// value, a moment in time, the loop's end in time — otherwise what's
    /// under the cursor lights. True when the frame needs redrawing.
    pub(crate) fn clip_view_moved(&mut self, panel: &Panel, mx: f32, my: f32) -> bool {
        if self.clip_view.is_none() {
            return false;
        }
        let scale = self.scale();
        let Some(page) = self.clip_view_page(panel, scale) else {
            return false;
        };
        let (drag, t_cursor, c) = {
            let cv = self.clip_view.as_ref().expect("open");
            (cv.drag, cv.view.t_at(mx, panel.axis), cv.c)
        };
        let i = self.clip_view_clip().map(|(i, _)| i);
        if let (Some(d), Some(i)) = (drag, i) {
            let t = self.snap_local(t_cursor - d.grab_dt).max(0.0);
            let dirty = match d.kind {
                DragKind::Key { target, k } => {
                    let v = page.value_at(my) - d.grab_dv;
                    self.editor.move_key(i, c, target, k, t, v)
                }
                DragKind::Loop => {
                    // At least a beat; snap rides the playhead-snap toggle.
                    let beat = 60.0 / self.grid().bpm.max(1.0);
                    let len = self.snap_local(t_cursor).max(beat);
                    self.editor.set_obj_clip_loop_len(i, c, len)
                }
                DragKind::Time { t: from } => match self.editor.retime_keys_at(i, c, from, t) {
                    Some(landed) => {
                        if let Some(cv) = self.clip_view.as_mut() {
                            if let Some(dr) = &mut cv.drag {
                                dr.kind = DragKind::Time { t: landed };
                            }
                            cv.sel = Some(Sel::Time(landed));
                        }
                        (landed - from).abs() > 1e-6
                    }
                    None => false,
                },
            };
            if let Some(dr) = self.clip_view.as_mut().and_then(|cv| cv.drag.as_mut()) {
                dr.moved = true;
            }
            return dirty;
        }
        let on_panel = panel.lanes.contains(mx, my)
            || panel.names_box.contains(mx, my)
            || panel.ruler.contains(mx, my);
        let over = if on_panel { page.hit(mx, my) } else { None };
        let Some(cv) = self.clip_view.as_mut() else {
            return false;
        };
        let dirty = over != cv.over;
        cv.over = over;
        dirty
    }

    /// The button came up: a drag ends (the gesture's undo step closes
    /// with the release). True when one was running.
    pub(crate) fn clip_view_release(&mut self) -> bool {
        self.clip_view
            .as_mut()
            .and_then(|cv| cv.drag.take())
            .is_some()
    }

    /// The wheel over the bottom panel while the view is open: Ctrl
    /// zooms local time at the cursor, Shift (or a plain wheel over the
    /// axis) pans it, a plain wheel over the sidebar scrolls the rows.
    pub(crate) fn clip_view_wheel(&mut self, panel: &Panel, cx: f32, cy: f32, dy: f32) -> bool {
        let Some((_, clip)) = self.clip_view_clip() else {
            return false;
        };
        let span = content_span(clip, self.editor.bar_s);
        let scale = self.scale();
        let ctrl = self.modifiers.control_key();
        let shift = self.modifiers.shift_key();
        let over_names = panel.names_box.contains(cx, cy) || panel.gutter.contains(cx, cy);
        let Some(cv) = self.clip_view.as_mut() else {
            return false;
        };
        if ctrl {
            let pivot = cv.view.t_at(cx, panel.axis);
            cv.view.zoom((1.0f32 / 1.18).powf(dy), pivot, span);
        } else if shift || !over_names {
            let dt = -dy * cv.view.span() * 0.10;
            cv.view.pan(dt, span);
        } else {
            cv.scroll = (cv.scroll - dy * 60.0 * scale).max(0.0);
        }
        true
    }

    /// Delete while the view is open acts on the pick: the key, or every
    /// key at the moment. With nothing picked, an unkeyed row (a listing
    /// and nothing more) comes off the list; a keyed one says to pick a
    /// key. Never reaches past the view for the clip or the object. True
    /// when something changed.
    pub(crate) fn clip_view_delete(&mut self) -> bool {
        let Some(cv) = self.clip_view.as_ref() else {
            return false;
        };
        let (c, sel, shown) = (cv.c, cv.sel, cv.target);
        let Some((i, clip)) = self.clip_view_clip() else {
            return false;
        };
        let shown_unkeyed = shown.is_some_and(|t| !keyed(clip, t));
        let done = match sel {
            Some(Sel::Key { target, k }) => self.editor.delete_key(i, c, target, k),
            Some(Sel::Time(t)) => self.editor.delete_keys_at(i, c, t),
            None => match (shown, shown_unkeyed) {
                (Some(t), true) => {
                    if let Some(cv) = self.clip_view.as_mut() {
                        cv.armed.retain(|a| *a != t);
                        cv.target = None;
                    }
                    true
                }
                _ => {
                    self.export_note = Some("pick a key to delete it".to_string());
                    false
                }
            },
        };
        if done && let Some(cv) = self.clip_view.as_mut() {
            cv.sel = None;
        }
        done
    }
}
