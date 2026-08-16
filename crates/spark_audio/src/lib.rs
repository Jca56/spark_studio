//! spark_audio — Spark's audio engine.
//!
//! The song is fixed, so everything is analyzed once up front and baked:
//! decode the whole track (FFmpeg subprocess → raw PCM), then run our own
//! FFT over it to produce analysis curves any parameter can bind to.
//! Playback (cpal) arrives next; it will read the same baked samples.

mod analysis;
mod decode;
pub mod fft;

use std::path::Path;
use std::sync::Arc;

pub use analysis::Curves;

/// Everything is resampled to this rate on decode.
pub const SAMPLE_RATE: u32 = 48_000;
/// Mono samples per waveform peak bucket.
pub const PEAK_BUCKET: usize = 512;

/// A fully decoded and analyzed track, immutable once built.
pub struct Track {
    /// Interleaved stereo f32 at [`SAMPLE_RATE`], kept for playback.
    pub samples: Arc<Vec<f32>>,
    /// Seconds.
    pub duration: f32,
    /// Per-bucket `[min, max]` of the mono mix — the timeline waveform.
    pub peaks: Vec<[f32; 2]>,
    pub curves: Curves,
}

impl Track {
    /// Decode + analyze. Slow (seconds for a full song) — call off-thread.
    pub fn load(path: &Path) -> Result<Track, String> {
        let stereo = decode::decode(path)?;
        if stereo.is_empty() {
            return Err("decoded zero samples".into());
        }
        let mono: Vec<f32> = stereo
            .chunks_exact(2)
            .map(|c| (c[0] + c[1]) * 0.5)
            .collect();
        let peaks = analysis::peaks(&mono);
        let curves = analysis::curves(&mono);
        Ok(Track {
            duration: mono.len() as f32 / SAMPLE_RATE as f32,
            samples: Arc::new(stereo),
            peaks,
            curves,
        })
    }
}
