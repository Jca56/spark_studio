//! The export's sound: the arrangement's mix — the song where its clip
//! puts it, every other sound at its place and level — rendered
//! offline through the same mixer playback uses, written as a WAV
//! beside the analysis cache, and handed to FFmpeg as the video's
//! audio input. What you heard is what the file gets; no filter graph
//! has to reproduce the arrangement.
//!
//! The WAV is our own: a 44-byte header and IEEE-float samples, which
//! FFmpeg reads without being told anything. Deleted once the encode
//! is done, by whoever ran it.

use std::io::Write;
use std::path::{Path, PathBuf};

use spark_audio::{SAMPLE_RATE, Voice, render};

/// `samples` (interleaved stereo f32) as a WAV file at `path`.
pub fn write_wav(path: &Path, samples: &[f32]) -> std::io::Result<()> {
    let data_len = (samples.len() * 4) as u32;
    let mut f = std::io::BufWriter::new(std::fs::File::create(path)?);
    f.write_all(b"RIFF")?;
    f.write_all(&(36 + data_len).to_le_bytes())?;
    f.write_all(b"WAVE")?;
    f.write_all(b"fmt ")?;
    f.write_all(&16u32.to_le_bytes())?;
    f.write_all(&3u16.to_le_bytes())?; // IEEE float
    f.write_all(&2u16.to_le_bytes())?; // stereo
    f.write_all(&SAMPLE_RATE.to_le_bytes())?;
    f.write_all(&(SAMPLE_RATE * 2 * 4).to_le_bytes())?; // bytes per second
    f.write_all(&8u16.to_le_bytes())?; // bytes per frame
    f.write_all(&32u16.to_le_bytes())?; // bits per sample
    f.write_all(b"data")?;
    f.write_all(&data_len.to_le_bytes())?;
    for s in samples {
        f.write_all(&s.to_le_bytes())?;
    }
    f.flush()
}

/// Render `range` seconds of `voices` to a fresh WAV in `dir`. `None`
/// with nothing to hear — the video goes out silent, without an audio
/// stream at all.
pub fn render_to_wav(voices: &[Voice], range: (f32, f32), dir: &Path) -> Option<PathBuf> {
    if voices.is_empty() {
        return None;
    }
    let from = (range.0.max(0.0) * SAMPLE_RATE as f32).round() as usize;
    let to = (range.1.max(0.0) * SAMPLE_RATE as f32).round() as usize;
    let frames = to.saturating_sub(from);
    if frames == 0 {
        return None;
    }
    let mix = render(voices, from, frames);
    if std::fs::create_dir_all(dir).is_err() {
        return None;
    }
    let path = dir.join(format!("export-mix-{}.wav", std::process::id()));
    match write_wav(&path, &mix) {
        Ok(()) => Some(path),
        Err(e) => {
            println!("couldn't write the export's mix: {e}");
            None
        }
    }
}

impl crate::Studio {
    /// The mix for an export of `range`, as a file FFmpeg can take.
    pub(crate) fn render_export_audio(&self, range: (f32, f32)) -> Option<PathBuf> {
        let dir = crate::cache_dir()?;
        render_to_wav(&self.voices(), range, &dir)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    /// The header says float stereo at the engine's rate, the data is
    /// the samples verbatim, and a rendered range is cut to its frames.
    #[test]
    fn the_wav_is_float_stereo_and_the_range_is_cut() {
        let dir = std::env::temp_dir().join(format!("spark-wav-test-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let v = Voice {
            samples: Arc::new((0..48000 * 2).map(|i| (i % 7) as f32 * 0.1).collect()),
            at: 0,
            offset: 0,
            len: 48000,
            gain: 1.0,
        };
        let path = render_to_wav(&[v], (0.5, 0.75), &dir).expect("a wav");
        let bytes = std::fs::read(&path).unwrap();
        assert_eq!(&bytes[..4], b"RIFF");
        assert_eq!(&bytes[8..12], b"WAVE");
        assert_eq!(u16::from_le_bytes([bytes[20], bytes[21]]), 3, "IEEE float");
        assert_eq!(u16::from_le_bytes([bytes[22], bytes[23]]), 2, "stereo");
        assert_eq!(
            u32::from_le_bytes([bytes[24], bytes[25], bytes[26], bytes[27]]),
            SAMPLE_RATE
        );
        let frames = 12000; // a quarter second
        let data_len = u32::from_le_bytes([bytes[40], bytes[41], bytes[42], bytes[43]]);
        assert_eq!(data_len as usize, frames * 2 * 4);
        assert_eq!(bytes.len(), 44 + frames * 8);
        // The first sample is the file's frame 24000 (0.5 s in).
        let first = f32::from_le_bytes([bytes[44], bytes[45], bytes[46], bytes[47]]);
        assert!((first - ((24000 * 2) % 7) as f32 * 0.1).abs() < 1e-6);
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_dir(&dir);
        assert!(render_to_wav(&[], (0.0, 1.0), &dir).is_none(), "silence needs no file");
    }
}
