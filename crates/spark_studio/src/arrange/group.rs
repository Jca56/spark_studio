//! Moving clips: the grabbed one, and every other selected clip with
//! it. A body drag carries the whole selection by one amount — the
//! grabbed clip's snapped move — so Ctrl+A and a drag shoves the
//! arrangement over to make room for an intro without an "insert
//! time" command (Alva, 2026-09-02: "couldn't I just… move everything
//! to the right?"). Edges trim the grabbed clip alone.
//!
//! **The grid never moves; the song moves onto it.** When the grabbed
//! clip is the song, what snaps is its *first bar* — the file's start
//! is a pickup before it, and landing the file's edge on a bar line
//! would put every beat off by that pickup. Anything else snaps its
//! start, and the rest of the selection follows by the same amount.
//!
//! Every distance here is measured from where the clips *started* the
//! drag, never from where they are mid-drag: bounds read against the
//! current position while the move is applied from the original one
//! let each step left go only half as far as the last (Alva: "moving
//! it back left it like fights back").

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

    /// Where the grabbed clip's start would land for a wanted start:
    /// the song by its first bar, anything else by its start.
    fn snapped_start(&self, r: ClipRef, want: f32) -> f32 {
        if self.is_song_clip(r)
            && let (Some(first), Some((start, _))) = (self.song_first_bar(), self.clip_span_of(r))
        {
            // The first bar's offset from the clip's start is fixed by
            // the file; snap the bar, carry the start along.
            let lead = first - start;
            return self.snap_time(want + lead) - lead;
        }
        self.snap_time(want)
    }

    /// Move every clip in the drag's group by the grabbed clip's move.
    fn move_group(&mut self, d: &ClipDrag, t_raw: f32) -> bool {
        let want = t_raw - d.grab_dt;
        let target = self.snapped_start(d.r, want);
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

    /// How far the group may move from where it started: nobody before
    /// zero, nobody into a clip on its own track that isn't coming
    /// along. Measured from the drag's original starts (see the module
    /// note).
    fn group_bounds(&self, group: &[(ClipRef, f32)]) -> (f32, f32) {
        let in_group = |r: ClipRef| group.iter().any(|(g, _)| *g == r);
        let mut members = Vec::new();
        let mut neighbours = Vec::new();
        for &(r, orig) in group {
            let Some((_, len)) = self.clip_span_of(r) else {
                continue;
            };
            members.push((orig, orig + len));
            // The clips on the same track that stay put, as (start, end).
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
            neighbours.push(others);
        }
        dt_bounds(&members, &neighbours)
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

/// The range a group may move by, from its members' original spans
/// (`(start, end)` each) and, per member, the spans on its track that
/// stay put: no member before zero, none into a stayer. Pure, so the
/// arithmetic that once fought back can be held to account.
pub(super) fn dt_bounds(members: &[(f32, f32)], neighbours: &[Vec<(f32, f32)>]) -> (f32, f32) {
    let mut lo = f32::MIN;
    let mut hi = f32::MAX;
    for (k, &(start, end)) in members.iter().enumerate() {
        lo = lo.max(-start);
        for &(os, oe) in neighbours.get(k).map(Vec::as_slice).unwrap_or(&[]) {
            if oe <= start + 1e-4 {
                lo = lo.max(oe - start);
            } else if os >= end - 1e-4 {
                hi = hi.min(os - end);
            }
        }
    }
    (lo, hi)
}

#[cfg(test)]
mod tests {
    use super::dt_bounds;

    /// The bounds are the same whatever the cursor has done since the
    /// press: they come from where the clips started, so a move left
    /// reaches all the way to zero — it doesn't halve every step.
    #[test]
    fn the_bounds_come_from_the_original_starts() {
        // One clip, started at 10, nothing else on its track.
        let (lo, hi) = dt_bounds(&[(10.0, 14.0)], &[Vec::new()]);
        assert_eq!(lo, -10.0, "all the way to zero");
        assert_eq!(hi, f32::MAX, "no end to the right");
        // The same call, asked again mid-drag, answers the same.
        assert_eq!(dt_bounds(&[(10.0, 14.0)], &[Vec::new()]), (lo, hi));
        // A stayer before and after: the gap on either side.
        let (lo, hi) = dt_bounds(&[(10.0, 14.0)], &[vec![(2.0, 6.0), (20.0, 25.0)]]);
        assert!((lo - -4.0).abs() < 1e-6);
        assert!((hi - 6.0).abs() < 1e-6);
        // Two members: the tightest of both, and a member at zero pins lo.
        let (lo, hi) = dt_bounds(
            &[(0.0, 4.0), (10.0, 14.0)],
            &[Vec::new(), vec![(16.0, 20.0)]],
        );
        assert_eq!(lo, 0.0);
        assert!((hi - 2.0).abs() < 1e-6);
    }
}
