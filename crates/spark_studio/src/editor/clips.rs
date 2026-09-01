//! The arrangement, editor side: object clips (when each object exists
//! and how it moves) and comp clips (the comps this comp places). Every
//! operation undoable. Evaluation lives in `keys` (objects) and
//! `comps.rs` (placed comps).

use crate::doc::{Clip, CompAsset, ObjClip};
use crate::history::Tag;

use super::Editor;

impl Editor {
    // ---- object clips ---------------------------------------------------

    /// Object `i`'s clips, sorted by start.
    pub fn obj_clips(&self, i: usize) -> &[ObjClip] {
        self.clips.get(i).map(Vec::as_slice).unwrap_or(&[])
    }

    /// Move or trim object `i`'s clip `c` — one undo step per drag.
    /// The span is clamped against the clip's neighbours rather than
    /// merged into them: an object can't overlap itself, so a drag past a
    /// neighbour stops at its edge (Ableton would eat the neighbour;
    /// refusing is the honest v1). A left-trim eats content — `offset`
    /// grows by what was cut, so the surviving motion keeps its place.
    ///
    /// A clip whose loop *is* its whole length keeps it that way when
    /// its right edge moves: stretch a newborn bar to eight and you get
    /// eight bars to key, not one bar eight times (Alva's first clip
    /// view, 2026-08-31 — every key stamped past bar one wrapped into
    /// it). Shorten the loop brace in the clip view and the clip becomes
    /// a repeater: from then on the edge trims how many times it plays.
    pub fn set_obj_clip_span(&mut self, i: usize, c: usize, start: f32, len: f32) -> bool {
        let Some(old) = self.clips.get(i).and_then(|l| l.get(c)) else {
            return false;
        };
        let old = old.clone();
        // Room between the neighbours.
        let lo = c
            .checked_sub(1)
            .and_then(|p| self.clips[i].get(p))
            .map(|p| p.end())
            .unwrap_or(0.0);
        let hi = self.clips[i].get(c + 1).map(|n| n.start).unwrap_or(f32::MAX);
        let start = start.clamp(lo, (hi - 0.05).max(lo));
        let len = len.clamp(0.05, hi - start);
        let mut next = old.clone();
        // A moved body carries its content; a trimmed left edge eats it.
        let left_trim = (start - old.start).abs() > 1e-6 && (start + len - old.end()).abs() < 1e-6;
        if left_trim {
            next.offset = (old.offset + (start - old.start)).max(0.0);
        } else if (old.loop_len - old.len).abs() < 1e-4 {
            next.loop_len = len.max(0.05);
        }
        next.start = start;
        next.len = len;
        if next == old {
            return false;
        }
        let s = self.snap();
        self.history.change(Tag::Clip, s);
        self.clips[i][c] = next;
        self.clear_posed();
        true
    }

    /// Set how much of object `i`'s clip `c` repeats — the loop brace on
    /// the clip view's ruler. One undo step per drag.
    pub fn set_obj_clip_loop_len(&mut self, i: usize, c: usize, len: f32) -> bool {
        let len = len.max(0.05);
        let Some(clip) = self.clips.get(i).and_then(|l| l.get(c)) else {
            return false;
        };
        if (clip.loop_len - len).abs() < 1e-6 {
            return false;
        }
        let s = self.snap();
        self.history.change(Tag::Clip, s);
        self.clips[i][c].loop_len = len;
        self.clear_posed();
        true
    }

    /// Toggle whether object `i`'s clip `c` loops. Undoable.
    pub fn toggle_obj_clip_loop(&mut self, i: usize, c: usize) -> bool {
        if self.clips.get(i).and_then(|l| l.get(c)).is_none() {
            return false;
        }
        let s = self.snap();
        self.history.push(s);
        let clip = &mut self.clips[i][c];
        clip.loop_on = !clip.loop_on;
        println!("clip loop {}", if clip.loop_on { "on" } else { "off" });
        self.clear_posed();
        true
    }

    /// Delete object `i`'s clip `c`. The object stays — a track with no
    /// clips is an instrument with nothing scheduled. Undoable.
    pub fn delete_obj_clip(&mut self, i: usize, c: usize) -> bool {
        if self.clips.get(i).and_then(|l| l.get(c)).is_none() {
            return false;
        }
        let s = self.snap();
        self.history.push(s);
        self.clips[i].remove(c);
        self.clear_posed();
        println!("clip removed");
        true
    }

    /// Duplicate object `i`'s clip `c` immediately after itself (its own
    /// length later, clamped into the gap before the next clip). Undoable;
    /// returns the new clip's index.
    pub fn duplicate_obj_clip(&mut self, i: usize, c: usize) -> Option<usize> {
        let clip = self.clips.get(i)?.get(c)?.clone();
        let start = clip.end();
        let room = self
            .clips[i]
            .get(c + 1)
            .map(|n| n.start - start)
            .unwrap_or(f32::MAX);
        if room < 0.05 {
            println!("no room after the clip");
            return None;
        }
        let s = self.snap();
        self.history.push(s);
        let mut dup = clip;
        dup.start = start;
        dup.len = dup.len.min(room);
        self.clips[i].insert(c + 1, dup);
        self.clear_posed();
        Some(c + 1)
    }

    // ---- comp clips -----------------------------------------------------

    pub fn comp_clips(&self) -> &[Clip] {
        &self.comp_clips
    }

    pub fn comp_assets(&self) -> &[CompAsset] {
        &self.comp_assets
    }

    pub fn comp_asset(&self, id: u32) -> Option<&CompAsset> {
        self.comp_assets.iter().find(|a| a.id == id)
    }

    /// Register a comp file with the arrangement. The same path twice is
    /// one asset, like mesh assets.
    pub fn add_comp_asset(&mut self, path: String) -> u32 {
        if let Some(a) = self.comp_assets.iter().find(|a| a.path == path) {
            return a.id;
        }
        let id = self.comp_assets.iter().map(|a| a.id).max().unwrap_or(0) + 1;
        self.comp_assets.push(CompAsset { id, path });
        id
    }

    /// Place a comp clip. Undoable; returns its index.
    pub fn place_clip(&mut self, comp: u32, track: u32, start: f32, len: f32) -> usize {
        let s = self.snap();
        self.history.push(s);
        self.comp_clips.push(Clip {
            track,
            comp,
            start: start.max(0.0),
            len: len.max(0.05),
        });
        self.comp_clips.len() - 1
    }

    /// Move or trim comp clip `i` — one undo step per drag.
    pub fn set_clip_span(&mut self, i: usize, track: u32, start: f32, len: f32) -> bool {
        let Some(&old) = self.comp_clips.get(i) else {
            return false;
        };
        let next = Clip {
            track,
            comp: old.comp,
            start: start.max(0.0),
            len: len.max(0.05),
        };
        if next == old {
            return false;
        }
        let s = self.snap();
        self.history.change(Tag::Clip, s);
        self.comp_clips[i] = next;
        true
    }

    /// Delete comp clip `i`. The comp asset stays — other clips may play
    /// it, and an unused asset line in the file is harmless. Undoable.
    pub fn delete_clip(&mut self, i: usize) -> bool {
        if i >= self.comp_clips.len() {
            return false;
        }
        let s = self.snap();
        self.history.push(s);
        self.comp_clips.remove(i);
        println!("clip removed");
        true
    }

    /// The first comp track where `[start, start+len)` lands on empty air,
    /// or a fresh one under everything — where File > Place Comp drops.
    pub fn free_track(&self, start: f32, len: f32) -> u32 {
        let overlaps = |t: u32| {
            self.comp_clips
                .iter()
                .any(|c| c.track == t && c.start < start + len && start < c.start + c.len)
        };
        let mut t = 0;
        while overlaps(t) {
            t += 1;
        }
        t
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Place, drag, delete — each one undoable, and a same-drag move
    /// coalesces into one step.
    #[test]
    fn comp_clips_place_move_and_undo() {
        let mut e = Editor::empty();
        let id = e.add_comp_asset("/x/spin.spark".into());
        assert_eq!(e.add_comp_asset("/x/spin.spark".into()), id, "deduped");
        let i = e.place_clip(id, 0, 4.0, 8.0);
        assert_eq!(e.comp_clips()[i].start, 4.0);
        assert!(e.set_clip_span(i, 0, 5.0, 8.0));
        assert!(e.set_clip_span(i, 1, 6.0, 8.0));
        assert!(!e.set_clip_span(i, 1, 6.0, 8.0), "no-op says so");
        e.end_gesture();
        assert_eq!(e.comp_clips()[i].track, 1);
        e.undo();
        assert_eq!(e.comp_clips()[i].start, 4.0, "the whole drag undid at once");
        e.undo();
        assert!(e.comp_clips().is_empty(), "the placement undid");
    }

    /// Placement avoids landing on another comp clip.
    #[test]
    fn place_finds_the_first_free_track() {
        let mut e = Editor::empty();
        let id = e.add_comp_asset("/x/a.spark".into());
        e.place_clip(id, 0, 0.0, 10.0);
        assert_eq!(e.free_track(4.0, 4.0), 1, "track 0 is busy there");
        assert_eq!(e.free_track(12.0, 4.0), 0, "track 0 is free later");
    }

    /// An object is born with a one-bar clip at the playhead; moving and
    /// trimming clamp against its neighbours — an object can't overlap
    /// itself.
    #[test]
    fn object_clips_are_born_and_never_overlap() {
        let mut e = Editor::empty();
        e.set_time(4.0);
        e.set_cursor_canvas([300.0, 300.0]);
        e.choose_tool(crate::props::Tool::Circle);
        e.mouse_down(false);
        e.set_cursor_canvas([400.0, 300.0]);
        e.mouse_up();
        let clips = e.obj_clips(0);
        assert_eq!(clips.len(), 1);
        assert_eq!(clips[0].start, 4.0, "born at the playhead");
        assert_eq!(clips[0].len, e.bar_s, "one bar long");
        assert!(clips[0].loop_on, "looping its own length");
        // A duplicate lands flush after; the two can't be dragged into
        // each other.
        assert_eq!(e.duplicate_obj_clip(0, 0), Some(1));
        let end0 = e.obj_clips(0)[0].end();
        assert_eq!(e.obj_clips(0)[1].start, end0);
        assert!(
            !e.set_obj_clip_span(0, 1, end0 - 1.0, e.bar_s),
            "a fully-blocked drag is a no-op"
        );
        assert_eq!(
            e.obj_clips(0)[1].start,
            end0,
            "the drag stopped at the neighbour's edge"
        );
        // A left trim eats content: offset grows by the cut.
        assert!(e.set_obj_clip_span(0, 0, 4.5, e.bar_s - 0.5));
        let c0 = &e.obj_clips(0)[0];
        assert!((c0.offset - 0.5).abs() < 1e-4, "offset {}", c0.offset);
        assert!((c0.end() - end0).abs() < 1e-4, "right edge stayed put");
    }

    /// A newborn clip loops its whole self, and stays whole-clip when its
    /// right edge is dragged; once the loop is shorter than the clip, the
    /// edge only changes how many times it repeats.
    #[test]
    fn the_whole_clip_loop_follows_the_right_edge() {
        let mut e = Editor::empty();
        e.set_time(0.0);
        e.set_cursor_canvas([300.0, 300.0]);
        e.choose_tool(crate::props::Tool::Circle);
        e.mouse_down(false);
        e.set_cursor_canvas([400.0, 300.0]);
        e.mouse_up();
        let bar = e.bar_s;
        assert_eq!(e.obj_clips(0)[0].loop_len, bar);
        // Each drag is its own gesture — the release between them is
        // what keeps consecutive clip edits from coalescing.
        assert!(e.set_obj_clip_span(0, 0, 0.0, bar * 8.0));
        e.end_gesture();
        assert!(
            (e.obj_clips(0)[0].loop_len - bar * 8.0).abs() < 1e-4,
            "eight bars to key"
        );
        // A moved body changes nothing about the loop.
        assert!(e.set_obj_clip_span(0, 0, bar, bar * 8.0));
        e.end_gesture();
        assert!((e.obj_clips(0)[0].loop_len - bar * 8.0).abs() < 1e-4);
        // Shorten the brace: the clip is a repeater now.
        assert!(e.set_obj_clip_loop_len(0, 0, bar));
        e.end_gesture();
        assert!(e.set_obj_clip_span(0, 0, bar, bar * 12.0));
        e.end_gesture();
        assert_eq!(
            e.obj_clips(0)[0].loop_len,
            bar,
            "the edge only adds repeats"
        );
        // A left trim never touches it either.
        assert!(e.set_obj_clip_span(0, 0, bar * 2.0, bar * 11.0));
        e.end_gesture();
        assert_eq!(e.obj_clips(0)[0].loop_len, bar);
        e.undo();
        assert_eq!(e.obj_clips(0)[0].start, bar, "the trim undid");
        e.undo();
        assert!((e.obj_clips(0)[0].len - bar * 8.0).abs() < 1e-4, "the stretch undid");
    }

    /// Deleting the last clip keeps the object — a track with nothing
    /// scheduled — and the object stops existing at the playhead.
    #[test]
    fn an_object_outlives_its_clips() {
        let mut e = Editor::empty();
        e.set_time(0.0);
        e.set_cursor_canvas([300.0, 300.0]);
        e.choose_tool(crate::props::Tool::Circle);
        e.mouse_down(false);
        e.set_cursor_canvas([400.0, 300.0]);
        e.mouse_up();
        assert!(e.exists_now(0));
        assert!(e.delete_obj_clip(0, 0));
        assert_eq!(e.shapes().len(), 1, "the object stays");
        assert!(!e.exists_now(0), "but it isn't there now");
        e.undo();
        assert!(e.exists_now(0), "undo brings the clip back");
    }
}
