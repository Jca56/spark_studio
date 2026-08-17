//! Offline analysis: waveform peaks and the audio-reactive curves — band
//! energies tuned for EDM (kick/sub, snare body, leads, hats), onset
//! strength (spectral flux), and the RMS loudness envelope.
//!
//! Curves are sampled per FFT hop and normalized to 0..1 over the whole
//! track, so "Scale = 1.0 + bass × 0.5" behaves the same on every song.

use crate::{PEAK_BUCKET, SAMPLE_RATE, fft::fft};

pub(crate) const WINDOW: usize = 2048;
pub(crate) const HOP: usize = 512;

/// How far a curve sample lags the time it is labelled with.
///
/// Sample `h` is the FFT of `mono[h*HOP .. h*HOP + WINDOW]`, so it describes
/// a whole window of audio, and what it describes is centred half a window
/// *after* the moment the index names. Ignoring that made every curve read
/// ~21 ms early — shapes reacting before the hit, and a beat grid drawn
/// slightly ahead of the transients it was found from.
pub const CURVE_LAG: f32 = WINDOW as f32 * 0.5 / SAMPLE_RATE as f32;

/// Analysis curves, all the same length, [`Curves::rate`] samples/second.
pub struct Curves {
    pub rate: f32,
    pub bass: Vec<f32>,
    pub low_mid: Vec<f32>,
    pub mid: Vec<f32>,
    pub high: Vec<f32>,
    pub rms: Vec<f32>,
    pub onset: Vec<f32>,
}

impl Curves {
    /// Linear-interpolated lookup of one curve at time `t` (seconds).
    ///
    /// `t` is a moment in the song; [`CURVE_LAG`] converts it to the sample
    /// whose window is *centred* there, so what you read at `t` is what the
    /// track is doing at `t`.
    pub fn sample(curve: &[f32], rate: f32, t: f32) -> f32 {
        if curve.is_empty() {
            return 0.0;
        }
        let x = ((t - CURVE_LAG) * rate).max(0.0);
        let i = (x as usize).min(curve.len() - 1);
        let j = (i + 1).min(curve.len() - 1);
        let frac = x - i as f32;
        curve[i] * (1.0 - frac) + curve[j] * frac
    }
}

/// Waveform min/max per [`PEAK_BUCKET`] mono samples.
pub fn peaks(mono: &[f32]) -> Vec<[f32; 2]> {
    mono.chunks(PEAK_BUCKET)
        .map(|c| {
            let mut lo = 0.0f32;
            let mut hi = 0.0f32;
            for &s in c {
                lo = lo.min(s);
                hi = hi.max(s);
            }
            [lo, hi]
        })
        .collect()
}

/// Frequency band edges in Hz: (kick/sub, snare body, leads/vocals, hats/air).
const BANDS: [(f32, f32); 4] = [
    (20.0, 150.0),
    (150.0, 500.0),
    (500.0, 3000.0),
    (3000.0, 12000.0),
];

pub fn curves(mono: &[f32]) -> Curves {
    let rate = SAMPLE_RATE as f32 / HOP as f32;
    let hann: Vec<f32> = (0..WINDOW)
        .map(|i| {
            let x = i as f32 / (WINDOW - 1) as f32;
            0.5 - 0.5 * (2.0 * std::f32::consts::PI * x).cos()
        })
        .collect();
    let hz_per_bin = SAMPLE_RATE as f32 / WINDOW as f32;
    let bins: Vec<(usize, usize)> = BANDS
        .iter()
        .map(|&(lo, hi)| {
            let a = ((lo / hz_per_bin) as usize).max(1);
            let b = ((hi / hz_per_bin) as usize).min(WINDOW / 2);
            (a, b.max(a + 1))
        })
        .collect();

    let hops = mono.len().saturating_sub(WINDOW) / HOP;
    let mut out = Curves {
        rate,
        bass: Vec::with_capacity(hops),
        low_mid: Vec::with_capacity(hops),
        mid: Vec::with_capacity(hops),
        high: Vec::with_capacity(hops),
        rms: Vec::with_capacity(hops),
        onset: Vec::with_capacity(hops),
    };
    let mut re = vec![0.0f32; WINDOW];
    let mut im = vec![0.0f32; WINDOW];
    let mut mag = vec![0.0f32; WINDOW / 2];
    let mut prev_mag = vec![0.0f32; WINDOW / 2];

    for h in 0..hops {
        let frame = &mono[h * HOP..h * HOP + WINDOW];
        let mut sq = 0.0f32;
        for i in 0..WINDOW {
            re[i] = frame[i] * hann[i];
            im[i] = 0.0;
            sq += frame[i] * frame[i];
        }
        out.rms.push((sq / WINDOW as f32).sqrt());
        fft(&mut re, &mut im);
        for i in 0..WINDOW / 2 {
            mag[i] = (re[i] * re[i] + im[i] * im[i]).sqrt();
        }
        let mut flux = 0.0f32;
        for i in 0..WINDOW / 2 {
            flux += (mag[i] - prev_mag[i]).max(0.0);
        }
        // There is no frame before the first, so its "rise" is the entire
        // spectrum arriving out of silence — a full-scale onset at t=0 that
        // isn't one. It flashed every audio-reactive shape on the first
        // frame and put a thumb on the scale of every beat-phase search.
        out.onset.push(if h == 0 { 0.0 } else { flux });

        for (band, curve) in
            bins.iter()
                .zip([&mut out.bass, &mut out.low_mid, &mut out.mid, &mut out.high])
        {
            let (a, b) = *band;
            let mut e = 0.0f32;
            for &m in &mag[a..b] {
                e += m * m;
            }
            curve.push((e / (b - a) as f32).sqrt());
        }
        std::mem::swap(&mut mag, &mut prev_mag);
    }

    for curve in [
        &mut out.bass,
        &mut out.low_mid,
        &mut out.mid,
        &mut out.high,
        &mut out.rms,
        &mut out.onset,
    ] {
        normalize(curve);
    }
    out
}

/// Scale a curve into 0..1 by its 98th percentile, clamped — one freak
/// transient shouldn't flatten the whole song.
fn normalize(curve: &mut [f32]) {
    if curve.is_empty() {
        return;
    }
    let mut sorted: Vec<f32> = curve.to_vec();
    sorted.sort_by(|a, b| a.total_cmp(b));
    let p98 = sorted[(sorted.len() - 1) * 98 / 100].max(f32::MIN_POSITIVE);
    for v in curve {
        *v = (*v / p98).min(1.0);
    }
}
