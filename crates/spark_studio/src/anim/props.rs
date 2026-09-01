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
pub const PROP_ORDER: [Prop; 19] = [
    Prop::X,
    Prop::Y,
    Prop::Z,
    Prop::Rotation,
    Prop::Tilt,
    Prop::Turn,
    Prop::Width,
    Prop::Height,
    Prop::Scale,
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
    }
}

/// Read one property off a shape; `None` where it doesn't apply (sides of a
/// circle, thickness of a fill).
pub fn prop_value(shape: &Shape, prop: Prop) -> Option<f32> {
    match prop {
        Prop::X => Some(shape.center()[0]),
        Prop::Y => Some(shape.center()[1]),
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
    }
}

pub fn parse_prop(tag: &str) -> Option<Prop> {
    PROP_ORDER.into_iter().find(|p| prop_tag(*p) == tag)
}
