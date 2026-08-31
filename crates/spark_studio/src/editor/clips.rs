//! The arrangement, editor side: the comps this comp places and the
//! clips that play them. A clip is (track, comp, start, len) — its comp
//! loops inside it, so the clip's length is *how long it plays*, not how
//! long the comp is. Evaluation lives in `comps.rs`; this file is the
//! document operations, every one undoable.

use crate::doc::{Clip, CompAsset};
use crate::history::Tag;

use super::Editor;

impl Editor {
    pub fn clips(&self) -> &[Clip] {
        &self.clips
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

    /// Place a clip. Undoable; returns its index.
    pub fn place_clip(&mut self, comp: u32, track: u32, start: f32, len: f32) -> usize {
        let s = self.snap();
        self.history.push(s);
        self.clips.push(Clip {
            track,
            comp,
            start: start.max(0.0),
            len: len.max(0.05),
        });
        self.clips.len() - 1
    }

    /// Move or trim clip `i` — one undo step per drag (see [`Tag::Clip`]).
    pub fn set_clip_span(&mut self, i: usize, track: u32, start: f32, len: f32) -> bool {
        let Some(&old) = self.clips.get(i) else {
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
        self.clips[i] = next;
        true
    }

    /// Delete clip `i`. The comp asset stays — other clips may play it,
    /// and an unused asset line in the file is harmless. Undoable.
    pub fn delete_clip(&mut self, i: usize) -> bool {
        if i >= self.clips.len() {
            return false;
        }
        let s = self.snap();
        self.history.push(s);
        self.clips.remove(i);
        println!("clip removed");
        true
    }

    /// The first track where `[start, start+len)` lands on empty air, or
    /// a fresh one under everything — where File > Place Comp drops.
    pub fn free_track(&self, start: f32, len: f32) -> u32 {
        let overlaps = |t: u32| {
            self.clips
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
    fn clips_place_move_and_undo() {
        let mut e = Editor::empty();
        let id = e.add_comp_asset("/x/spin.spark".into());
        assert_eq!(e.add_comp_asset("/x/spin.spark".into()), id, "deduped");
        let i = e.place_clip(id, 0, 4.0, 8.0);
        assert_eq!(e.clips()[i].start, 4.0);
        // A drag is many set_clip_span calls and one undo step.
        assert!(e.set_clip_span(i, 0, 5.0, 8.0));
        assert!(e.set_clip_span(i, 1, 6.0, 8.0));
        assert!(!e.set_clip_span(i, 1, 6.0, 8.0), "no-op says so");
        e.end_gesture();
        assert_eq!(e.clips()[i].track, 1);
        e.undo();
        assert_eq!(e.clips()[i].start, 4.0, "the whole drag undid at once");
        e.undo();
        assert!(e.clips().is_empty(), "the placement undid");
    }

    /// Placement avoids landing on another clip.
    #[test]
    fn place_finds_the_first_free_track() {
        let mut e = Editor::empty();
        let id = e.add_comp_asset("/x/a.spark".into());
        e.place_clip(id, 0, 0.0, 10.0);
        assert_eq!(e.free_track(4.0, 4.0), 1, "track 0 is busy there");
        assert_eq!(e.free_track(12.0, 4.0), 0, "track 0 is free later");
    }
}
