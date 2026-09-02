//! Moving clips: the grabbed one, and every other selected clip with
//! it. A body drag carries the whole selection by one amount — the
//! grabbed clip's snapped move — so Ctrl+A and a drag shoves the
//! arrangement over to make room for an intro without an "insert
//! time" command (Alva, 2026-09-02: "couldn't I just… move everything
//! to the right?"). Edges trim the grabbed clip alone.
//!
//! **The song's beats stay on the grid.** With the song in the move,
//! snap works in whole grid steps from where the clips started rather
//! than onto grid lines: the song's *file* start is a pickup before its
//! first bar, so landing the file's edge on a bar line would put every
//! beat off by that pickup — and the grid's phase follows the song, so
//! snapping onto lines that move with the drag would snap to nothing.

use super::{CLIP_DRAG_START, ClipDrag, ClipRef, Zone};
use crate::Studio;
use crate::doc::SONG;
use crate::timeline::Panel;

impl Studio {
    /// The cursor moved with a clip held: once it has travelled, the
    /// body moves the selection or an edge trims the grabbed clip.
    /// True when something changed.
    pub(crate) fn clip_drag_moved(&mut self, panel: &Panel, mx: f32) -> bool {
        let start_px = CLIP_DRAG_START * self.scale();
        let Some(d) = self.clip_drag.as_mut() else {
            return false;
        };
        if (mx - d.press_x).abs() >= start_px {
            d.moved = true;
        }
        if !d.moved {
            return false;
        }
        let d = d.clone();
        let t_raw = self.time_view.t_at(mx, panel.axis);
        match d.zone {
            Zone::Move => self.move_group(&d, t_raw),
            Zone::Left | Zone::Right => self.trim_clip(d.r, d.zone, t_raw),
        }
    }

    /// A clip's start and length as they stand — an audio clip's
    /// length through its file.
    pub(crate) fn clip_span_of(&self, r: ClipRef) -> Option<(f32, f32)> {
        match r {
            ClipRef::Obj { obj, c } => {
                let i = self.editor.index_of(obj)?;
                let cl = self.editor.obj_clips(i).get(c)?;
                Some((cl.start, cl.len))
            }
            ClipRef::Comp(i) => self.editor.comp_clips().get(i).map(|c| (c.start, c.len)),
            ClipRef::Audio(k) => {
                let c = self.audio_editor().audio_clips().get(k)?;
                Some((c.start, self.clip_span(c)))
            }
        }
    }

    fn is_song_clip(&self, r: ClipRef) -> bool {
        match r {
            ClipRef::Audio(k) => self
                .audio_editor()
                .audio_clips()
                .get(k)
                .is_some_and(|c| c.asset == SONG),
            _ => false,
        }
    }

    /// `x` rounded to the grid's step, while snap is on — a *relative*
    /// snap, for moves that must keep their phase.
    fn snap_step(&self, x: f32) -> f32 {
        if !self.snap_playhead {
            return x;
        }
        let step = self.grid_div.step_s(self.grid().bpm);
        (x / step).round() * step
    }

    /// Move every clip in the drag's group by the grabbed clip's move.
    fn move_group(&mut self, d: &ClipDrag, t_raw: f32) -> bool {
        let want = t_raw - d.grab_dt;
        let song_moves = d.group.iter().any(|(r, _)| self.is_song_clip(*r));
        let target = if song_moves {
            d.orig + self.snap_step(want - d.orig)
        } else {
            self.snap_time(want)
        };
        let (lo, hi) = self.group_bounds(&d.group);
        let dt = if lo <= hi {
            (target - d.orig).clamp(lo, hi)
        } else {
            0.0
        };
        // Far end first for a move right, near end first for a move
        // left, so nobody in the group walks into a group-mate still
        // sitting where it was.
        let mut members = d.group.clone();
        members.sort_by(|a, b| a.1.total_cmp(&b.1));
        if dt > 0.0 {
            members.reverse();
        }
        let mut changed = false;
        for (r, orig) in members {
            let Some((_, len)) = self.clip_span_of(r) else {
                continue;
            };
            changed |= self.set_span(r, orig + dt, len);
        }
        changed
    }

    /// How far the group may move: nobody before zero, nobody into a
    /// clip on its own track that isn't coming along.
    fn group_bounds(&self, group: &[(ClipRef, f32)]) -> (f32, f32) {
        let mut lo = f32::MIN;
        let mut hi = f32::MAX;
        let in_group = |r: ClipRef| group.iter().any(|(g, _)| *g == r);
        for &(r, _) in group {
            let Some((start, len)) = self.clip_span_of(r) else {
                continue;
            };
            let end = start + len;
            lo = lo.max(-start);
            // The neighbours on the same track, as (start, end).
            let others: Vec<(f32, f32)> = match r {
                ClipRef::Obj { obj, c } => match self.editor.index_of(obj) {
                    Some(i) => self
                        .editor
                        .obj_clips(i)
                        .iter()
                        .enumerate()
                        .filter(|(j, _)| *j != c && !in_group(ClipRef::Obj { obj, c: *j }))
                        .map(|(_, o)| (o.start, o.end()))
                        .collect(),
                    None => Vec::new(),
                },
                ClipRef::Audio(k) => {
                    let ed = self.audio_editor();
                    let asset = ed.audio_clips().get(k).map(|c| c.asset);
                    ed.audio_clips()
                        .iter()
                        .enumerate()
                        .filter(|(j, o)| {
                            *j != k && Some(o.asset) == asset && !in_group(ClipRef::Audio(*j))
                        })
                        .map(|(_, o)| (o.start, o.start + self.clip_span(o)))
                        .collect()
                }
                // Comp clips share a track freely.
                ClipRef::Comp(_) => Vec::new(),
            };
            for (os, oe) in others {
                if oe <= start + 1e-4 {
                    lo = lo.max(oe - start);
                } else if os >= end - 1e-4 {
                    hi = hi.min(os - end);
                }
            }
        }
        (lo, hi)
    }

    /// Trim the grabbed clip's edge to the (snapped) cursor.
    fn trim_clip(&mut self, r: ClipRef, zone: Zone, t_raw: f32) -> bool {
        let Some((start, len)) = self.clip_span_of(r) else {
            return false;
        };
        let end = start + len;
        let t = self.snap_time(t_raw);
        let (s, l) = match zone {
            Zone::Left => {
                let s = t.clamp(0.0, end - 0.05);
                (s, end - s)
            }
            _ => {
                let e = t.max(start + 0.05);
                (start, e - start)
            }
        };
        self.set_span(r, s, l)
    }

    /// Set a clip's span through its editor op. Audio is the project's,
    /// read-only while a placed comp is being edited.
    fn set_span(&mut self, r: ClipRef, start: f32, len: f32) -> bool {
        match r {
            ClipRef::Obj { obj, c } => match self.editor.index_of(obj) {
                Some(i) => self.editor.set_obj_clip_span(i, c, start, len),
                None => false,
            },
            ClipRef::Comp(i) => match self.editor.comp_clips().get(i) {
                Some(c) => self.editor.set_clip_span(i, c.track, start, len),
                None => false,
            },
            ClipRef::Audio(k) => {
                if self.in_comp() {
                    return false;
                }
                let Some(c) = self.editor.audio_clips().get(k).copied() else {
                    return false;
                };
                let file_len = self.file_len(c.asset);
                // A whole-file clip stays whole when only moved: hand the
                // op its resolved span, and it keeps the zero for us.
                self.editor.set_audio_clip_span(k, start, len, file_len)
            }
        }
    }
}
