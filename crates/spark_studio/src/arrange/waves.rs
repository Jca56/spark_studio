//! The waveforms inside the audio clip bars: each bar carries its
//! file's peaks, mapped through the clip's place and trim, so a
//! trimmed or moved clip shows exactly the stretch of the file it
//! plays. Split from the frame so the painter reads on its own.

use std::collections::HashMap;

use spark_ui::UiRect;

use super::{ArrangeScene, ClipRef};
use crate::doc::{AudioClip, SONG};
use crate::editor::Editor;
use crate::sound::Slot;
use crate::timeline::{Panel, TimeView, wave_rects};

impl crate::Studio {
    /// A map from timeline time to song time that owns what it needs —
    /// the song's clips and length — so a frame can keep it after
    /// `self` is borrowed by its passes. What the waveform overlay maps
    /// the grid through: `None` where the song isn't playing.
    pub(crate) fn song_mapper(&self) -> impl Fn(f32) -> Option<f32> + Clone + 'static {
        let clips: Vec<AudioClip> = self
            .audio_editor()
            .audio_clips()
            .iter()
            .filter(|c| c.asset == SONG)
            .copied()
            .collect();
        let len = self.audio.as_ref().map(|t| t.duration).unwrap_or(0.0);
        move |t: f32| clips.iter().find(|c| c.contains(t, len)).map(|c| c.local(t))
    }
}

/// The waveform rects for every audio clip bar in `sc`, clipped to the
/// axis. `audio_ed` owns the clips; `song` and `sounds` hold the files.
pub fn clip_waves(
    sc: &ArrangeScene,
    panel: &Panel,
    view: &TimeView,
    scale: f32,
    audio_ed: &Editor,
    song: Option<&spark_audio::Track>,
    sounds: &HashMap<u32, Slot>,
) -> Vec<UiRect> {
    let mut out = Vec::new();
    let (ax, aw) = panel.axis;
    for cr in &sc.clips {
        let (Some(asset), ClipRef::Audio(k)) = (cr.audio, cr.r) else {
            continue;
        };
        let Some(ac) = audio_ed.audio_clips().get(k).copied() else {
            continue;
        };
        let (peaks, file_len): (&[[f32; 2]], f32) = if asset == SONG {
            match song {
                Some(t) => (&t.peaks, t.duration),
                None => continue,
            }
        } else {
            match sounds.get(&asset) {
                Some(Slot::Ready(s)) => (&s.peaks, s.duration),
                _ => continue,
            }
        };
        let xr = (cr.bar.x.max(ax), (cr.bar.x + cr.bar.w).min(ax + aw));
        out.extend(wave_rects(
            panel,
            (cr.bar.y, cr.bar.y + cr.bar.h),
            scale,
            peaks,
            file_len,
            0.9,
            xr,
            &|x| Some(ac.local(view.t_at(x, panel.axis))),
        ));
    }
    out
}
