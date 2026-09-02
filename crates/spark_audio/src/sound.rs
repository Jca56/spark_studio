//! A decoded audio file, ready to place on the timeline: the samples
//! the mixer plays and the peaks the arrangement draws. No analysis —
//! only the song is analyzed (see [`crate::Track`]); a sound is just
//! sound.

use std::path::Path;
use std::sync::Arc;

use crate::{SAMPLE_RATE, analysis, decode};

pub struct Sound {
    /// Interleaved stereo f32 at [`SAMPLE_RATE`].
    pub samples: Arc<Vec<f32>>,
    /// Seconds.
    pub duration: f32,
    /// Per-bucket `[min, max]` of the mono mix — the waveform.
    pub peaks: Vec<[f32; 2]>,
}

impl Sound {
    /// Decode (FFmpeg subprocess) and bucket the peaks. Call off-thread:
    /// a long file takes a moment.
    pub fn load(path: &Path) -> Result<Sound, String> {
        let stereo = decode::decode(path)?;
        if stereo.is_empty() {
            return Err("decoded zero samples".into());
        }
        let mono = mono_of(&stereo);
        Ok(Sound {
            duration: mono.len() as f32 / SAMPLE_RATE as f32,
            peaks: analysis::peaks(&mono),
            samples: Arc::new(stereo),
        })
    }
}

/// The mono mix of interleaved stereo.
pub(crate) fn mono_of(stereo: &[f32]) -> Vec<f32> {
    stereo
        .chunks_exact(2)
        .map(|c| (c[0] + c[1]) * 0.5)
        .collect()
}
