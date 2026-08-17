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
pub const PROP_ORDER: [Prop; 13] = [
    Prop::X,
    Prop::Y,
    Prop::Rotation,
    Prop::Width,
    Prop::Height,
    Prop::Scale,
    Prop::Glow,
    Prop::Brightness,
    Prop::Sides,
    Prop::Thickness,
    Prop::Density,
    Prop::Twinkle,
    Prop::TwinkleRate,
];

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
        Prop::Sides => shape.set_sides(v.round().max(3.0) as u32),
        Prop::Thickness => shape.set_thickness(v),
        Prop::Density => shape.set_density(v),
        Prop::Twinkle => shape.set_twinkle(v),
        Prop::TwinkleRate => shape.set_twinkle_rate(v),
        Prop::Seed => shape.set_seed(v),
        // React amounts live on the editor, not the shape — never curves.
        Prop::ReactScale | Prop::ReactGlow | Prop::ReactBright => {}
    }
}

/// Read one property off a shape; `None` where it doesn't apply (sides of a
/// circle, thickness of a fill).
pub fn prop_value(shape: &Shape, prop: Prop) -> Option<f32> {
    match prop {
        Prop::X => Some(shape.center()[0]),
        Prop::Y => Some(shape.center()[1]),
        Prop::Rotation => Some(shape.rotation()),
        Prop::Scale => Some(shape.size()),
        Prop::Width => shape.box_size().map(|b| b[0]),
        Prop::Height => shape.box_size().map(|b| b[1]),
        Prop::Glow => Some(shape.glow_radius()),
        Prop::Brightness => Some(shape.brightness()),
        Prop::Sides => shape.sides().map(|n| n as f32),
        Prop::Thickness => shape.thickness(),
        Prop::Density => shape.density(),
        Prop::Twinkle => shape.twinkle(),
        Prop::TwinkleRate => shape.twinkle_rate(),
        // Never stamped: a seed is which sky you got, not a value that means
        // anything halfway between two of itself.
        Prop::Seed => None,
        Prop::ReactScale | Prop::ReactGlow | Prop::ReactBright => None,
    }
}

/// Bit for `prop` in a keyed-property mask (inspector gold values).
pub fn prop_bit(prop: Prop) -> u16 {
    1 << PROP_ORDER.iter().position(|p| *p == prop).unwrap_or(15)
}

// --- serialization tags (the `anim` lines of the .spark format) ---

pub fn prop_tag(prop: Prop) -> &'static str {
    match prop {
        Prop::X => "x",
        Prop::Y => "y",
        Prop::Rotation => "rot",
        Prop::Scale => "scale",
        Prop::Width => "w",
        Prop::Height => "h",
        Prop::Glow => "glow",
        Prop::Brightness => "bright",
        Prop::Sides => "sides",
        Prop::Thickness => "thick",
        Prop::Density => "density",
        Prop::Twinkle => "twinkle",
        Prop::TwinkleRate => "twinkrate",
        // Present for exhaustiveness; the seed rides the shape's own line.
        Prop::Seed => "seed",
        // Present for exhaustiveness; react amounts serialize on their own
        // `react` line, never as curves.
        Prop::ReactScale => "react-scale",
        Prop::ReactGlow => "react-glow",
        Prop::ReactBright => "react-bright",
    }
}

pub fn parse_prop(tag: &str) -> Option<Prop> {
    PROP_ORDER.into_iter().find(|p| prop_tag(*p) == tag)
}
