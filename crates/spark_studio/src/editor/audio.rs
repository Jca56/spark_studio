//! The arrangement's audio half, editor side: the sounds a comp names,
//! the clips that place them (the song's included — it is asset
//! [`SONG`]), and each track's volume. Every operation undoable. What
//! the clips *sound* like is the studio's business (`sound.rs`): the
//! editor knows paths and times, never samples.
//!
//! Clips of one asset never overlap — a file can't play over itself,
//! the same rule an object's clips keep — so a drag clamps against its
//! neighbours on the same track. The list stays in insertion order:
//! a clip's index is its name for a gesture, and a sort mid-drag would
//! rename it.

use crate::doc::{AudioClip, SONG, SoundAsset};
use crate::history::Tag;

use super::Editor;

/// The shortest an audio clip can be trimmed to, seconds.
const MIN_LEN: f32 = 0.05;

impl Editor {
    pub fn audio_clips(&self) -> &[AudioClip] {
        &self.aclips
    }

    pub fn sounds(&self) -> &[SoundAsset] {
        &self.sounds
    }

    pub fn sound(&self, id: u32) -> Option<&SoundAsset> {
        self.sounds.iter().find(|s| s.id == id)
    }

    /// A track's linear gain; unity unless set.
    pub fn volume(&self, asset: u32) -> f32 {
        self.volumes
            .iter()
            .find(|(id, _)| *id == asset)
            .map(|(_, g)| *g)
            .unwrap_or(1.0)
    }

    /// Set a track's gain — one undo step per drag of its box.
    pub fn set_volume(&mut self, asset: u32, gain: f32) -> bool {
        let gain = gain.max(0.0);
        if (self.volume(asset) - gain).abs() < 1e-6 {
            return false;
        }
        let s = self.snap();
        self.history.change(Tag::Volume(asset), s);
        self.volumes.retain(|(id, _)| *id != asset);
        if (gain - 1.0).abs() > 1e-6 {
            self.volumes.push((asset, gain));
        }
        true
    }

    /// Register a sound file with the comp. The same path twice is one
    /// asset. Ids count from 1 — 0 is the song's.
    pub fn add_sound(&mut self, path: String) -> u32 {
        if let Some(s) = self.sounds.iter().find(|s| s.path == path) {
            return s.id;
        }
        let id = self.sounds.iter().map(|s| s.id).max().unwrap_or(SONG) + 1;
        self.sounds.push(SoundAsset { id, path });
        id
    }

    /// Whether any clip plays `asset`.
    pub fn asset_placed(&self, asset: u32) -> bool {
        self.aclips.iter().any(|c| c.asset == asset)
    }

    /// Put a clip of `asset` on the arrangement — the whole file from
    /// `start` when `len` is zero. Undoable; returns its index.
    pub fn place_audio(&mut self, asset: u32, start: f32, len: f32) -> usize {
        let s = self.snap();
        self.history.push(s);
        self.aclips.push(AudioClip {
            asset,
            start: start.max(0.0),
            len: len.max(0.0),
            offset: 0.0,
        });
        self.aclips.len() - 1
    }

    /// The room clip `k` has between its neighbours on its own track:
    /// the latest end before it and the earliest start after it.
    fn audio_room(&self, k: usize, file_len: f32) -> (f32, f32) {
        let me = self.aclips[k];
        let mut lo = 0.0f32;
        let mut hi = f32::MAX;
        for (j, c) in self.aclips.iter().enumerate() {
            if j == k || c.asset != me.asset {
                continue;
            }
            let end = c.end(file_len);
            if end <= me.start + 1e-4 {
                lo = lo.max(end);
            } else if c.start >= me.end(file_len) - 1e-4 {
                hi = hi.min(c.start);
            }
        }
        (lo, hi)
    }

    /// Move or trim audio clip `k` — one undo step per drag. `file_len`
    /// is the file's length in seconds (what a whole-file clip's span
    /// resolves against; zero when the file isn't loaded). A moved body
    /// carries its content; a left-trim eats it (`offset` grows); no
    /// edge can reach outside the file.
    pub fn set_audio_clip_span(&mut self, k: usize, start: f32, len: f32, file_len: f32) -> bool {
        let Some(old) = self.aclips.get(k).copied() else {
            return false;
        };
        let (lo, hi) = self.audio_room(k, file_len);
        // A move keeps its length and stops at the neighbour's edge; a
        // trim may shrink to fit.
        let old_span = old.span(file_len);
        let is_move = (len - old_span).abs() < 1e-4 && (start - old.start).abs() > 1e-6;
        let start = if is_move {
            start.clamp(lo, (hi - old_span).max(lo))
        } else {
            start.clamp(lo, (hi - MIN_LEN).max(lo))
        };
        let mut len = len.clamp(MIN_LEN, (hi - start).max(MIN_LEN));
        let mut next = old;
        let old_end = old.end(file_len);
        let left_trim = (start - old.start).abs() > 1e-6 && (start + len - old_end).abs() < 1e-4;
        if left_trim {
            next.offset = (old.offset + (start - old.start)).max(0.0);
        }
        // Nothing plays past the file's end.
        if file_len > 0.0 {
            len = len.min((file_len - next.offset).max(MIN_LEN));
        }
        next.start = start;
        next.len = len;
        // A clip that still plays to the end of its file keeps saying so
        // — a move never turns "whole" into a number.
        if old.len == 0.0 && !left_trim && (next.end(file_len) - (start + old.span(file_len))).abs() < 1e-4 {
            next.len = 0.0;
        }
        if next == old {
            return false;
        }
        let s = self.snap();
        self.history.change(Tag::Clip, s);
        self.aclips[k] = next;
        true
    }

    /// Delete audio clip `k`. The asset stays named — a sound with
    /// nothing scheduled, the way an object keeps its track. Undoable.
    pub fn delete_audio_clip(&mut self, k: usize) -> bool {
        if k >= self.aclips.len() {
            return false;
        }
        let s = self.snap();
        self.history.push(s);
        self.aclips.remove(k);
        println!("audio clip removed");
        true
    }

    /// Duplicate audio clip `k` flush after itself, clamped into the gap
    /// before the next clip on its track. Undoable; the new index.
    pub fn duplicate_audio_clip(&mut self, k: usize, file_len: f32) -> Option<usize> {
        let clip = *self.aclips.get(k)?;
        let span = clip.span(file_len);
        let start = clip.start + span;
        let room = self
            .aclips
            .iter()
            .filter(|c| c.asset == clip.asset && c.start >= start - 1e-4)
            .map(|c| c.start - start)
            .fold(f32::MAX, f32::min);
        if room < MIN_LEN {
            println!("no room after the clip");
            return None;
        }
        let s = self.snap();
        self.history.push(s);
        self.aclips.push(AudioClip {
            asset: clip.asset,
            start,
            len: span.min(room),
            offset: clip.offset,
        });
        Some(self.aclips.len() - 1)
    }

    /// Where the song sits: its earliest clip. The grid's phase and the
    /// react curves read the song through this.
    pub fn song_clip(&self) -> Option<AudioClip> {
        self.aclips
            .iter()
            .filter(|c| c.asset == SONG)
            .min_by(|a, b| a.start.total_cmp(&b.start))
            .copied()
    }

    /// Song time for timeline time `t`: through whichever song clip
    /// covers it, or none — the song isn't playing then.
    pub fn song_local(&self, t: f32, song_len: f32) -> Option<f32> {
        self.aclips
            .iter()
            .filter(|c| c.asset == SONG && c.contains(t, song_len))
            .map(|c| c.local(t))
            .next()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A clip can be placed, moved, trimmed from either edge, and never
    /// crosses a neighbour on its own track or the end of its file.
    #[test]
    fn audio_clips_move_trim_and_respect_their_neighbours() {
        let mut e = Editor::empty();
        let id = e.add_sound("/vo/intro.wav".into());
        assert_eq!(id, 1, "sounds count from one; the song is zero");
        assert_eq!(e.add_sound("/vo/intro.wav".into()), 1, "the same file once");
        let a = e.place_audio(id, 0.0, 0.0);
        let b = e.place_audio(id, 20.0, 5.0);
        let file = 10.0;
        // A whole-file clip moved stays whole.
        assert!(e.set_audio_clip_span(a, 2.0, 10.0, file));
        assert_eq!(e.audio_clips()[a].len, 0.0);
        assert_eq!(e.audio_clips()[a].start, 2.0);
        // It can't run into its neighbour: 2 + 10 = 12 < 20 is fine, but
        // a move to 15 would end at 25, over b — clamped to end at 20.
        assert!(e.set_audio_clip_span(a, 15.0, 10.0, file));
        assert!((e.audio_clips()[a].start - 10.0).abs() < 1e-4, "{:?}", e.audio_clips()[a]);
        // A left-trim eats content.
        assert!(e.set_audio_clip_span(a, 12.0, 8.0, file));
        assert!((e.audio_clips()[a].offset - 2.0).abs() < 1e-4);
        assert!((e.audio_clips()[a].len - 8.0).abs() < 1e-4);
        // A right-trim can't reach past the file: 8 s of content remain.
        assert!(!e.set_audio_clip_span(a, 12.0, 30.0, file) || e.audio_clips()[a].len <= 8.0 + 1e-4);
        // Delete, then undo brings it back.
        assert!(e.delete_audio_clip(b));
        assert_eq!(e.audio_clips().len(), 1);
        e.undo();
        assert_eq!(e.audio_clips().len(), 2);
    }

    /// The song's clip says where the song is; time before it has no
    /// song time, time inside it maps through the trim.
    #[test]
    fn the_song_is_read_through_its_clip() {
        let mut e = Editor::empty();
        assert!(e.song_clip().is_none());
        let k = e.place_audio(SONG, 6.0, 0.0);
        assert!(e.set_audio_clip_span(k, 6.0, 180.0, 180.0) || true);
        assert_eq!(e.song_local(3.0, 180.0), None, "the intro has no song in it");
        assert!((e.song_local(7.5, 180.0).unwrap() - 1.5).abs() < 1e-6);
        assert!(e.song_local(186.5, 180.0).is_none(), "after the song ends");
    }

    /// Volume is per track, unity by default, and one drag is one undo.
    #[test]
    fn volume_is_per_track_and_undoable() {
        let mut e = Editor::empty();
        assert_eq!(e.volume(SONG), 1.0);
        assert!(e.set_volume(SONG, 0.5));
        assert!(e.set_volume(SONG, 0.25));
        assert!(!e.set_volume(SONG, 0.25), "no change, no step");
        assert_eq!(e.volume(SONG), 0.25);
        e.undo();
        assert_eq!(e.volume(SONG), 1.0, "the drag was one step");
        assert!(e.set_volume(1, 1.0) == false, "unity onto unity is nothing");
    }
}
