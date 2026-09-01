//! The editor's vocabulary: tools, editable properties, the neon palette,
//! and the shape factory. Split from `editor` so the interaction state
//! machine stays readable.

use spark_render::{Shape, ShapeKind};
use spark_ui::{
    ICON_CIRCLE, ICON_CUBE, ICON_LINE, ICON_PATH, ICON_PENTAGON, ICON_SQUARE, ICON_STARS, ICON_SUN,
};

pub const PALETTE: [[f32; 3]; 7] = [
    [1.00, 0.16, 0.85], // magenta
    [0.16, 0.75, 1.00], // cyan
    [0.55, 0.25, 1.00], // violet
    [1.00, 0.45, 0.10], // ember
    [0.10, 1.00, 0.55], // acid
    [1.00, 0.95, 0.30], // laser
    [1.00, 0.10, 0.12], // red
];
pub(crate) const PALETTE_NAMES: [&str; 7] =
    ["magenta", "cyan", "violet", "ember", "acid", "laser", "red"];

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Tool {
    Select,
    Circle,
    Box,
    Polygon,
    Line,
    /// Drag a region; it fills with scattered stars.
    Stars,
}

/// A shape kind's icon glyph and auto-name — what a layer with no
/// user-given name is called, and the glyph any future object list draws.
/// One definition, so the keyframe lane and the status strip can't
/// disagree about what a shape is called.
pub(crate) fn kind_parts(kind: ShapeKind) -> (f32, &'static str) {
    match kind {
        ShapeKind::Circle => (ICON_CIRCLE, "circle"),
        ShapeKind::Box => (ICON_SQUARE, "box"),
        ShapeKind::Ngon => (ICON_PENTAGON, "polygon"),
        ShapeKind::Line => (ICON_LINE, "line"),
        ShapeKind::Path => (ICON_PATH, "path"),
        ShapeKind::Stars => (ICON_STARS, "stars"),
        ShapeKind::Mesh => (ICON_CUBE, "mesh"),
        ShapeKind::Light => (ICON_SUN, "light"),
    }
}

/// An animatable/editable property of the selected shape. The React trio
/// are audio-reaction amounts (bass→scale, bass→glow, mid/onset→bright):
/// inspector-editable and saved, but never keyframed. Neither is `Seed`,
/// which picks *which* scatter a star field is — interpolating between two
/// skies is a re-roll every frame, not an animation.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Prop {
    X,
    Y,
    /// Depth: how far back from the canvas the shape's plane sits.
    Z,
    Rotation,
    /// The plane's rotation about its horizontal axis. Counts turns, like
    /// Rotation.
    Tilt,
    /// The plane's rotation about its vertical axis. Counts turns.
    Turn,
    Scale,
    Width,
    Height,
    Glow,
    Brightness,
    /// How much of the shape reaches the frame. Unlike brightness, which is
    /// how hard it burns, this is whether it is *there* — and it is on the
    /// shape rather than on the effects stack because fading in and out is
    /// not a look you add, it is the most ordinary thing an animation does.
    Opacity,
    Sides,
    Thickness,
    /// A spot light's cone half-angle, degrees.
    Cone,
    /// An ambient light's rim strength, 0 to 1.
    Rim,
    /// A mesh's third side, canvas units.
    Depth,
    /// Star field: cells across the longer axis, one star each.
    Density,
    /// Star field: how hard the stars pulse.
    Twinkle,
    /// Star field: how fast they pulse, radians per second.
    TwinkleRate,
    /// Star field: which scatter the sky is. Only the old card's re-roll
    /// button ever constructed it; the prop table still knows it.
    #[allow(dead_code)]
    Seed,
    /// Audio-reaction amounts. Constructed by the inspector when it
    /// lands; the react array they describe never left the document.
    #[allow(dead_code)]
    ReactScale,
    #[allow(dead_code)]
    ReactGlow,
    #[allow(dead_code)]
    ReactBright,
}

/// Style settings carried by Ctrl+C / Ctrl+V between shapes — the look,
/// never the geometry.
#[derive(Clone)]
pub struct StyleClip {
    pub rgb: [f32; 3],
    pub intensity: f32,
    pub glow: f32,
    pub thickness: Option<f32>,
    pub outline: Option<bool>,
    pub additive: bool,
    /// Gradient fill: on/off and the end color.
    pub gradient: bool,
    pub rgb2: [f32; 3],
    /// A star field's look. `None` off a field, and the setters are no-ops
    /// off one too, so a field's style pastes harmlessly onto a circle and
    /// a circle's leaves a field's stars alone.
    pub density: Option<f32>,
    pub twinkle: Option<f32>,
    pub twinkle_rate: Option<f32>,
    pub star_form: Option<usize>,
}

/// Snapshot of the primary selection, for handle drags and whatever
/// inspector the redesign grows.
#[allow(dead_code)] // some fields' readers left with the old panels
pub struct Props {
    pub x: f32,
    pub y: f32,
    pub rotation: f32,
    /// What the card's S says — see [`extent`].
    pub size: f32,
    /// Width, height and depth, full, where the shape has them.
    pub w: Option<f32>,
    pub h: Option<f32>,
    pub d: Option<f32>,
    pub z: f32,
    pub tilt: f32,
    pub turn: f32,
    /// The shape's color (linear).
    pub rgb: [f32; 3],
    /// The gradient's end color (linear).
    pub rgb2: [f32; 3],
}

/// What the card's **S** field says: the shape's full size — its longer
/// side, a circle's diameter — or, for a light, its range. Inside, a
/// shape keeps half extents (`Shape::size` is a radius), which is what
/// made S read as nonsense beside Width and Height: a plane at S 900
/// was 1800 wide (Alva, 2026-08-31: "Scale makes no sense to me
/// whatsoever"). The card speaks full sizes now; `set_prop(Scale)` takes
/// one back.
pub fn extent(s: &spark_render::Shape) -> f32 {
    if s.is_light() { s.size() } else { s.size() * 2.0 }
}

/// Slider/scrub range per property. `canvas` is the comp's size: the
/// place and side ranges run across it.
pub fn range(prop: Prop, canvas: [f32; 2]) -> (f32, f32) {
    let [cw, ch] = canvas;
    match prop {
        Prop::X => (0.0, cw),
        Prop::Y => (0.0, ch),
        // Never clamped — see `fit`. Here for the slider maths only.
        Prop::Rotation | Prop::Tilt | Prop::Turn => {
            (-std::f32::consts::PI, std::f32::consts::PI)
        }
        // Toward the camera for positive. It sits about 1480 units in
        // front of the canvas; nearer than this and the plane is a blur
        // across the lens.
        Prop::Z => (-12000.0, 1400.0),
        Prop::Scale => (3.0, 4000.0),
        Prop::Width => (6.0, cw),
        Prop::Height => (6.0, ch),
        // Glow starts at nothing. Brightness stops at 3 rather than 5: 1.0 is
        // now exactly the colour you picked, so everything above it is
        // overdrive, and a slider whose useful half is its first fifth is a
        // slider you can't aim.
        Prop::Glow => (0.0, 200.0),
        Prop::Brightness => (0.05, 3.0),
        // All the way to nothing: an effect that can't reach zero is a
        // fade-out you can only ever nearly do.
        Prop::Opacity => (0.0, 1.0),
        Prop::Sides => (3.0, 12.0),
        Prop::Thickness => (1.0, 30.0),
        Prop::Cone => (2.0, 120.0),
        Prop::Rim => (0.0, 1.0),
        Prop::Depth => (1.0, ch),
        Prop::Density => (2.0, 120.0),
        Prop::Twinkle => (0.0, 1.0),
        Prop::TwinkleRate => (0.0, 12.0),
        Prop::Seed => (0.0, 100.0),
        Prop::ReactScale | Prop::ReactGlow | Prop::ReactBright => (0.0, 2.0),
    }
}

/// Map a normalized slider position back to a property value.
#[allow(dead_code)] // kept for the redesign; the clip view / inspector re-consume it
pub fn value_for(prop: Prop, t: f32, canvas: [f32; 2]) -> f32 {
    let (min, max) = range(prop, canvas);
    min + t.clamp(0.0, 1.0) * (max - min)
}

/// Fit a hand-entered value into its property's range.
///
/// Rotation is exempt: it is not an angle in (-π, π], it is *how far the
/// shape has turned*, and it has to keep counting. Folding it made a
/// continuous spin impossible — stamp 0°, stamp 180°, then keep turning and
/// the third key came back as -170° instead of 190°, so the shape unwound
/// counter-clockwise to get there. Two full turns is 720 and means it.
///
/// Sizes have a floor and no ceiling: the range's top is where the slider
/// ends, not where a floor plane has to stop (Alva's room, 2026-08-31:
/// "it only scales to barely bigger than it was and it stops").
pub fn fit(prop: Prop, v: f32, canvas: [f32; 2]) -> f32 {
    if matches!(prop, Prop::Rotation | Prop::Tilt | Prop::Turn) {
        return v;
    }
    let (min, max) = range(prop, canvas);
    if matches!(prop, Prop::Scale | Prop::Width | Prop::Height | Prop::Depth) {
        return v.max(min);
    }
    v.clamp(min, max)
}

/// Where a stack index lands after `remove(from)` + `insert(to, _)`.
#[allow(dead_code)] // kept for the redesign; the old panels were the only caller
pub(crate) fn remap(s: usize, from: usize, to: usize) -> usize {
    if s == from {
        to
    } else if from < to && s > from && s <= to {
        s - 1
    } else if to < from && s >= to && s < from {
        s + 1
    } else {
        s
    }
}

pub(crate) fn dist(a: [f32; 2], b: [f32; 2]) -> f32 {
    ((b[0] - a[0]).powi(2) + (b[1] - a[1]).powi(2)).sqrt()
}

pub(crate) fn draw_shape(
    tool: Tool,
    press: [f32; 2],
    cursor: [f32; 2],
    sides: u32,
    rgb: [f32; 3],
) -> Shape {
    let d = dist(press, cursor).max(3.0);
    let half = [
        (cursor[0] - press[0]).abs().max(3.0),
        (cursor[1] - press[1]).abs().max(3.0),
    ];
    // A star field carries its own glow and star size, tuned so the first
    // drag already looks like a sky — only the color comes from the tool.
    if tool == Tool::Stars {
        return Shape::stars(press, half, seed_at(press))
            .color(rgb[0], rgb[1], rgb[2])
            .intensity(1.4);
    }
    let shape = match tool {
        Tool::Circle => Shape::circle(press, d).stroke(4.0),
        Tool::Box => Shape::rect(press, half).stroke(4.0),
        Tool::Polygon => Shape::ngon(press, d, sides).stroke(4.0),
        Tool::Line => Shape::line(press, cursor, 3.0),
        Tool::Select | Tool::Stars => unreachable!("handled above"),
    };
    // Plain by default: the colour you picked, at the brightness you picked,
    // with no halo. Glow is a thing you turn on (the Glow slider, or `A` /
    // `Z` on the keyboard) rather than a thing you spend the session turning
    // off.
    shape.color(rgb[0], rgb[1], rgb[2]).intensity(1.0).glow(0.0)
}

/// A star field's seed, from where it was drawn: two fields dragged in
/// different places get different skies, and re-dragging the same one keeps
/// its own while you size it.
pub(crate) fn seed_at(p: [f32; 2]) -> f32 {
    (p[0] * 7.13 + p[1] * 3.77).abs() % 100.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::{FRAC_PI_2, PI, TAU};

    /// Rotation counts turns; it does not fold. Folding it is what made a
    /// continuous spin impossible — the key past half a turn came back
    /// negative and the shape unwound to reach it.
    #[test]
    fn rotation_keeps_counting_past_a_full_turn() {
        let c = spark_render::CANVAS;
        for turns in [0.5f32, 1.0, 2.0, 5.0, -3.0] {
            let a = TAU * turns;
            assert!(
                (fit(Prop::Rotation, a, c) - a).abs() < 1e-4,
                "{turns} turns came back as {}",
                fit(Prop::Rotation, a, c) / TAU
            );
        }
        // And it still passes small angles through untouched.
        assert!((fit(Prop::Rotation, FRAC_PI_2, c) - FRAC_PI_2).abs() < 1e-6);
        assert!((fit(Prop::Rotation, PI + 0.1, c) - (PI + 0.1)).abs() < 1e-6);
    }

    /// Drawing a field is a drag over a region, like a box — and it must
    /// come out as a field, not as whatever the last kind added was.
    #[test]
    fn the_stars_tool_draws_a_field() {
        let s = draw_shape(Tool::Stars, [100.0, 100.0], [220.0, 180.0], 5, [1.0; 3]);
        assert_eq!(s.kind(), spark_render::ShapeKind::Stars);
        assert_eq!(s.center(), [100.0, 100.0]);
        assert_eq!(s.box_size(), Some([240.0, 160.0]), "the dragged region");
        assert!(s.density().is_some() && s.twinkle().is_some());
        // Two fields drawn in different places are different skies.
        let other = draw_shape(Tool::Stars, [700.0, 400.0], [820.0, 480.0], 5, [1.0; 3]);
        assert_ne!(s.seed(), other.seed());
    }

    #[test]
    fn other_props_still_clamp() {
        let c = spark_render::CANVAS;
        let (min, max) = range(Prop::Glow, c);
        assert_eq!(fit(Prop::Glow, max + 500.0, c), max);
        assert_eq!(fit(Prop::Glow, min - 500.0, c), min);
    }

    /// The place ranges are the comp's: on a portrait canvas Y runs to
    /// 1920 and X stops at 1080, so a typed 1500 down the phone's screen
    /// is a place on it rather than clamped to a landscape's bottom edge.
    #[test]
    fn place_ranges_follow_the_canvas() {
        let tall = [1080.0, 1920.0];
        assert_eq!(range(Prop::X, tall), (0.0, 1080.0));
        assert_eq!(range(Prop::Y, tall), (0.0, 1920.0));
        assert_eq!(fit(Prop::Y, 1500.0, tall), 1500.0);
        assert_eq!(fit(Prop::Y, 1500.0, spark_render::CANVAS), 1080.0);
        assert_eq!(value_for(Prop::Y, 0.5, tall), 960.0);
    }
}
