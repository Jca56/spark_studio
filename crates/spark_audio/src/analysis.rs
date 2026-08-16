//! Offline analysis: waveform peaks and the audio-reactive curves — band
//! energies tuned for EDM (kick/sub, snare body, leads, hats), onset
//! strength (spectral flux), and the RMS loudness envelope.
//!
//! Curves are sampled per FFT hop and normalized to 0..1 over the whole
//! track, so "Scale = 1.0 + bass × 0.5" behaves the same on every song.

use crate::{PEAK_BUCKET, SAMPLE_RATE, fft::fft};

const WINDOW: usize = 2048;
const HOP: usize = 512;

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
    pub fn sample(curve: &[f32], rate: f32, t: f32) -> f32 {
        if curve.is_empty() {
            return 0.0;
        }
        let x = (t * rate).max(0.0);
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
        out.onset.push(flux);

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

/// Estimated tempo and bar phase, from the onset curve.
pub struct BeatGrid {
    pub bpm: f32,
    /// Seconds to the first bar line.
    pub first_bar: f32,
}

/// Autocorrelate the onsets over 60–180 BPM lags, fold into the EDM-typical
/// 100–200 range, then comb for the bar phase that catches the most onset
/// energy. Crude, user-correctable later — but dubstep sits on the grid.
pub fn beat_grid(onset: &[f32], rate: f32) -> BeatGrid {
    let min_lag = ((rate * 60.0 / 180.0) as usize).max(1);
    let max_lag = ((rate * 60.0 / 60.0) as usize).min(onset.len() / 2);
    let mut best = (min_lag, -1.0f32);
    for lag in min_lag..max_lag.max(min_lag + 1) {
        let mut sum = 0.0f32;
        for i in 0..onset.len().saturating_sub(lag) {
            sum += onset[i] * onset[i + lag];
        }
        let score = sum / onset.len().saturating_sub(lag).max(1) as f32;
        if score > best.1 {
            best = (lag, score);
        }
    }
    let mut bpm = 60.0 * rate / best.0 as f32;
    while bpm < 100.0 {
        bpm *= 2.0;
    }
    while bpm > 200.0 {
        bpm /= 2.0;
    }

    let bar = rate * 4.0 * 60.0 / bpm;
    let steps = (bar as usize).max(1);
    let mut best_phase = 0usize;
    let mut best_e = -1.0f32;
    for phase in 0..steps {
        let mut e = 0.0f32;
        let mut i = phase as f32;
        while (i as usize) < onset.len() {
            e += onset[i as usize];
            i += bar;
        }
        if e > best_e {
            best_e = e;
            best_phase = phase;
        }
    }
    BeatGrid {
        bpm,
        first_bar: best_phase as f32 / rate,
    }
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
