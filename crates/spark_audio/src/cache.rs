//! The analysis cache: peaks, curves and the beat grid baked to one
//! binary file per (track, mtime, size), so reopening a project skips
//! the whole-track FFT and lands in the song instantly. Decode still
//! runs — playback needs the samples — but decode is the fast half.
//!
//! Hand-rolled little-endian binary (dependency policy: we build our
//! own): magic, beat grid, peaks, then the six curves, each a length
//! and its f32s. Any short read or wrong magic is a miss, never an
//! error — a cache's only failure mode is to be re-made.

use std::path::{Path, PathBuf};

use crate::analysis::Curves;
use crate::beat::BeatGrid;

const MAGIC: &[u8; 8] = b"SPARKAC1";

/// Everything analysis produces — what the cache holds.
pub(crate) struct Baked {
    pub peaks: Vec<[f32; 2]>,
    pub curves: Curves,
    pub beat: BeatGrid,
}

/// The cache file for `song`: named by a hash of its path, size and
/// mtime, so an edited or replaced track can never wear a stale bake.
fn slot(dir: &Path, song: &Path) -> Option<PathBuf> {
    let meta = std::fs::metadata(song).ok()?;
    let mtime = meta
        .modified()
        .ok()?
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?;
    // FNV-1a over the identity triple.
    let mut h: u64 = 0xcbf29ce484222325;
    let mut eat = |bytes: &[u8]| {
        for &b in bytes {
            h ^= b as u64;
            h = h.wrapping_mul(0x100000001b3);
        }
    };
    eat(song.to_string_lossy().as_bytes());
    eat(&meta.len().to_le_bytes());
    eat(&mtime.as_secs().to_le_bytes());
    Some(dir.join(format!("{h:016x}.acache")))
}

pub(crate) fn read(dir: &Path, song: &Path) -> Option<Baked> {
    decode(&std::fs::read(slot(dir, song)?).ok()?)
}

pub(crate) fn write(dir: &Path, song: &Path, baked: &Baked) {
    let Some(path) = slot(dir, song) else { return };
    // A cache that can't be written is a cache that gets re-made; the
    // song is not the place to report disk trouble from.
    let _ = std::fs::create_dir_all(dir);
    let _ = std::fs::write(path, encode(baked));
}

pub(crate) fn encode(b: &Baked) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&b.beat.bpm.to_le_bytes());
    out.extend_from_slice(&b.beat.first_bar.to_le_bytes());
    out.extend_from_slice(&(b.peaks.len() as u64).to_le_bytes());
    for p in &b.peaks {
        out.extend_from_slice(&p[0].to_le_bytes());
        out.extend_from_slice(&p[1].to_le_bytes());
    }
    out.extend_from_slice(&b.curves.rate.to_le_bytes());
    for curve in [
        &b.curves.bass,
        &b.curves.low_mid,
        &b.curves.mid,
        &b.curves.high,
        &b.curves.rms,
        &b.curves.onset,
    ] {
        out.extend_from_slice(&(curve.len() as u64).to_le_bytes());
        for v in curve.iter() {
            out.extend_from_slice(&v.to_le_bytes());
        }
    }
    out
}

pub(crate) fn decode(data: &[u8]) -> Option<Baked> {
    let mut at = 0usize;
    let take = |at: &mut usize, n: usize| -> Option<&[u8]> {
        let s = data.get(*at..*at + n)?;
        *at += n;
        Some(s)
    };
    if take(&mut at, 8)? != MAGIC {
        return None;
    }
    let f32_at = |at: &mut usize| -> Option<f32> {
        Some(f32::from_le_bytes(take(at, 4)?.try_into().ok()?))
    };
    let bpm = f32_at(&mut at)?;
    let first_bar = f32_at(&mut at)?;
    let len = |at: &mut usize| -> Option<usize> {
        let n = u64::from_le_bytes(take(at, 8)?.try_into().ok()?);
        // A length no real song reaches is a corrupt file, not a request
        // for forty gigabytes.
        (n < 100_000_000).then_some(n as usize)
    };
    let n = len(&mut at)?;
    let mut peaks = Vec::with_capacity(n);
    for _ in 0..n {
        peaks.push([f32_at(&mut at)?, f32_at(&mut at)?]);
    }
    let rate = f32_at(&mut at)?;
    let curve = |at: &mut usize| -> Option<Vec<f32>> {
        let n = len(at)?;
        let mut v = Vec::with_capacity(n);
        for _ in 0..n {
            v.push(f32_at(at)?);
        }
        Some(v)
    };
    Some(Baked {
        peaks,
        curves: Curves {
            rate,
            bass: curve(&mut at)?,
            low_mid: curve(&mut at)?,
            mid: curve(&mut at)?,
            high: curve(&mut at)?,
            rms: curve(&mut at)?,
            onset: curve(&mut at)?,
        },
        beat: BeatGrid { bpm, first_bar },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn baked() -> Baked {
        Baked {
            peaks: vec![[-0.5, 0.7], [-0.1, 0.2]],
            curves: Curves {
                rate: 93.75,
                bass: vec![0.1, 0.9, 0.4],
                low_mid: vec![0.2],
                mid: vec![0.3, 0.5],
                high: vec![],
                rms: vec![0.6],
                onset: vec![0.0, 1.0],
            },
            beat: BeatGrid {
                bpm: 140.0,
                first_bar: 0.37,
            },
        }
    }

    /// The bake survives the disk byte for byte, and a truncated or
    /// foreign file is a miss rather than a panic or a garbage grid.
    #[test]
    fn the_bake_round_trips_and_corruption_is_a_miss() {
        let bytes = encode(&baked());
        let back = decode(&bytes).expect("decodes");
        let b = baked();
        assert_eq!(back.beat.bpm, b.beat.bpm);
        assert_eq!(back.beat.first_bar, b.beat.first_bar);
        assert_eq!(back.peaks, b.peaks);
        assert_eq!(back.curves.rate, b.curves.rate);
        assert_eq!(back.curves.bass, b.curves.bass);
        assert_eq!(back.curves.high, b.curves.high);
        assert_eq!(back.curves.onset, b.curves.onset);
        for cut in [0, 4, 8, bytes.len() - 1] {
            assert!(decode(&bytes[..cut]).is_none(), "cut at {cut} decoded");
        }
        assert!(decode(b"SPARKXX1whatever").is_none());
    }
}
