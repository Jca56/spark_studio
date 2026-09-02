//! Audio on the arrangement, studio side: the files behind the clips,
//! what the mixer hears, where the song sits, and the volume boxes.
//!
//! The editor knows paths and times (`editor/audio.rs`); this is where
//! they meet samples. The song is decoded and analyzed into `audio`
//! (its `Track`); every other sound is decoded into a [`Slot`]. From
//! the clips and the loaded files the studio builds the **voices** the
//! player mixes — rebuilt whenever their numbers change, every frame
//! checked, so a drag, an undo or a load all reach the ear without a
//! call site remembering to say so.
//!
//! **The song sits where its clip says.** Timeline time is the master
//! clock; song time is read *through* the song's clip — the react
//! curves map that way, so an intro before the song is silence with
//! the beat grid ticking and every reaction at rest. The grid itself
//! never moves with the song (`Studio::grid`). While a placed comp is being edited the
//! audio is the *project's* (the parked editor at the bottom of the
//! breadcrumb), the way a clip in a Live set is edited against the
//! set's own arrangement.

use std::path::PathBuf;

use spark_audio::{SAMPLE_RATE, Sound, Voice};

use crate::arrange::{AudioBar, AudioTrack, ClipRef};
use crate::doc::{AudioClip, SONG};
use crate::editor::Editor;
use crate::{AppEvent, Studio};

/// What the player was last handed, by the numbers that define it:
/// each clip's (asset, start, offset, length, gain bits), and how many
/// of them had a file to play.
pub(crate) type VoicesKey = (Vec<(u32, usize, usize, usize, u32)>, usize);

/// A sound the comp names, as the studio has it.
pub(crate) enum Slot {
    Loading,
    Ready(Sound),
    /// The file couldn't be read or decoded; its clips stay on the
    /// arrangement in red, like a comp whose file is gone.
    Missing,
}

/// A volume box being dragged: which track, where the press was, and
/// what the box read then.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct VolDrag {
    pub asset: u32,
    pub start_y: f32,
    pub start_db: f32,
}

/// The box's reach: silence to a modest boost. A quarter of a decibel
/// per logical pixel, Shift for a tenth of that.
pub(crate) const DB_MIN: f32 = -60.0;
pub(crate) const DB_MAX: f32 = 12.0;
const DB_PER_PX: f32 = 0.25;

pub(crate) fn db_of(gain: f32) -> f32 {
    if gain <= 0.001 {
        DB_MIN
    } else {
        (20.0 * gain.log10()).clamp(DB_MIN, DB_MAX)
    }
}

pub(crate) fn gain_of(db: f32) -> f32 {
    if db <= DB_MIN + 1e-3 {
        0.0
    } else {
        10f32.powf(db.min(DB_MAX) / 20.0)
    }
}

/// What the box reads for a gain: `+0.0 dB`, `-6.5 dB`, or `-inf dB`.
pub(crate) fn volume_text(gain: f32) -> String {
    let db = db_of(gain);
    if db <= DB_MIN + 1e-3 {
        "-inf dB".to_string()
    } else {
        format!("{db:+.1} dB")
    }
}

/// Where a volume drag has landed, in dB: `dy` is physical px down
/// from the press (up raises), `fine` a tenth.
pub(crate) fn dragged_db(start_db: f32, dy: f32, scale: f32, fine: bool) -> f32 {
    let k = if fine { 0.1 } else { 1.0 };
    (start_db - dy / scale * DB_PER_PX * k).clamp(DB_MIN, DB_MAX)
}

impl Studio {
    /// The editor whose audio plays: the project's — parked at the
    /// bottom of the breadcrumb while a placed comp is being edited.
    pub(crate) fn audio_editor(&self) -> &Editor {
        self.comp_stack.first().map(|c| &c.editor).unwrap_or(&self.editor)
    }

    /// Whether a placed comp is being edited — its audio is read-only.
    pub(crate) fn in_comp(&self) -> bool {
        !self.comp_stack.is_empty()
    }

    /// The length of a loaded audio file, seconds; zero while it isn't.
    pub(crate) fn file_len(&self, asset: u32) -> f32 {
        if asset == SONG {
            return self.audio.as_ref().map(|t| t.duration).unwrap_or(0.0);
        }
        match self.sounds.get(&asset) {
            Some(Slot::Ready(s)) => s.duration,
            _ => 0.0,
        }
    }

    /// How long a clip plays: through its file's length, or — for a
    /// file not loaded yet — its own length, else a bar so it can be
    /// seen and grabbed.
    pub(crate) fn clip_span(&self, c: &AudioClip) -> f32 {
        let fl = self.file_len(c.asset);
        if fl > 0.0 {
            c.span(fl)
        } else if c.len > 0.0 {
            c.len
        } else {
            self.editor.bar_s
        }
    }

    /// Song time for timeline time `t`, or none where the song isn't
    /// playing — the intro, a gap, after the end.
    pub(crate) fn song_local(&self, t: f32) -> Option<f32> {
        let len = self.audio.as_ref()?.duration;
        self.audio_editor().song_local(t, len)
    }

    /// The react curves at timeline time `t`: the song's, through its
    /// clip; nothing where there is no song to react to.
    pub(crate) fn levels_at(&self, t: f32) -> Option<crate::fx::Levels> {
        let track = self.audio.as_ref()?;
        let lt = self.song_local(t)?;
        Some(crate::fx::Levels::at(track, lt))
    }

    /// What an audio asset is called on its row: the file's name.
    pub(crate) fn asset_name(&self, asset: u32) -> String {
        let ed = self.audio_editor();
        let path = if asset == SONG {
            ed.audio_path().map(str::to_string)
        } else {
            ed.sound(asset).map(|s| s.path.clone())
        };
        path.map(|p| {
            std::path::Path::new(&p)
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or(p)
        })
        .unwrap_or_else(|| "sound".to_string())
    }

    /// The audio tracks as the arrangement lists them: the song first
    /// (when the comp names one), then each sound, with their clips
    /// resolved to spans.
    pub(crate) fn audio_tracks(&self) -> Vec<AudioTrack> {
        let ed = self.audio_editor();
        let mut ids: Vec<u32> = Vec::new();
        if ed.audio_path().is_some() {
            ids.push(SONG);
        }
        ids.extend(ed.sounds().iter().map(|s| s.id));
        ids.iter()
            .map(|&asset| AudioTrack {
                asset,
                name: self.asset_name(asset),
                missing: matches!(self.sounds.get(&asset), Some(Slot::Missing)),
                volume: volume_text(ed.volume(asset)),
                clips: ed
                    .audio_clips()
                    .iter()
                    .enumerate()
                    .filter(|(_, c)| c.asset == asset)
                    .map(|(k, c)| AudioBar {
                        k,
                        start: c.start,
                        span: self.clip_span(c),
                    })
                    .collect(),
            })
            .collect()
    }

    /// The clips as the mixer hears them — every one whose file is
    /// loaded, at its place, trim, length and track volume.
    pub(crate) fn voices(&self) -> Vec<Voice> {
        let ed = self.audio_editor();
        let frames = |s: f32| (s.max(0.0) * SAMPLE_RATE as f32).round() as usize;
        ed.audio_clips()
            .iter()
            .filter_map(|c| {
                let samples = if c.asset == SONG {
                    self.audio.as_ref()?.samples.clone()
                } else {
                    match self.sounds.get(&c.asset)? {
                        Slot::Ready(s) => s.samples.clone(),
                        _ => return None,
                    }
                };
                Some(Voice {
                    samples,
                    at: frames(c.start),
                    offset: frames(c.offset),
                    len: frames(self.clip_span(c)),
                    gain: ed.volume(c.asset),
                })
            })
            .collect()
    }

    /// Hand the player the voices when they've changed — called every
    /// frame, cheap when nothing has. The device opens on the first
    /// voice; if it can't, the comp still scrubs and keys on its own
    /// clock, and the failure is said once.
    pub(crate) fn sync_voices(&mut self) {
        self.sync_sounds();
        let voices = self.voices();
        let key: Vec<(u32, usize, usize, usize, u32)> = self
            .audio_editor()
            .audio_clips()
            .iter()
            .zip(voices.iter().map(|v| (v.at, v.offset, v.len, v.gain.to_bits())))
            .map(|(c, (at, off, len, g))| (c.asset, at, off, len, g))
            .collect();
        // Voices skip unloaded clips, so the zip above can pair wrongly
        // once one is missing; the count of loaded files joins the key.
        let key_full: VoicesKey = (key, voices.len());
        if self.voices_key.as_ref() == Some(&key_full) {
            return;
        }
        if self.player.is_none() && !voices.is_empty() && !self.player_failed {
            match spark_audio::Player::new() {
                Ok(p) => {
                    // The device is the clock from here: pick up where the
                    // silent clock was, running if it was running.
                    p.seek(self.editor.time());
                    if self.silent_play.take().is_some() {
                        p.toggle();
                    }
                    self.player = Some(p);
                    self.apply_loop();
                }
                Err(e) => {
                    self.player_failed = true;
                    println!("playback unavailable ({e}) — scrubbing and keyframing still work");
                }
            }
        }
        if let Some(p) = &self.player {
            p.set_voices(voices);
        }
        self.voices_key = Some(key_full);
    }

    /// Where the arrangement's content ends: the last clip of any kind
    /// — audio, object, comp. Zero on an empty comp.
    pub(crate) fn content_end(&self) -> f32 {
        let ed = &self.editor;
        let mut end = 0.0f32;
        for c in self.audio_editor().audio_clips() {
            end = end.max(c.start + self.clip_span(c));
        }
        for i in 0..ed.shapes().len() {
            for c in ed.obj_clips(i) {
                end = end.max(c.end());
            }
        }
        for c in ed.comp_clips() {
            end = end.max(c.start + c.len);
        }
        end
    }

    /// What File > Export renders: the loop while it's on, else the
    /// whole arrangement from the top to the bar after its last clip.
    pub(crate) fn export_range(&self) -> Option<(f32, f32)> {
        if let (Some((a, b)), true) = (self.loop_region, self.loop_on)
            && b > a
        {
            return Some((a, b));
        }
        let end = self.content_end();
        if end <= 0.0 {
            return None;
        }
        let beat = self.grid();
        let bar_s = 4.0 * 60.0 / beat.bpm.max(1.0);
        // Up to the next bar line, so a clip ending mid-bar isn't cut.
        let k = ((end - beat.first_bar) / bar_s).ceil().max(0.0);
        let to = beat.first_bar + k * bar_s;
        Some((0.0, if to > end - 1e-3 { to } else { to + bar_s }))
    }

    /// File > Import Sound…: name the file, drop its whole length at
    /// the playhead on its own row, and decode it off-thread.
    pub(crate) fn import_sound(&mut self, path: PathBuf) {
        if self.in_comp() {
            let note = "Audio belongs to the project — go Back first".to_string();
            println!("{note}");
            self.export_note = Some(note);
            return;
        }
        let p = path.to_string_lossy().into_owned();
        let id = self.editor.add_sound(p.clone());
        let k = self.editor.place_audio(id, self.editor.time(), 0.0);
        self.selected_clips = vec![ClipRef::Audio(k)];
        if !matches!(self.sounds.get(&id), Some(Slot::Ready(_))) {
            self.spawn_sound_load(id, p);
        }
        println!("sound placed at the playhead");
    }

    /// Decode every sound the comp names that isn't loaded or loading.
    pub(crate) fn sync_sounds(&mut self) {
        let want: Vec<(u32, String)> = self
            .audio_editor()
            .sounds()
            .iter()
            .filter(|s| !self.sounds.contains_key(&s.id))
            .map(|s| (s.id, s.path.clone()))
            .collect();
        for (id, path) in want {
            self.spawn_sound_load(id, path);
        }
    }

    fn spawn_sound_load(&mut self, id: u32, path: String) {
        self.sounds.insert(id, Slot::Loading);
        let proxy = self.proxy.clone();
        std::thread::spawn(move || {
            let result = Sound::load(std::path::Path::new(&path)).map_err(|e| e.to_string());
            let _ = proxy.send_event(AppEvent::SoundLoaded(id, path, result));
        });
    }

    /// A sound's decode finished — or didn't.
    pub(crate) fn sound_loaded(&mut self, id: u32, path: String, result: Result<Sound, String>) {
        match result {
            Ok(s) => {
                println!("sound ready: {path} ({:.1}s)", s.duration);
                self.sounds.insert(id, Slot::Ready(s));
            }
            Err(e) => {
                println!("couldn't load sound {path}: {e}");
                self.sounds.insert(id, Slot::Missing);
            }
        }
        self.request_redraw();
    }

    /// The song's decode failed: its clips show as missing.
    pub(crate) fn song_missing(&mut self, missing: bool) {
        if missing {
            self.sounds.insert(SONG, Slot::Missing);
        } else {
            self.sounds.remove(&SONG);
        }
    }

    // ---- the volume box ---------------------------------------------

    /// A press on a track's volume box: the drag starts from what it
    /// reads. Read-only inside a placed comp.
    pub(crate) fn volume_press(&mut self, asset: u32, cy: f32) {
        if self.in_comp() {
            return;
        }
        self.vol_drag = Some(VolDrag {
            asset,
            start_y: cy,
            start_db: db_of(self.audio_editor().volume(asset)),
        });
    }

    /// The cursor moved with a volume box held: up raises, Shift is
    /// fine. True when the frame needs redrawing.
    pub(crate) fn volume_moved(&mut self, my: f32) -> bool {
        let Some(d) = self.vol_drag else {
            return false;
        };
        let db = dragged_db(d.start_db, my - d.start_y, self.scale(), self.modifiers.shift_key());
        self.editor.set_volume(d.asset, gain_of(db))
    }

    /// The box let go: the drag was one undo step. True when one was held.
    pub(crate) fn volume_release(&mut self) -> bool {
        self.vol_drag.take().is_some()
    }

    // ---- clip selection ---------------------------------------------

    /// The clip a single-clip verb acts on: the last one selected.
    pub(crate) fn primary_clip(&self) -> Option<ClipRef> {
        self.selected_clips.last().copied()
    }

    /// Ctrl+A over the timeline: every clip on the arrangement, so the
    /// whole thing can be shoved right to make room for an intro.
    pub(crate) fn select_all_clips(&mut self) -> bool {
        let mut all = Vec::new();
        for i in 0..self.editor.shapes().len() {
            let obj = self.editor.shape_id(i);
            all.extend((0..self.editor.obj_clips(i).len()).map(|c| ClipRef::Obj { obj, c }));
        }
        all.extend((0..self.editor.comp_clips().len()).map(ClipRef::Comp));
        if !self.in_comp() {
            all.extend((0..self.editor.audio_clips().len()).map(ClipRef::Audio));
        }
        let changed = all != self.selected_clips;
        self.selected_clips = all;
        println!("{} clips selected", self.selected_clips.len());
        changed
    }

    /// Delete every selected clip — one undo step. Audio clips resolve
    /// by index, so they go highest first.
    pub(crate) fn delete_selected_clips(&mut self) -> bool {
        let sel = std::mem::take(&mut self.selected_clips);
        if sel.is_empty() {
            return false;
        }
        let mut objs: Vec<(u32, usize)> = Vec::new();
        let mut comps: Vec<usize> = Vec::new();
        let mut audio: Vec<usize> = Vec::new();
        for r in sel {
            match r {
                ClipRef::Obj { obj, c } => objs.push((obj, c)),
                ClipRef::Comp(i) => comps.push(i),
                ClipRef::Audio(k) => audio.push(k),
            }
        }
        // Highest index first within a list, so earlier deletions never
        // renumber a later one.
        objs.sort_by(|a, b| b.cmp(a));
        comps.sort_by(|a, b| b.cmp(a));
        audio.sort_by(|a, b| b.cmp(a));
        let mut any = false;
        for (obj, c) in objs {
            if let Some(i) = self.editor.index_of(obj) {
                any |= self.editor.delete_obj_clip(i, c);
            }
        }
        for i in comps {
            any |= self.editor.delete_clip(i);
        }
        if !self.in_comp() {
            for k in audio {
                any |= self.editor.delete_audio_clip(k);
            }
        }
        any
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The box reads in decibels, unity is +0.0, silence is -inf, and a
    /// drag moves a quarter dB a pixel (a tenth of that with Shift).
    #[test]
    fn volume_reads_in_decibels_and_drags_by_the_pixel() {
        assert_eq!(volume_text(1.0), "+0.0 dB");
        assert_eq!(volume_text(0.5), "-6.0 dB");
        assert_eq!(volume_text(0.0), "-inf dB");
        assert!((gain_of(db_of(0.25)) - 0.25).abs() < 1e-4);
        assert_eq!(gain_of(DB_MIN), 0.0);
        assert!((dragged_db(0.0, -40.0, 1.0, false) - 10.0).abs() < 1e-4, "up raises");
        assert!((dragged_db(0.0, 40.0, 2.0, false) - -5.0).abs() < 1e-4, "scaled");
        assert!((dragged_db(0.0, -40.0, 1.0, true) - 1.0).abs() < 1e-4, "Shift is fine");
        assert_eq!(dragged_db(0.0, -1000.0, 1.0, false), DB_MAX, "capped");
        assert_eq!(dragged_db(0.0, 1000.0, 1.0, false), DB_MIN, "floored");
    }
}
