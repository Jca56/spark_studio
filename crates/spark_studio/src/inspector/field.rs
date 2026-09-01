//! Scrub fields: the inspector's control for a number with no ceiling —
//! a position, an angle, a size. Drag up to raise it, down to lower it,
//! Shift for a tenth; a clean click (no travel) opens it for typing,
//! Enter commits, Esc lets go. Angles are shown and typed in degrees and
//! stored in radians, counting turns: 720 means two of them.

use crate::props::Prop;

/// Canvas units (or degrees) per logical pixel of drag.
pub fn scrub_step(prop: Prop) -> f32 {
    match prop {
        Prop::Rotation | Prop::Tilt | Prop::Turn => 0.5,
        _ => 1.0,
    }
}

/// Whether a field shows its number in degrees.
pub fn is_angle(prop: Prop) -> bool {
    matches!(prop, Prop::Rotation | Prop::Tilt | Prop::Turn)
}

/// The number a field shows for a stored value.
pub fn shown(prop: Prop, stored: f32) -> f32 {
    if is_angle(prop) {
        stored.to_degrees()
    } else {
        stored
    }
}

/// The stored value for a number a field shows or was typed.
pub fn stored(prop: Prop, shown: f32) -> f32 {
    if is_angle(prop) {
        shown.to_radians()
    } else {
        shown
    }
}

/// Where a scrub has dragged to, in the field's shown units: `dy` is
/// physical px down from the press (up raises), `fine` a tenth.
pub fn scrubbed(prop: Prop, start: f32, dy: f32, scale: f32, fine: bool) -> f32 {
    let k = if fine { 0.1 } else { 1.0 };
    start - dy / scale * scrub_step(prop) * k
}

/// How a field prints its number: whole when it is, one decimal when it
/// isn't — a fine scrub has to show it moved.
pub fn format(v: f32) -> String {
    if (v - v.round()).abs() < 0.05 {
        format!("{:.0}", v.round())
    } else {
        format!("{v:.1}")
    }
}

/// What a typed field means, if it means a number.
pub fn parse(text: &str) -> Option<f32> {
    let t = text.trim();
    if t.is_empty() {
        return None;
    }
    t.parse::<f32>().ok().filter(|v| v.is_finite())
}

/// The fields the transform strip shows, in rows of three: place, aim,
/// size — and a mesh's depth on its own. A prop the primary lacks
/// (a circle's width, a light's spin) is skipped by the page.
pub const ROWS: [&[(Prop, &str)]; 4] = [
    &[(Prop::X, "X"), (Prop::Y, "Y"), (Prop::Z, "Z")],
    &[(Prop::Rotation, "Rot"), (Prop::Tilt, "Tilt"), (Prop::Turn, "Turn")],
    &[(Prop::Scale, "S"), (Prop::Width, "W"), (Prop::Height, "H")],
    &[(Prop::Depth, "D")],
];

#[cfg(test)]
mod tests {
    use super::*;

    /// A scrub moves the number by the pixels travelled, up is up, Shift
    /// is a tenth, and an angle scrubs in half-degrees.
    #[test]
    fn a_scrub_moves_by_the_pixels() {
        assert_eq!(scrubbed(Prop::X, 100.0, -50.0, 1.0, false), 150.0);
        assert_eq!(scrubbed(Prop::X, 100.0, 50.0, 1.0, false), 50.0);
        assert!((scrubbed(Prop::X, 100.0, -50.0, 1.0, true) - 105.0).abs() < 1e-4);
        // Physical px are divided by the scale, so the feel is the same
        // on the 4K at 1.4 as on the secondary at 1.0.
        assert!((scrubbed(Prop::Y, 0.0, -140.0, 1.4, false) - 100.0).abs() < 1e-3);
        assert!((scrubbed(Prop::Rotation, 0.0, -100.0, 1.0, false) - 50.0).abs() < 1e-4);
    }

    /// Angles are shown in degrees and stored in radians, and keep
    /// counting past a turn either way.
    #[test]
    fn angles_round_trip_through_degrees() {
        let two_turns = std::f32::consts::TAU * 2.0;
        assert!((shown(Prop::Rotation, two_turns) - 720.0).abs() < 1e-3);
        assert!((stored(Prop::Tilt, 720.0) - two_turns).abs() < 1e-5);
        assert_eq!(shown(Prop::X, 12.5), 12.5);
        assert_eq!(stored(Prop::Scale, 12.5), 12.5);
    }

    /// Whole numbers print whole, others to a decimal; typing takes a
    /// number and nothing else.
    #[test]
    fn numbers_print_and_parse() {
        assert_eq!(format(960.0), "960");
        assert_eq!(format(960.02), "960");
        assert_eq!(format(12.5), "12.5");
        assert_eq!(format(-0.4), "-0.4");
        assert_eq!(parse(" 42 "), Some(42.0));
        assert_eq!(parse("-12.5"), Some(-12.5));
        assert_eq!(parse(""), None);
        assert_eq!(parse("abc"), None);
        assert_eq!(parse("inf"), None);
    }
}
