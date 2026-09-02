//! The property table: which properties animate, in what order they are
//! applied, how a value is read off a shape and written back, and the tags
//! they serialize under.
//!
//! Reading and writing are deliberately a matched pair — [`prop_value`]
//! returns `None` exactly where [`apply_prop`] would be a no-op (the sides
//! of a circle, the thickness of a fill), so a stamp never records a value
//! that could not be played back.

use spark_render::Shape;

use crate::props::Prop;

/// Evaluation order: geometry before uniform scale, look last. Width/Height
/// set absolute extents, then Scale multiplies both axes — so a box keyed on
/// all three lands at Scale's size with W/H's aspect, deterministically.
///
/// Glow is deliberately absent: it lives on the Glow *effect* now, and a
/// value can only have one owner. Keeping it here too would mean the curve
/// wrote `shape.glow` and then the effect resolver overwrote it a moment
/// later — the keyframe would silently do nothing.
pub const PROP_ORDER: [Prop; 30] = [
    Prop::X,
    Prop::Y,
    Prop::Z,
    Prop::Rotation,
    Prop::Tilt,
    Prop::Turn,
    Prop::Width,
    Prop::Height,
    Prop::Scale,
    // After the centre props: a line keys by its ends (see [`keyable`]),
    // and a clip that keyed X·Y·Rot·S before there were ends still
    // plays — the ends land last and say where the line is.
    Prop::X1,
    Prop::Y1,
    Prop::X2,
    Prop::Y2,
    Prop::Brightness,
    Prop::Opacity,
    Prop::Sides,
    Prop::Thickness,
    Prop::Cone,
    Prop::Density,
    Prop::Twinkle,
    Prop::TwinkleRate,
    // Last, so the keyed-bit mask of everything before it holds.
    Prop::Rim,
    Prop::Depth,
    Prop::Jag,
    Prop::Branches,
    Prop::Strike,
    Prop::Hole,
    Prop::Twist,
    Prop::Spin,
    Prop::Grain,
];

/// What the *first* stamp on a shape keys: where it is, how it's turned,
/// how big it is. A shape has to have a pose before it can have a change,
/// and this is the smallest set that counts as one.
///
/// Width and Height stay out on purpose — they arrive the moment they're
/// actually stretched. `Scale` reads `b[0].max(b[1])`, the same extents
/// Width and Height write, so stretching the longer axis moves Scale too
/// and [`changed`] catches both; stretching the shorter one leaves Scale
/// genuinely unmoved, and a flat Scale curve re-applies as a no-op.
pub const FIRST_POSE: [Prop; 4] = [Prop::X, Prop::Y, Prop::Rotation, Prop::Scale];

/// A line's first pose: its two ends, which is the whole of where it is.
pub const LINE_FIRST_POSE: [Prop; 4] = [Prop::X1, Prop::Y1, Prop::X2, Prop::Y2];

/// What a stamp lays down on a shape whose clip has no keys yet — the
/// pose for its kind, less anything it can't key.
pub fn first_pose(shape: &Shape) -> Vec<Prop> {
    let pose: &[Prop] = if shape.is_line() {
        &LINE_FIRST_POSE
    } else {
        &FIRST_POSE
    };
    pose.iter().copied().filter(|&p| keyable(shape, p)).collect()
}

/// Whether `prop` is a setting this shape can key — what the stamp
/// diffs, what the clip view lists, what an armed setting must be.
///
/// A value has one owner on a curve, and a line's place has two
/// descriptions: its centre, angle and length, or its two ends. Keying
/// both would have a stamp lay X·Y·Rot·S *and* X1·Y1·X2·Y2 for one
/// drag, and the two curves fight from then on. **A line keys by its
/// ends**: X·Y·Rot·S still edit it — moving the whole line moves both
/// ends, and that is what the stamp records. A light is aimed, not
/// spun, so it has no Rot to key either (the inspector's rule, held
/// here so the stamp agrees with it).
pub fn keyable(shape: &Shape, prop: Prop) -> bool {
    if prop_value(shape, prop).is_none() {
        return false;
    }
    if shape.is_line() {
        return !matches!(prop, Prop::X | Prop::Y | Prop::Rotation | Prop::Scale);
    }
    !(shape.is_light() && prop == Prop::Rotation)
}

/// The canvas axis a place property moves along — 0 for the X's, 1
/// for the Y's — so a copy's curves can be carried beside the original
/// (a duplicate's nudge, a paste's offset) whatever describes its
/// place: a centre or a line's ends. `None` for everything else.
pub fn place_axis(prop: Prop) -> Option<usize> {
    match prop {
        Prop::X | Prop::X1 | Prop::X2 => Some(0),
        Prop::Y | Prop::Y1 | Prop::Y2 => Some(1),
        _ => None,
    }
}

/// Whether two values of `prop` are far enough apart to be a hand edit
/// rather than float noise — the question `stamp_key` asks of every
/// property to decide which ones it is actually keying.
///
/// The tolerance scales with the property's own range, because these live
/// on wildly different scales: a tenth of a canvas pixel is nothing for X
/// and the same absolute number is a tenth of Twinkle's entire span. The
/// default canvas sets the scale for the place and side properties — a
/// tolerance, not a range, so the comp's actual size needn't be known.
pub fn changed(prop: Prop, a: f32, b: f32) -> bool {
    let (lo, hi) = crate::props::range(prop, spark_render::CANVAS);
    (a - b).abs() > (hi - lo).abs() * 1e-4
}

/// Write one property value absolutely (curves never accumulate — every
/// setter here lands on the target regardless of the shape's current state).
pub fn apply_prop(shape: &mut Shape, prop: Prop, v: f32) {
    match prop {
        Prop::X => {
            let c = shape.center();
            shape.set_center([v, c[1]]);
        }
        Prop::Y => {
            let c = shape.center();
            shape.set_center([c[0], v]);
        }
        Prop::X1 => {
            let (a, _) = shape.line_ends();
            shape.set_line_start([v, a[1]]);
        }
        Prop::Y1 => {
            let (a, _) = shape.line_ends();
            shape.set_line_start([a[0], v]);
        }
        Prop::X2 => {
            let (_, b) = shape.line_ends();
            shape.set_line_end([v, b[1]]);
        }
        Prop::Y2 => {
            let (_, b) = shape.line_ends();
            shape.set_line_end([b[0], v]);
        }
        Prop::Rotation => shape.set_rotation(v),
        Prop::Z => shape.set_z(v),
        Prop::Tilt => shape.set_tilt(v),
        Prop::Turn => shape.set_turn(v),
        Prop::Scale => {
            let cur = shape.size();
            if cur > 0.001 {
                shape.scale_by(v / cur);
            }
        }
        Prop::Width => shape.set_box_width(v),
        Prop::Height => shape.set_box_height(v),
        Prop::Glow => shape.set_glow(v),
        Prop::Brightness => shape.set_brightness(v),
        Prop::Opacity => shape.set_opacity(v),
        Prop::Sides => shape.set_sides(v.round().max(3.0) as u32),
        Prop::Thickness => shape.set_thickness(v),
        Prop::Cone => shape.set_cone(v),
        Prop::Rim => shape.set_rim(v),
        Prop::Depth => shape.set_depth(v),
        Prop::Density => shape.set_density(v),
        Prop::Twinkle => shape.set_twinkle(v),
        Prop::TwinkleRate => shape.set_twinkle_rate(v),
        Prop::Seed => shape.set_seed(v),
        Prop::Jag => shape.set_jag(v),
        Prop::Branches => shape.set_branches(v),
        Prop::Strike => shape.set_strike_rate(v),
        Prop::Hole => shape.set_hole(v),
        Prop::Twist => shape.set_twist(v),
        Prop::Spin => shape.set_spin(v),
        Prop::Grain => shape.set_grain(v),
    }
}

/// Read one property off a shape; `None` where it doesn't apply (sides of a
/// circle, thickness of a fill).
pub fn prop_value(shape: &Shape, prop: Prop) -> Option<f32> {
    match prop {
        Prop::X => Some(shape.center()[0]),
        Prop::Y => Some(shape.center()[1]),
        Prop::X1 => shape.is_line().then(|| shape.line_ends().0[0]),
        Prop::Y1 => shape.is_line().then(|| shape.line_ends().0[1]),
        Prop::X2 => shape.is_line().then(|| shape.line_ends().1[0]),
        Prop::Y2 => shape.is_line().then(|| shape.line_ends().1[1]),
        Prop::Rotation => Some(shape.rotation()),
        Prop::Z => Some(shape.z()),
        Prop::Tilt => Some(shape.tilt()),
        Prop::Turn => Some(shape.turn()),
        Prop::Scale => Some(shape.size()),
        Prop::Width => shape.box_size().map(|b| b[0]),
        Prop::Height => shape.box_size().map(|b| b[1]),
        Prop::Glow => Some(shape.glow_radius()),
        Prop::Brightness => Some(shape.brightness()),
        Prop::Opacity => Some(shape.opacity()),
        Prop::Sides => shape.sides().map(|n| n as f32),
        Prop::Thickness => shape.thickness(),
        Prop::Cone => shape.cone(),
        Prop::Rim => shape.rim(),
        Prop::Depth => shape.depth(),
        Prop::Density => shape.density(),
        Prop::Twinkle => shape.twinkle(),
        Prop::TwinkleRate => shape.twinkle_rate(),
        // Never stamped: a seed is which sky you got, not a value that means
        // anything halfway between two of itself.
        Prop::Seed => None,
        Prop::Jag => shape.jag(),
        Prop::Branches => shape.branches(),
        Prop::Strike => shape.strike_rate(),
        Prop::Hole => shape.hole(),
        Prop::Twist => shape.twist(),
        Prop::Spin => shape.spin(),
        Prop::Grain => shape.grain(),
    }
}

/// Bit for `prop` in a keyed-property mask (inspector gold values).
#[allow(dead_code)] // kept for the redesign; the old panels were the only caller
pub fn prop_bit(prop: Prop) -> u32 {
    1 << PROP_ORDER.iter().position(|p| *p == prop).unwrap_or(31)
}

// --- serialization tags (the `anim` lines of the .spark format) ---

pub fn prop_tag(prop: Prop) -> &'static str {
    match prop {
        Prop::X => "x",
        Prop::Y => "y",
        Prop::X1 => "x1",
        Prop::Y1 => "y1",
        Prop::X2 => "x2",
        Prop::Y2 => "y2",
        Prop::Rotation => "rot",
        Prop::Z => "z",
        Prop::Tilt => "tilt",
        Prop::Turn => "turn",
        Prop::Scale => "scale",
        Prop::Width => "w",
        Prop::Height => "h",
        Prop::Glow => "glow",
        Prop::Brightness => "bright",
        Prop::Opacity => "opacity",
        Prop::Sides => "sides",
        Prop::Thickness => "thick",
        Prop::Cone => "cone",
        Prop::Rim => "rim",
        Prop::Depth => "depth",
        Prop::Density => "density",
        Prop::Twinkle => "twinkle",
        Prop::TwinkleRate => "twinkrate",
        // Present for exhaustiveness; the seed rides the shape's own line.
        Prop::Seed => "seed",
        Prop::Jag => "jag",
        Prop::Branches => "forks",
        Prop::Strike => "strike",
        Prop::Hole => "hole",
        Prop::Twist => "twist",
        Prop::Spin => "spin",
        Prop::Grain => "grain",
    }
}

pub fn parse_prop(tag: &str) -> Option<Prop> {
    PROP_ORDER.into_iter().find(|p| prop_tag(*p) == tag)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A line reads and writes its ends one coordinate at a time, and
    /// writing one leaves the other three alone — which is what lets one
    /// end hold while the other swings. Nothing else has ends.
    #[test]
    fn a_line_reads_and_writes_its_ends() {
        let mut line = Shape::line([100.0, 200.0], [300.0, 400.0], 3.0);
        assert_eq!(prop_value(&line, Prop::X1), Some(100.0));
        assert_eq!(prop_value(&line, Prop::Y1), Some(200.0));
        assert_eq!(prop_value(&line, Prop::X2), Some(300.0));
        assert_eq!(prop_value(&line, Prop::Y2), Some(400.0));
        apply_prop(&mut line, Prop::Y2, 50.0);
        assert_eq!(line.line_ends(), ([100.0, 200.0], [300.0, 50.0]));
        apply_prop(&mut line, Prop::X1, 0.0);
        assert_eq!(line.line_ends(), ([0.0, 200.0], [300.0, 50.0]));
        let mut circle = Shape::circle([100.0, 100.0], 40.0);
        for p in LINE_FIRST_POSE {
            assert_eq!(prop_value(&circle, p), None, "{p:?} on a circle");
        }
        apply_prop(&mut circle, Prop::X2, 900.0);
        assert_eq!(circle.center(), [100.0, 100.0], "a circle has no end to move");
        // The pair holds across the whole table: a stamp never records a
        // value it could not play back.
        for p in PROP_ORDER {
            assert_eq!(parse_prop(prop_tag(p)), Some(p), "{p:?} tag round trip");
        }
        // An end is a place, so a copy's curves carry it along too.
        assert_eq!(place_axis(Prop::X2), Some(0));
        assert_eq!(place_axis(Prop::Y1), Some(1));
        assert_eq!(place_axis(Prop::Rotation), None);
    }

    /// What a shape can key: a line its ends and never its centre, a
    /// circle its centre and never an end, a light no spin — and the
    /// first pose follows.
    #[test]
    fn a_line_keys_by_its_ends() {
        let line = Shape::line([0.0, 0.0], [100.0, 0.0], 3.0);
        for p in [Prop::X, Prop::Y, Prop::Rotation, Prop::Scale] {
            assert!(!keyable(&line, p), "{p:?} keyable on a line");
        }
        for p in [Prop::X1, Prop::Y1, Prop::X2, Prop::Y2, Prop::Z, Prop::Opacity] {
            assert!(keyable(&line, p), "{p:?} not keyable on a line");
        }
        assert_eq!(first_pose(&line), LINE_FIRST_POSE.to_vec());
        let circle = Shape::circle([0.0, 0.0], 40.0);
        assert!(keyable(&circle, Prop::X) && !keyable(&circle, Prop::X1));
        assert_eq!(first_pose(&circle), FIRST_POSE.to_vec());
        let light = Shape::light([0.0, 0.0], spark_render::LightKind::Sun);
        assert!(!keyable(&light, Prop::Rotation), "a light is aimed, not spun");
        assert!(keyable(&light, Prop::X));
    }
}
