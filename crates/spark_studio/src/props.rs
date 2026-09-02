//! The editor's vocabulary: tools, editable properties, the neon palette,
//! and the shape factory. Split from `editor` so the interaction state
//! machine stays readable.

use spark_render::{Shape, ShapeKind};
use spark_ui::{
    ICON_BOLT, ICON_CAMERA, ICON_CIRCLE, ICON_CUBE, ICON_LINE, ICON_PATH, ICON_PENTAGON, ICON_SQUARE,
    ICON_STARS, ICON_SUN, ICON_VORTEX,
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

/// The inspector's swatch grid — Lantern Studio's layout (Alva,
/// 2026-08-31): eight hues across, light / full / dark down, and a row of
/// neutrals ending in Spark's two accents. The full row is the neon seven
/// with a blue between cyan and violet, so `C` still cycles colours that
/// are on the grid. Display-space codes; converted once.
pub const SWATCH_COLS: usize = 8;
pub const SWATCH_ROWS: usize = 4;
const SWATCH_HEX: [u32; SWATCH_COLS * SWATCH_ROWS] = [
    0xFF8A8A, 0xFFB380, 0xFFF59A, 0x9AFF9A, 0x8AF5FF, 0x8AB8FF, 0xC48AFF, 0xFF8AD6, //
    0xFF1A1F, 0xFF731A, 0xFFF24D, 0x1AFF8C, 0x29BFFF, 0x2255FF, 0x8C40FF, 0xFF29D9, //
    0x8A0F12, 0x8A3A08, 0x8A8000, 0x0A6B2E, 0x0A5C66, 0x0A2E8A, 0x4A0A8A, 0x8A0A5A, //
    0xFFFFFF, 0xC8C8C8, 0x888888, 0x505050, 0x2A2A2A, 0x000000, 0xFFC800, 0xC94DF0,
];

/// Spark's gold — the accent, and the colour a fresh session draws with
/// (Alva's call, 2026-08-31). On the grid, last row.
pub fn gold() -> [f32; 3] {
    let c = spark_ui::srgb(0xFFC800);
    [c[0], c[1], c[2]]
}

/// The grid as linear RGB, row-major.
pub fn swatch_grid() -> &'static [[f32; 3]; SWATCH_COLS * SWATCH_ROWS] {
    static GRID: std::sync::OnceLock<[[f32; 3]; SWATCH_COLS * SWATCH_ROWS]> =
        std::sync::OnceLock::new();
    GRID.get_or_init(|| {
        SWATCH_HEX.map(|h| {
            let c = spark_ui::srgb(h);
            [c[0], c[1], c[2]]
        })
    })
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Tool {
    Select,
    Circle,
    Box,
    Polygon,
    Line,
    /// Drag a region; it fills with scattered stars.
    Stars,
    /// Drag from A to B; lightning crackles between them.
    Bolt,
    /// Drag a region; an accretion disk swirls around a void in it.
    Vortex,
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
        ShapeKind::Bolt => (ICON_BOLT, "lightning"),
        ShapeKind::Vortex => (ICON_VORTEX, "vortex"),
        ShapeKind::Camera => (ICON_CAMERA, "camera"),
    }
}

/// An animatable/editable property of the selected shape. Audio reaction
/// is not one — it is the React *effect*, whose amounts are effect
/// parameters. `Seed` is never keyframed either: it picks *which* scatter
/// a star field is, and interpolating between two skies is a re-roll every
/// frame, not an animation.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Prop {
    X,
    Y,
    /// A line's ends: where it starts and where it stops. A line *is*
    /// its two ends — X·Y·Rot·S are read off them — so these are what a
    /// line keys, and the way one end swings while the other holds
    /// (Alva, 2026-09-01: the lasers pivot on the speakers).
    X1,
    Y1,
    X2,
    Y2,
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
    /// Lightning: how far the bolt wanders from the line, canvas units.
    Jag,
    /// Lightning: how many forks leave the bolt.
    Branches,
    /// Lightning: re-rolls a second — the crackle.
    Strike,
    /// Vortex: the void's radius as a fraction of the disk's.
    Hole,
    /// Vortex: how tightly the streaks spiral; either sign winds the
    /// other way.
    Twist,
    /// Vortex: how fast the disk turns, radians a second.
    Spin,
    /// Vortex: how fine and broken-up the streaks are.
    Grain,
    /// Camera: how far the picture jolts, canvas units.
    Shake,
    /// Camera: how fast it rumbles, shakes a second.
    ShakeRate,
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
    /// A line's two ends; `None` for everything else.
    pub ends: Option<([f32; 2], [f32; 2])>,
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
    if s.is_light() {
        s.size()
    } else {
        s.size() * 2.0
    }
}

/// Slider/scrub range per property. `canvas` is the comp's size: the
/// place and side ranges run across it.
pub fn range(prop: Prop, canvas: [f32; 2]) -> (f32, f32) {
    let [cw, ch] = canvas;
    match prop {
        Prop::X | Prop::X1 | Prop::X2 => (0.0, cw),
        Prop::Y | Prop::Y1 | Prop::Y2 => (0.0, ch),
        // Never clamped — see `fit`. Here for the slider maths only.
        Prop::Rotation | Prop::Tilt | Prop::Turn => (-std::f32::consts::PI, std::f32::consts::PI),
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
        // As far as `[` / `]` and `set_sides` go — the old slider stopped
        // at 12, which was a ceiling the keyboard didn't have.
        Prop::Sides => (3.0, 24.0),
        Prop::Thickness => (1.0, 30.0),
        Prop::Cone => (2.0, 120.0),
        Prop::Rim => (0.0, 1.0),
        Prop::Depth => (1.0, ch),
        Prop::Density => (2.0, 120.0),
        Prop::Twinkle => (0.0, 1.0),
        Prop::TwinkleRate => (0.0, 12.0),
        Prop::Seed => (0.0, 100.0),
        Prop::Jag => (0.0, 300.0),
        Prop::Branches => (0.0, spark_render::MAX_BRANCHES),
        Prop::Strike => (0.0, 60.0),
        Prop::Hole => (0.0, 0.9),
        Prop::Twist => (-8.0, 8.0),
        Prop::Spin => (-6.0, 6.0),
        Prop::Grain => (0.0, 1.0),
        // A slider's reach, not a wall — see `fit`.
        Prop::Shake => (0.0, 200.0),
        Prop::ShakeRate => (0.0, 60.0),
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
/// **The transform has no walls.** Rotation is not an angle in (-π, π],
/// it is *how far the shape has turned*, and it has to keep counting —
/// folding it made a continuous spin impossible (stamp 0°, stamp 180°,
/// keep turning, and the third key came back as -170°, so the shape
/// unwound to reach it). Place and depth are free too: a shape flies in
/// from off the canvas, and Z runs wherever the gizmo already lets it
/// (Alva, 2026-08-31: "Z only goes up to 1400 but with the gizmo I can
/// move it to 2800 and key it — if I ever touch that keyframe again it
/// snaps down to 1400"). The ranges those props declare are where a
/// slider would end, not where the object may go. Sizes keep a floor
/// and no ceiling (Alva's room: "it only scales to barely bigger than it
/// was and it stops"). Only the look props — a slider's whole world —
/// still clamp.
pub fn fit(prop: Prop, v: f32, canvas: [f32; 2]) -> f32 {
    if matches!(
        prop,
        Prop::X
            | Prop::Y
            | Prop::X1
            | Prop::Y1
            | Prop::X2
            | Prop::Y2
            | Prop::Z
            | Prop::Rotation
            | Prop::Tilt
            | Prop::Turn
    ) {
        return v;
    }
    let (min, max) = range(prop, canvas);
    if matches!(
        prop,
        Prop::Scale | Prop::Width | Prop::Height | Prop::Depth | Prop::Shake | Prop::ShakeRate
    ) {
        return v.max(min);
    }
    // Float noise at a wall is the wall: a slider dragged to its end
    // can land a float short of it (an opacity of 0.99999994 — which a
    // key then holds, and which is not 1 to anything that asks), so a
    // value within a hair of an end is snapped onto it.
    let hair = (max - min).abs() * 1e-6;
    if (v - max).abs() <= hair {
        return max;
    }
    if (v - min).abs() <= hair {
        return min;
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

/// The shape a drag from `press` to `cursor` makes with `tool`, dressed
/// in the tool's draw defaults and the current colour. Glow is *not*
/// written here: it is an effect, and `fx::resolve` overwrites the
/// shape's own glow field from the stack every frame — the birth road
/// (`Editor::mouse_down`) adds the Glow effect the defaults ask for.
pub(crate) fn draw_shape(
    tool: Tool,
    press: [f32; 2],
    cursor: [f32; 2],
    d: &crate::defaults::ToolDefaults,
    rgb: [f32; 3],
) -> Shape {
    let dist = dist(press, cursor).max(3.0);
    let half = [
        (cursor[0] - press[0]).abs().max(3.0),
        (cursor[1] - press[1]).abs().max(3.0),
    ];
    if tool == Tool::Stars {
        let mut s = Shape::stars(press, half, seed_at(press))
            .color(rgb[0], rgb[1], rgb[2])
            .intensity(d.brightness);
        // A star's radius rides the thickness slot on a field.
        s.set_thickness(d.thickness);
        s.set_opacity(d.opacity);
        s.set_density(d.density);
        s.set_twinkle(d.twinkle);
        s.set_twinkle_rate(d.rate);
        s.set_star_form(d.form);
        return s;
    }
    if tool == Tool::Vortex {
        let mut s = Shape::vortex(press, half, seed_at(press))
            .color(rgb[0], rgb[1], rgb[2])
            .intensity(d.brightness);
        s.set_thickness(d.thickness);
        s.set_opacity(d.opacity);
        s.set_hole(d.hole);
        s.set_twist(d.twist);
        s.set_spin(d.spin);
        s.set_grain(d.grain);
        return s;
    }
    if tool == Tool::Bolt {
        let mut s = Shape::bolt(press, cursor, seed_at(press))
            .color(rgb[0], rgb[1], rgb[2])
            .intensity(d.brightness);
        s.set_thickness(d.thickness);
        s.set_opacity(d.opacity);
        s.set_jag(d.jag);
        s.set_branches(d.branches);
        s.set_strike_rate(d.strike);
        return s;
    }
    // An outline is a stroke; a fill is stroke zero.
    let stroke = if d.outline { d.thickness } else { 0.0 };
    let shape = match tool {
        Tool::Circle => Shape::circle(press, dist).stroke(stroke),
        Tool::Box => Shape::rect(press, half).stroke(stroke),
        Tool::Polygon => Shape::ngon(press, dist, d.sides).stroke(stroke),
        Tool::Line => Shape::line(press, cursor, d.thickness),
        Tool::Select | Tool::Stars | Tool::Bolt | Tool::Vortex => unreachable!("handled above"),
    };
    let mut shape = shape
        .color(rgb[0], rgb[1], rgb[2])
        .intensity(d.brightness)
        .glow(0.0);
    shape.set_opacity(d.opacity);
    shape
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

    /// Lightning is drawn like a line — from the press to the cursor —
    /// wearing the tool's defaults, and it is a line for everything
    /// about where it is: it keys by its ends, and its own knobs are
    /// keyable too.
    #[test]
    fn a_bolt_is_a_line_with_a_temper() {
        use crate::anim::{keyable, prop_value};
        let d = crate::defaults::ToolDefaults::birth(Tool::Bolt);
        let s = draw_shape(Tool::Bolt, [100.0, 100.0], [700.0, 400.0], &d, [1.0, 0.5, 0.2]);
        assert_eq!(s.kind(), spark_render::ShapeKind::Bolt);
        assert!(s.is_line() && s.is_bolt());
        assert_eq!(s.line_ends(), ([100.0, 100.0], [700.0, 400.0]));
        assert_eq!(s.center(), [400.0, 250.0]);
        assert_eq!(s.jag(), Some(d.jag));
        assert_eq!(s.branches(), Some(d.branches));
        assert_eq!(s.strike_rate(), Some(d.strike));
        assert!(s.seed().is_some(), "a bolt has a seed like a sky does");
        assert_eq!(kind_parts(s.kind()).1, "lightning");
        for p in [Prop::X1, Prop::Y2, Prop::Jag, Prop::Branches, Prop::Strike] {
            assert!(keyable(&s, p), "{p:?} should key on a bolt");
        }
        assert!(!keyable(&s, Prop::X), "a line keys by its ends, not its centre");
        assert_eq!(prop_value(&s, Prop::Jag), Some(d.jag));
        // A circle has no jag to key.
        let c = Shape::circle([0.0; 2], 10.0);
        assert!(!keyable(&c, Prop::Jag));
        // The birth look is the renderer's own fresh bolt, so there is one
        // source of truth for what lightning starts as.
        let fresh = Shape::bolt([0.0; 2], [1.0; 2], 0.0);
        assert_eq!(d.jag, fresh.jag().unwrap());
        assert_eq!(d.glow, fresh.glow_radius());
        assert!(d.glow > 0.0, "lightning is born glowing");
    }

    /// Drawing a field is a drag over a region, like a box — and it must
    /// come out as a field, not as whatever the last kind added was.
    #[test]
    fn the_stars_tool_draws_a_field() {
        let d = crate::defaults::ToolDefaults::birth(Tool::Stars);
        let s = draw_shape(Tool::Stars, [100.0, 100.0], [220.0, 180.0], &d, [1.0; 3]);
        assert_eq!(s.kind(), spark_render::ShapeKind::Stars);
        assert_eq!(s.center(), [100.0, 100.0]);
        assert_eq!(s.box_size(), Some([240.0, 160.0]), "the dragged region");
        assert!(s.density().is_some() && s.twinkle().is_some());
        // Two fields drawn in different places are different skies.
        let other = draw_shape(Tool::Stars, [700.0, 400.0], [820.0, 480.0], &d, [1.0; 3]);
        assert_ne!(s.seed(), other.seed());
    }

    #[test]
    fn other_props_still_clamp() {
        let c = spark_render::CANVAS;
        let (min, max) = range(Prop::Glow, c);
        assert_eq!(fit(Prop::Glow, max + 500.0, c), max);
        assert_eq!(fit(Prop::Glow, min - 500.0, c), min);
    }

    /// A value a float short of a wall is the wall: the opacity a drag
    /// left at 0.99999994 is 1, and a key holds a 1 the renderer agrees
    /// is one. A value a real distance in is left alone.
    #[test]
    fn float_noise_at_a_wall_is_the_wall() {
        let c = spark_render::CANVAS;
        assert_eq!(fit(Prop::Opacity, 0.999_999_94, c), 1.0);
        assert_eq!(fit(Prop::Opacity, 0.000_000_06, c), 0.0);
        assert_eq!(fit(Prop::Opacity, 0.999, c), 0.999);
        assert_eq!(fit(Prop::Glow, 199.999_99, c), 200.0);
    }

    /// The place ranges are the comp's — on a portrait canvas Y runs to
    /// 1920 — but they are a slider's reach, not a wall: a place off the
    /// canvas and a depth past the old 1400 ceiling both pass through,
    /// the way the gizmo already put them there.
    #[test]
    fn place_ranges_follow_the_canvas_and_the_transform_has_no_walls() {
        let tall = [1080.0, 1920.0];
        assert_eq!(range(Prop::X, tall), (0.0, 1080.0));
        assert_eq!(range(Prop::Y, tall), (0.0, 1920.0));
        assert_eq!(fit(Prop::Y, 1500.0, tall), 1500.0);
        assert_eq!(fit(Prop::Y, 1500.0, spark_render::CANVAS), 1500.0);
        assert_eq!(fit(Prop::X, -400.0, spark_render::CANVAS), -400.0);
        assert_eq!(fit(Prop::Z, 2800.0, spark_render::CANVAS), 2800.0);
        assert_eq!(fit(Prop::Z, -20000.0, spark_render::CANVAS), -20000.0);
        assert_eq!(value_for(Prop::Y, 0.5, tall), 960.0);
    }
}
