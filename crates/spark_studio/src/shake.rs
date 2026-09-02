//! Camera shake: how far the render camera is jolted at a moment, from a
//! camera object's Amount and Speed on its clip's clock.
//!
//! A continuous rumble — Alva's pick (2026-09-02) over hits that decay:
//! smooth noise scaled by Amount, so React on Amount is the drop and a
//! key in and out is the fade. The noise is our own 1D value noise: a
//! random height at every lattice point, a smooth curve between, in two
//! octaves — the second finer and fainter, so the rumble has grit without
//! jittering. Speed is lattice points a second: how many times a second
//! the camera changes its mind. Each axis reads its own lattice, so the
//! jolt wanders rather than sliding along one diagonal.
//!
//! A pure function of (amount, speed, t): a paused frame shakes exactly
//! as the same frame in motion, and export matches the viewer.

/// A random height in [-1, 1] at lattice point `i` of lattice `seed`.
fn lattice(i: i32, seed: u32) -> f32 {
    let mut h = (i as u32).wrapping_mul(0x9E37_79B1) ^ seed.wrapping_mul(0x85EB_CA77);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2C1B_3C6D);
    h ^= h >> 12;
    h = h.wrapping_mul(0x297A_2D39);
    h ^= h >> 15;
    (h as f32 / u32::MAX as f32) * 2.0 - 1.0
}

/// 1D value noise: the lattice's heights, smoothstepped between.
fn value_noise(x: f32, seed: u32) -> f32 {
    let i = x.floor();
    let f = x - i;
    let a = lattice(i as i32, seed);
    let b = lattice(i as i32 + 1, seed);
    a + (b - a) * (f * f * (3.0 - 2.0 * f))
}

/// The second octave's pitch and weight over the first.
const GRIT_PITCH: f32 = 2.3;
const GRIT: f32 = 0.35;

/// Two octaves, normalised so the sum stays within [-1, 1].
fn rumble(x: f32, seed: u32) -> f32 {
    (value_noise(x, seed) + GRIT * value_noise(x * GRIT_PITCH + 7.1, seed ^ 0xA5A5)) / (1.0 + GRIT)
}

/// The jolt at clip time `t`: canvas units right and down, never further
/// than `amount`, changing its mind `rate` times a second. Nothing at an
/// amount or a speed of zero — a speed of zero is still, not stuck.
pub fn offset(amount: f32, rate: f32, t: f32) -> [f32; 2] {
    if amount <= 0.0 || rate <= 0.0 {
        return [0.0; 2];
    }
    let x = t * rate;
    [amount * rumble(x, 1), amount * rumble(x, 2)]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nothing_at_zero() {
        assert_eq!(offset(0.0, 12.0, 3.3), [0.0; 2]);
        assert_eq!(offset(20.0, 0.0, 3.3), [0.0; 2]);
        assert_eq!(offset(20.0, -1.0, 3.3), [0.0; 2]);
    }

    /// Never past the amount, and it does get most of the way there.
    #[test]
    fn stays_within_the_amount_and_uses_it() {
        let amount = 25.0;
        let mut peak = 0.0f32;
        for k in 0..20_000 {
            let [x, y] = offset(amount, 12.0, k as f32 * 0.0037);
            assert!(x.abs() <= amount + 1e-3 && y.abs() <= amount + 1e-3, "{x} {y}");
            peak = peak.max(x.abs()).max(y.abs());
        }
        assert!(peak > amount * 0.6, "the rumble barely reached {peak}");
    }

    /// It moves, both axes on their own, and smoothly: no jump a frame
    /// could see as a cut.
    #[test]
    fn wanders_smoothly_on_two_axes() {
        let (amount, rate) = (20.0, 12.0);
        let a = offset(amount, rate, 0.1);
        let b = offset(amount, rate, 0.6);
        assert_ne!(a, b);
        assert!((a[0] - a[1]).abs() > 1e-3, "the axes read different lattices: {a:?}");
        let dt = 1e-3;
        for k in 0..5_000 {
            let t = k as f32 * 0.0021;
            let p = offset(amount, rate, t);
            let q = offset(amount, rate, t + dt);
            let step = ((q[0] - p[0]).powi(2) + (q[1] - p[1]).powi(2)).sqrt();
            assert!(step < amount * rate * dt * 5.0, "a jump of {step} at {t}");
        }
    }

    /// Speed is time's scale, and the same moment always shakes the same
    /// way — a paused frame is the frame in motion.
    #[test]
    fn speed_scales_time_and_the_answer_is_fixed() {
        let a = offset(10.0, 6.0, 2.0);
        let b = offset(10.0, 12.0, 1.0);
        assert!((a[0] - b[0]).abs() < 1e-4 && (a[1] - b[1]).abs() < 1e-4);
        assert_eq!(offset(10.0, 6.0, 2.0), a);
    }
}
