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
//! row off the list); Shift-click diamonds to pick several, Shift-drag
//! the graph to band them, Ctrl+A for the whole curve — a picked set
//! drags, deletes, copies and eases as one; with snap on a dragged
//! value lands on the graph's value rules or a magnet (`snap`), Ctrl
//! frees it; right-click a key, a moment, a row or the air for the
//! menu (`menu`: the value box, Linear|Smooth, Copy · Cut · Paste ·
//! Delete); drag the loop brace's end on the ruler to set how much of
//! the clip repeats; the ruler scrubs the song through the clip;
//! Ctrl+wheel zooms, Shift+wheel pans, the wheel over the sidebar
//! scrolls the rows; Ctrl+C copies the pick or (nothing picked) the
//! whole clip's keys, Ctrl+X cuts it, and Ctrl+V lands them at the
//! playhead — here or on any other object's clip, matched by setting
//! (`editor::curves::copy`). `K` still stamps into the active clip at
//! the playhead — new keys arrive on the graph live.

mod draw;
mod input;
mod labels;
mod menu;
mod page;
mod snap;
#[cfg(test)]
mod tests;
mod words;

pub use page::{Hit, Input, Page, Sel};
pub use words::{beat_label, fmt_target, keyable_targets, target_label};

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
    /// A picked set, dragged by one of its keys — the anchor — which the
    /// grab offsets are measured from.
    Set {
        anchor: (Target, usize),
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
    /// A selection band being dragged across the graph: where it was
    /// pressed and where the cursor is, physical px.
    pub band: Option<([f32; 2], [f32; 2])>,
    /// How far the rows are scrolled up, physical px.
    pub scroll: f32,
    last_press: Option<(Instant, [f32; 2])>,
}

/// The band's rectangle, from its two corners.
pub fn band_rect(b: ([f32; 2], [f32; 2])) -> Viewport {
    let (a, c) = b;
    Viewport {
        x: a[0].min(c[0]),
        y: a[1].min(c[1]),
        w: (a[0] - c[0]).abs(),
        h: (a[1] - c[1]).abs(),
    }
}

/// What the frame draws for the view, by batch, and what the shared
/// timeline painters need to draw the axis in clip-local time.
pub struct Frame {
    /// The clip itself: what maps local time back to the song (the
    /// waveform overlay draws through it).
    pub clip: ObjClip,
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

    /// Grid quantization in local time, while playhead snap is on.
    fn snap_local(&self, t: f32) -> f32 {
        if !self.snap_playhead {
            return t;
        }
        self.grid_div.snap(t, 0.0, self.grid().bpm)
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
            band: None,
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

    /// `K` and the diamond: the stamp, as the clip view shapes it. With
    /// the view open, **the shown setting alone** is stamped, moved or
    /// not — never the others, whatever moved with it (Alva,
    /// 2026-09-01: "it keeps making keyframes in other settings and
    /// makes a mess"). Nothing shown, nothing stamped. With the view
    /// closed, the arrangement's quick rule stands.
    pub(crate) fn stamp(&mut self) -> bool {
        // A key (or a moment) picked in the view: K updates *it* to the
        // settings as they stand, wherever the playhead is.
        let picked = self.clip_view.as_ref().and_then(|cv| {
            let i = self.editor.index_of(cv.obj)?;
            cv.sel.clone().map(|sel| (i, cv.c, sel))
        });
        if let Some((i, c, sel)) = picked {
            let done = match sel {
                Sel::Key { target, k } => self.editor.restamp_key(i, c, target, k),
                Sel::Time(t) => self.editor.restamp_keys_at(i, c, t),
                Sel::Keys(set) => self.editor.restamp_keys(i, c, &set),
            };
            self.export_note = Some(if done {
                "updated the picked key".to_string()
            } else {
                "the picked key already holds that value".to_string()
            });
            return done;
        }
        let shown: Option<(usize, Option<Target>)> = self.clip_view.as_ref().and_then(|cv| {
            let i = self.editor.index_of(cv.obj)?;
            Some((i, cv.target))
        });
        match shown {
            Some((i, Some(target))) => self.editor.stamp_only(i, &[target]),
            Some((_, None)) => {
                self.export_note = Some("pick a setting to key it".to_string());
                false
            }
            None => self.editor.stamp_keys(None, true),
        }
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
            let alive = |target: Target, k: usize| {
                clip.anim.track(target).is_some_and(|tr| k < tr.keys.len())
            };
            // A pick that outlived its keys is dropped; a set keeps the
            // keys it still has.
            let sel_fix: Option<Option<Sel>> = match &cv.sel {
                Some(Sel::Key { target, k }) => (!alive(*target, *k)).then_some(None),
                Some(Sel::Time(t)) => {
                    let has = clip
                        .anim
                        .key_times()
                        .iter()
                        .any(|(kt, _)| (kt - t).abs() < KEY_EPS);
                    (!has).then_some(None)
                }
                Some(Sel::Keys(set)) => {
                    let kept: Vec<(Target, usize)> =
                        set.iter().copied().filter(|&(t, k)| alive(t, k)).collect();
                    (kept.len() != set.len()).then(|| (!kept.is_empty()).then_some(Sel::Keys(kept)))
                }
                None => None,
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
                sel_fix,
            )
        });
        let Some((span, keyable, listed, sel_fix)) = facts else {
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
        if let Some(fixed) = sel_fix {
            cv.sel = fixed;
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
            sel: cv.sel.clone(),
            scroll: cv.scroll,
            playhead: clip.contains(t).then(|| clip.local(t)),
            frozen: cv.drag.as_ref().map(|d| d.span),
            band: cv.band.map(band_rect),
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
            clip: clip.clone(),
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
        Some(match &cv.sel {
            Some(Sel::Key { target, k }) => {
                let key = clip.anim.track(*target)?.keys.get(*k)?;
                format!(
                    "{} · {} · {}",
                    target_label(*target, shape, fx),
                    beat_label(key.t, bpm),
                    fmt_target(*target, key.v, fx, canvas, shape.is_light())
                )
            }
            Some(Sel::Time(t)) => {
                let n = clip
                    .anim
                    .tracks
                    .iter()
                    .filter(|tr| tr.keys.iter().any(|k| (k.t - t).abs() < KEY_EPS))
                    .count();
                format!("{n} keys · {}", beat_label(*t, bpm))
            }
            Some(Sel::Keys(set)) => format!("{} keys picked", set.len()),
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
}
