//! Tempo and downbeat: where the bar lines go.
//!
//! Two questions, and they are not the same question. *How fast* comes from
//! autocorrelating the onset curve — the track's own self-similarity at one
//! beat's delay. *Where does bar one start* is harder, because the loudest
//! recurring transient in EDM is usually the snare, not the kick, and a
//! search that just looks for the biggest repeating hit lands confidently on
//! beat two or three. That is a quarter or a half bar out, and everything
//! downstream — the ruler, the shading, every keyframe stamped against it —
//! inherits the error.
//!
//! So the downbeat is chosen in two stages: lock the *beat* phase from the
//! onsets (any beat will do, they're all equally spaced), then decide which
//! of the four beats in the bar is beat one by asking the **bass** band,
//! where a kick dominates and a snare doesn't.

use crate::analysis::CURVE_LAG;

/// Estimated tempo and bar phase.
pub struct BeatGrid {
    pub bpm: f32,
    /// Seconds to the first bar line.
    pub first_bar: f32,
}

/// Tempos we search, before folding into the EDM-typical range.
const MIN_BPM: f32 = 60.0;
const MAX_BPM: f32 = 180.0;
const FOLD_LO: f32 = 100.0;
const FOLD_HI: f32 = 200.0;

/// How near a whole BPM the estimate has to land to be treated as that
/// tempo. See [`snap_bpm`].
const SNAP: f32 = 0.4;

pub fn beat_grid(onset: &[f32], bass: &[f32], rate: f32) -> BeatGrid {
    let bpm = tempo(onset, rate);
    let beat_hops = rate * 60.0 / bpm.max(1.0);
    let bar_hops = beat_hops * 4.0;

    // Which offset within a beat catches the most onset energy. Every beat
    // is a candidate here, so this only has to find the pulse, not the one.
    let beat_phase = best_phase(onset, beat_hops, beat_hops.round() as usize);

    // ...and which of the four is beat one. The kick lives in the bass band
    // and the snare mostly doesn't, so combing *that* at the bar period
    // separates them where the broadband onset curve can't.
    let mut best = (0.0f32, -1.0f32);
    for j in 0..4 {
        let start = beat_phase + beat_hops * j as f32;
        let e = comb(bass, start, bar_hops);
        if e > best.1 {
            best = (start, e);
        }
    }

    // The curves run a window-centre behind the times they're indexed by, so
    // the grid found in them does too.
    let mut first_bar = best.0 / rate + CURVE_LAG;
    // Walk back to the *earliest* bar line, since they're all equally valid
    // and the timeline starts here — a track that opens on the downbeat was
    // otherwise losing its whole first bar to a phase that had drifted a
    // hair past zero.
    let bar_s = bar_hops / rate;
    while first_bar - bar_s >= 0.0 {
        first_bar -= bar_s;
    }
    BeatGrid { bpm, first_bar }
}

/// Beats per minute, from the onset curve's self-similarity.
fn tempo(onset: &[f32], rate: f32) -> f32 {
    let n = onset.len();
    let min_lag = ((rate * 60.0 / MAX_BPM) as usize).max(1);
    let max_lag = ((rate * 60.0 / MIN_BPM) as usize).min(n / 2);
    if max_lag <= min_lag + 1 {
        return FOLD_LO;
    }
    // Mean-subtract first. A curve that never goes negative carries a large
    // DC term, and correlating it with itself buries the periodic ripple
    // under the square of that mean — every lag scores about the same.
    let mean = onset.iter().sum::<f32>() / n as f32;
    let dev: Vec<f32> = onset.iter().map(|v| v - mean).collect();
    let score = |lag: usize| -> f32 {
        let overlap = n.saturating_sub(lag);
        if overlap == 0 {
            return 0.0;
        }
        let mut sum = 0.0f32;
        for i in 0..overlap {
            sum += dev[i] * dev[i + lag];
        }
        sum / overlap as f32
    };
    let mut best = (min_lag, f32::NEG_INFINITY);
    for lag in min_lag..max_lag {
        let s = score(lag);
        if s > best.1 {
            best = (lag, s);
        }
    }
    // Whole-hop lags are a coarse grid: at ~140 BPM one hop either way is
    // over 3 BPM, and 0.5% of tempo error walks the grid a whole beat off by
    // the end of a two-minute track. Fit a parabola through the winning lag
    // and its neighbours to land between hops.
    let lag = refine(best.0, &score, min_lag, max_lag);
    snap_bpm(fold(60.0 * rate / lag))
}

/// Sub-hop peak position, by fitting a parabola through `l-1, l, l+1`.
fn refine(l: usize, score: &impl Fn(usize) -> f32, min_lag: usize, max_lag: usize) -> f32 {
    if l <= min_lag || l + 1 >= max_lag {
        return l as f32;
    }
    let (a, b, c) = (score(l - 1), score(l), score(l + 1));
    let denom = a - 2.0 * b + c;
    if denom >= 0.0 {
        return l as f32;
    }
    let delta = (0.5 * (a - c) / denom).clamp(-0.5, 0.5);
    l as f32 + delta
}

/// Halve or double into the range EDM actually sits in, so locking onto the
/// half-bar instead of the beat still lands on the right tempo.
fn fold(mut bpm: f32) -> f32 {
    if !bpm.is_finite() || bpm <= 0.0 {
        return FOLD_LO;
    }
    while bpm < FOLD_LO {
        bpm *= 2.0;
    }
    while bpm > FOLD_HI {
        bpm *= 0.5;
    }
    bpm
}

/// Round to a whole BPM when the estimate is already within [`SNAP`] of one.
///
/// Spark's tracks are produced, not performed: the tempo was typed into a
/// DAW and it is an integer essentially always. An estimate of 140.6 is not
/// evidence of a 140.6 BPM song, it's evidence of a 140 BPM song measured
/// through a finite window — and keeping the .6 costs a full beat of drift
/// across two minutes. Anything further out than `SNAP` is left alone, since
/// then the estimate is probably wrong in a way rounding won't rescue.
fn snap_bpm(bpm: f32) -> f32 {
    let whole = bpm.round();
    if (bpm - whole).abs() <= SNAP && whole > 0.0 {
        whole
    } else {
        bpm
    }
}

/// The offset in `0..steps` whose comb at `period` catches the most energy.
fn best_phase(curve: &[f32], period: f32, steps: usize) -> f32 {
    let mut best = (0.0f32, f32::NEG_INFINITY);
    for phase in 0..steps.max(1) {
        let e = comb(curve, phase as f32, period);
        if e > best.1 {
            best = (phase as f32, e);
        }
    }
    best.0
}

/// Sum `curve` at `start`, `start + period`, `start + 2*period`... Each tap
/// takes its sample plus its immediate neighbours: a transient spreads over
/// two or three hops, and a comb that reads one hop exactly is decided by
/// which side of a boundary the attack happened to land on.
fn comb(curve: &[f32], start: f32, period: f32) -> f32 {
    if period < 1.0 || curve.is_empty() {
        return 0.0;
    }
    let mut e = 0.0f32;
    let mut at = start;
    while (at as usize) < curve.len() {
        let i = at as usize;
        let lo = i.saturating_sub(1);
        let hi = (i + 2).min(curve.len());
        for &v in &curve[lo..hi] {
            e += v;
        }
        at += period;
    }
    e
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{SAMPLE_RATE, analysis};

    /// A deterministic noise source — snares need broadband content and a
    /// test needs the same one every run.
    struct Lcg(u32);

    impl Lcg {
        fn next(&mut self) -> f32 {
            self.0 = self.0.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            (self.0 >> 8) as f32 / 8_388_608.0 - 1.0
        }
    }

    /// A four-on-the-floor bar pattern: a deep kick on beat one, a bright
    /// snare on two and four. The snare is the *louder* event in spectral
    /// flux — broadband noise moves far more bins than a sine does — which
    /// is exactly the trap the old single-stage phase search fell into.
    fn track(bpm: f32, downbeat: f32, seconds: f32) -> Vec<f32> {
        let sr = SAMPLE_RATE as f32;
        let mut mono = vec![0.0f32; (sr * seconds) as usize];
        let beat = 60.0 / bpm;
        let mut rng = Lcg(0x5EED);
        let mut n = 0;
        loop {
            let t = downbeat + beat * n as f32;
            if t >= seconds {
                break;
            }
            let at = (t * sr) as usize;
            match n % 4 {
                0 => {
                    // Kick: 55 Hz, 140 ms, fast decay.
                    for k in 0..(sr * 0.14) as usize {
                        let Some(s) = mono.get_mut(at + k) else { break };
                        let x = k as f32 / sr;
                        *s += (x * 55.0 * std::f32::consts::TAU).sin() * (-x * 22.0).exp();
                    }
                }
                2 => {}
                _ => {
                    // Snare: noise burst, 90 ms.
                    for k in 0..(sr * 0.09) as usize {
                        let Some(s) = mono.get_mut(at + k) else { break };
                        let x = k as f32 / sr;
                        *s += rng.next() * 0.8 * (-x * 34.0).exp();
                    }
                }
            }
            n += 1;
        }
        mono
    }

    fn grid_of(bpm: f32, downbeat: f32) -> BeatGrid {
        let mono = track(bpm, downbeat, 24.0);
        let c = analysis::curves(&mono);
        beat_grid(&c.onset, &c.bass, c.rate)
    }

    /// Whole-hop lags can only express ~3 BPM steps around 140. Producers
    /// type whole numbers, and a fraction of a percent of tempo error is a
    /// whole beat of drift by the end of a track.
    #[test]
    fn tempo_lands_on_the_number_the_producer_typed() {
        for bpm in [140.0, 150.0, 174.0] {
            let got = grid_of(bpm, 0.5).bpm;
            assert!(
                (got - bpm).abs() < 0.01,
                "{bpm} BPM track measured as {got}"
            );
        }
    }

    /// The one that matters: bar one is the kick, even though the snare is
    /// the louder transient. Getting this wrong is a quarter-bar error in
    /// every bar line on screen.
    #[test]
    fn the_downbeat_is_the_kick_not_the_snare() {
        let bar = 4.0 * 60.0 / 140.0;
        for downbeat in [0.5f32, 0.93, 1.4] {
            let g = grid_of(140.0, downbeat);
            // Any bar line is a correct answer; being a beat out is not.
            let off = (g.first_bar - downbeat).rem_euclid(bar);
            let err = off.min(bar - off);
            assert!(
                err < 0.05,
                "downbeat {downbeat}s came back as {:.3}s ({err:.3}s into the bar)",
                g.first_bar
            );
        }
    }

    /// Bar lines repeat forever, so the one to report is the earliest — the
    /// timeline starts there, and everything before it is unreachable.
    #[test]
    fn the_first_bar_is_the_first_one() {
        let bar = 4.0 * 60.0 / 140.0;
        // A downbeat two bars in still has bar lines below it.
        let g = grid_of(140.0, 0.5 + bar * 2.0);
        assert!(
            g.first_bar < bar,
            "first bar reported at {:.3}s, but a bar is {bar:.3}s",
            g.first_bar
        );
        assert!(g.first_bar >= 0.0, "before the track started");
    }

    /// Folding has to survive locking onto a multiple of the beat.
    #[test]
    fn folding_reaches_the_edm_range() {
        assert_eq!(fold(70.0), 140.0, "half tempo doubles");
        assert_eq!(fold(35.0), 140.0, "quarter tempo doubles twice");
        assert_eq!(fold(280.0), 140.0, "double tempo halves");
        assert!((FOLD_LO..=FOLD_HI).contains(&fold(140.0)), "already in range");
    }

    /// Snapping rescues a measurement, not a bad guess.
    #[test]
    fn snapping_only_catches_near_misses() {
        assert_eq!(snap_bpm(140.35), 140.0, "a measurement error rounds");
        assert_eq!(snap_bpm(139.7), 140.0);
        // Far enough out that rounding would be inventing a number.
        assert_eq!(snap_bpm(140.5), 140.5, "left alone");
    }
}
