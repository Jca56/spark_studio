//! Value snapping on the graph. With the snap toggle on, a dragged key's
//! value lands on a **round number** — the graph's own value rules, a
//! 1·2·5 step chosen so the rules sit a legible distance apart — or on
//! a **magnet**: another key's value on the same track (a peak the same
//! height as the last one), the setting's floor or ceiling, zero.
//! Alva, 2026-09-01: "Do you know how impossible it is to get a key
//! right at 0 or 100 and not 0.2 or 101.7 … the single most important
//! thing." Ctrl while dragging lets a key go where the hand puts it.
//!
//! Pure numbers — the page draws the rules it hands out, so what you
//! see is where a key will land.

use crate::anim::Target;
use crate::inspector::is_angle;

/// How far apart the value rules sit, at least, logical px.
pub const RULE_PX: f32 = 16.0;
/// How near a magnet a value must come to be taken, logical px.
pub const MAGNET_PX: f32 = 9.0;

/// The rule spacing for a graph showing `span` over `band_px`: the first
/// step in the 1·2·5 series (in degrees for an angle: 1, 5, 10, 15, 30,
/// 45, 90, 180, 360) that is at least `min_px` tall.
pub fn value_step(target: Target, span: (f32, f32), band_px: f32, min_px: f32) -> f32 {
    let (lo, hi) = span;
    let per_px = (hi - lo).abs() / band_px.max(1.0);
    let min = per_px * min_px;
    if let Target::Shape(p) = target
        && is_angle(p)
    {
        const DEG: [f32; 9] = [1.0, 5.0, 10.0, 15.0, 30.0, 45.0, 90.0, 180.0, 360.0];
        let min_deg = min.to_degrees();
        let deg = DEG
            .into_iter()
            .find(|d| *d >= min_deg)
            .unwrap_or_else(|| (min_deg / 360.0).ceil() * 360.0);
        return deg.to_radians();
    }
    if min <= 0.0 {
        return 1.0;
    }
    let decade = 10f32.powf(min.log10().floor());
    [1.0, 2.0, 5.0, 10.0]
        .into_iter()
        .map(|m| m * decade)
        .find(|s| *s >= min)
        .unwrap_or(decade * 10.0)
}

/// Every rule from `lo` to `hi` at `step` — on multiples of the step,
/// so zero is always one of them when it is in range.
pub fn rules(span: (f32, f32), step: f32) -> Vec<f32> {
    let (lo, hi) = span;
    if step <= 0.0 || hi <= lo {
        return Vec::new();
    }
    let first = (lo / step).ceil() as i64;
    let last = (hi / step).floor() as i64;
    if last < first || last - first > 400 {
        return Vec::new();
    }
    (first..=last).map(|k| k as f32 * step).collect()
}

/// Where a dragged value lands: the nearest magnet within `grab` px,
/// else the nearest rule. `px_per_unit` turns values into distance.
pub fn snap_value(v: f32, step: f32, magnets: &[f32], px_per_unit: f32, grab_px: f32) -> f32 {
    let nearest = magnets
        .iter()
        .copied()
        .filter(|m| m.is_finite())
        .min_by(|a, b| (a - v).abs().total_cmp(&(b - v).abs()));
    if let Some(m) = nearest
        && (m - v).abs() * px_per_unit <= grab_px
    {
        return m;
    }
    if step > 0.0 {
        (v / step).round() * step
    } else {
        v
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::props::Prop;

    /// The step is the first of 1·2·5 tall enough on the graph: a 4K
    /// canvas's Y over 300 px gives 200s; an opacity gives twentieths;
    /// a side count gives ones; an angle steps in round degrees.
    #[test]
    fn the_step_is_a_round_number_a_rule_apart() {
        let y = Target::Shape(Prop::Y);
        assert_eq!(value_step(y, (0.0, 3840.0), 300.0, 16.0), 500.0);
        assert_eq!(value_step(y, (0.0, 1080.0), 300.0, 16.0), 100.0);
        assert_eq!(value_step(y, (0.0, 300.0), 300.0, 16.0), 20.0);
        let o = Target::Shape(Prop::Opacity);
        assert!((value_step(o, (0.0, 1.0), 300.0, 16.0) - 0.1).abs() < 1e-6);
        assert!((value_step(o, (0.0, 1.0), 900.0, 16.0) - 0.02).abs() < 1e-6);
        let sides = Target::Shape(Prop::Sides);
        assert_eq!(value_step(sides, (3.0, 24.0), 300.0, 16.0), 2.0);
        let rot = Target::Shape(Prop::Rotation);
        let step = value_step(rot, (0.0, std::f32::consts::PI), 300.0, 16.0);
        assert!((step.to_degrees() - 10.0).abs() < 1e-4, "{}°", step.to_degrees());
        let fx = Target::Effect { id: 1, param: 0 };
        assert_eq!(value_step(fx, (0.0, 200.0), 300.0, 16.0), 20.0);
    }

    /// Rules sit on multiples of the step, so zero is always a rule
    /// when it is in range, and a span off the multiples still gets its
    /// rules inside it.
    #[test]
    fn rules_sit_on_the_multiples() {
        assert_eq!(rules((0.0, 1080.0), 200.0), vec![0.0, 200.0, 400.0, 600.0, 800.0, 1000.0]);
        assert_eq!(rules((-150.0, 150.0), 100.0), vec![-100.0, 0.0, 100.0]);
        assert_eq!(rules((3.0, 24.0), 2.0)[0], 4.0);
        assert!(rules((5.0, 5.0), 1.0).is_empty());
    }

    /// A value within reach of a magnet takes it — another key's height,
    /// a floor, zero — otherwise it rounds to the nearest rule.
    #[test]
    fn a_value_takes_a_magnet_or_a_rule() {
        // One unit per px: a 9 px grab.
        assert_eq!(snap_value(101.7, 10.0, &[], 1.0, 9.0), 100.0);
        assert_eq!(snap_value(0.2, 10.0, &[], 1.0, 9.0), 0.0);
        assert_eq!(snap_value(106.0, 10.0, &[], 1.0, 9.0), 110.0);
        // A magnet at 103 beats the rule at 100.
        assert_eq!(snap_value(101.7, 10.0, &[103.0], 1.0, 9.0), 103.0);
        // Out of reach (12 px away), the rule wins.
        assert_eq!(snap_value(101.7, 10.0, &[113.7], 1.0, 9.0), 100.0);
        // A coarse graph: 20 units per px, the same magnet is well inside.
        assert_eq!(snap_value(300.0, 500.0, &[380.0], 0.05, 9.0), 380.0);
        assert_eq!(snap_value(300.0, 500.0, &[], 0.05, 9.0), 500.0);
    }
}
