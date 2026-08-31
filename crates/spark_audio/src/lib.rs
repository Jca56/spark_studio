//! spark_audio — Spark's audio engine.
//!
//! The song is fixed, so everything is analyzed once up front and baked:
//! decode the whole track (FFmpeg subprocess → raw PCM), then run our own
//! FFT over it to produce analysis curves any parameter can bind to.
//! Playback (cpal) arrives next; it will read the same baked samples.

mod analysis;
mod beat;
mod cache;
mod decode;
pub mod fft;
mod player;

use std::path::Path;
use std::sync::Arc;

pub use analysis::{CURVE_LAG, Curves};
pub use beat::BeatGrid;
pub use player::Player;

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
    pub beat: BeatGrid,
}

impl Track {
    /// Decode + analyze. Slow (seconds for a full song) — call off-thread.
    pub fn load(path: &Path) -> Result<Track, String> {
        Self::load_cached(path, None)
    }

    /// The same, with the analysis baked to `cache` (see `cache.rs`):
    /// the first import pays the FFT, every later open of the same file
    /// skips straight to the song. Decode always runs — playback needs
    /// the samples, and decode is the fast half.
    pub fn load_cached(path: &Path, cache_dir: Option<&Path>) -> Result<Track, String> {
        let stereo = decode::decode(path)?;
        if stereo.is_empty() {
            return Err("decoded zero samples".into());
        }
        let mono: Vec<f32> = stereo
            .chunks_exact(2)
            .map(|c| (c[0] + c[1]) * 0.5)
            .collect();
        let duration = mono.len() as f32 / SAMPLE_RATE as f32;
        let samples = Arc::new(stereo);
        if let Some(dir) = cache_dir
            && let Some(b) = cache::read(dir, path)
        {
            println!("analysis from cache");
            return Ok(Track {
                duration,
                samples,
                peaks: b.peaks,
                curves: b.curves,
                beat: b.beat,
            });
        }
        let peaks = analysis::peaks(&mono);
        let curves = analysis::curves(&mono);
        let beat = beat::beat_grid(&curves.onset, &curves.bass, curves.rate);
        if let Some(dir) = cache_dir {
            cache::write(
                dir,
                path,
                &cache::Baked {
                    peaks: peaks.clone(),
                    curves: Curves {
                        rate: curves.rate,
                        bass: curves.bass.clone(),
                        low_mid: curves.low_mid.clone(),
                        mid: curves.mid.clone(),
                        high: curves.high.clone(),
                        rms: curves.rms.clone(),
                        onset: curves.onset.clone(),
                    },
                    beat,
                },
            );
        }
        Ok(Track {
            duration,
            samples,
            peaks,
            curves,
            beat,
        })
    }
}
