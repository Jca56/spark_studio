//! The dice: random style for the next shape you draw.
//!
//! Armed from the color home, every new shape rolls its own look — colour,
//! glow, brightness, thickness, fill or outline, gradient, polygon sides,
//! star density — and keeps it. The roll happens once, at mouse-down, and
//! rides the drag unchanged: a shape that re-rolled on every cursor move
//! would strobe while you sized it, and you could never pick the one you
//! liked by letting go.
//!
//! Geometry is never rolled. Where you pressed and how far you dragged is
//! the drawing; the dice only dresses it.
//!
//! Rolling is an *edit*, not a render — the values land in the document
//! like any hand-set ones, so `render(project, t)` stays pure and the roll
//! is reproduced by undo/redo and save/load exactly as it fell.

use spark_render::{STAR_FORMS, Shape};
use spark_ui::picker::{hsv_to_rgb, srgb_to_linear};

/// xorshift64*: eight lines of our own rather than a crate. It only has to
/// be fast and not obviously patterned, which for picking a colour it is.
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        // Zero is xorshift's one fixed point; splitmix the seed so any input
        // (including zero) lands on a live state.
        let mut z = seed.wrapping_add(0x9E37_79B9_7F4A_7C15);
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        Self((z ^ (z >> 31)) | 1)
    }

    /// Seeded from the wall clock: each launch, and each roll within it,
    /// is its own.
    pub fn from_clock() -> Self {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0x5EED);
        Self::new(nanos)
    }

    pub fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    /// Uniform in `[0, 1)`.
    pub fn unit(&mut self) -> f32 {
        (self.next_u64() >> 40) as f32 / (1u64 << 24) as f32
    }

    /// Uniform in `[lo, hi)`.
    pub fn range(&mut self, lo: f32, hi: f32) -> f32 {
        lo + self.unit() * (hi - lo)
    }

    /// `true` with probability `p`.
    pub fn chance(&mut self, p: f32) -> bool {
        self.unit() < p
    }

    /// Uniform integer in `lo..=hi`.
    pub fn int(&mut self, lo: u32, hi: u32) -> u32 {
        lo + (self.unit() * (hi - lo + 1) as f32) as u32
    }
}

/// One roll of the dice: everything a new shape's look can be, fixed for
/// the length of a drag. Settings a given kind can't carry are ignored by
/// `apply` the same way the setters ignore them.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Roll {
    /// Linear RGB, always a saturated neon — a random grey is a bug report.
    pub rgb: [f32; 3],
    /// A gradient to a second colour, when the roll says so.
    pub rgb2: Option<[f32; 3]>,
    pub glow: f32,
    pub intensity: f32,
    /// Stroke half-width, for what strokes.
    pub thickness: f32,
    pub outline: bool,
    pub additive: bool,
    pub sides: u32,
    /// Star field look: cells across, twinkle depth, twinkle rate, form.
    pub stars: (f32, f32, f32, usize),
}

/// A saturated colour at a random hue, in linear RGB the way every shape
/// colour is held. Saturation stays high and value stays full so the roll
/// lands inside the neon family the palette belongs to.
pub fn neon(rng: &mut Rng) -> [f32; 3] {
    let srgb = hsv_to_rgb(rng.unit(), rng.range(0.72, 1.0), 1.0);
    [
        srgb_to_linear(srgb[0]),
        srgb_to_linear(srgb[1]),
        srgb_to_linear(srgb[2]),
    ]
}

impl Roll {
    pub fn new(rng: &mut Rng) -> Self {
        // Squared so the halo is usually modest and only sometimes huge:
        // a uniform glow made every third shape a fog bank.
        let glow = rng.unit().powi(2) * 160.0;
        Self {
            rgb: neon(rng),
            rgb2: rng.chance(0.4).then(|| neon(rng)),
            glow: if rng.chance(0.25) { 0.0 } else { glow },
            intensity: rng.range(0.7, 2.0),
            thickness: rng.range(1.5, 12.0),
            outline: rng.chance(0.5),
            additive: rng.chance(0.3),
            sides: rng.int(3, 8),
            stars: (
                rng.range(8.0, 60.0),
                rng.unit(),
                rng.range(0.5, 8.0),
                rng.int(0, STAR_FORMS.len() as u32 - 1) as usize,
            ),
        }
    }

    /// Dress a freshly drawn shape in this roll. Each setter is a no-op on
    /// a kind that has no such setting, so one roll fits every tool.
    pub fn apply(&self, mut shape: Shape) -> Shape {
        shape.set_rgb(self.rgb);
        shape.set_brightness(self.intensity);
        shape.set_glow(self.glow);
        shape.set_gradient(self.rgb2.is_some());
        if let Some(rgb2) = self.rgb2 {
            shape.set_rgb2(rgb2);
        }
        shape.set_additive(self.additive);
        shape.set_sides(self.sides);
        // Outline first: it writes the stroke width to a fixed 4.0, and the
        // rolled thickness has to land on top of that, not under it. On a
        // fill the thickness setter is a no-op, as it is for the panel.
        shape.set_outline(self.outline);
        shape.set_thickness(self.thickness);
        let (density, twinkle, rate, form) = self.stars;
        shape.set_density(density);
        shape.set_twinkle(twinkle);
        shape.set_twinkle_rate(rate);
        shape.set_star_form(form);
        shape
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The same seed rolls the same dice — what makes the rest testable.
    #[test]
    fn a_seed_is_reproducible() {
        let mut a = Rng::new(42);
        let mut b = Rng::new(42);
        assert_eq!(Roll::new(&mut a), Roll::new(&mut b));
        assert_ne!(Roll::new(&mut a), Roll::new(&mut Rng::new(43)));
    }

    /// `unit` stays inside `[0, 1)` and actually moves around in it.
    #[test]
    fn unit_covers_the_interval() {
        let mut rng = Rng::new(7);
        let (mut lo, mut hi) = (1.0f32, 0.0f32);
        for _ in 0..10_000 {
            let u = rng.unit();
            assert!((0.0..1.0).contains(&u), "{u} out of range");
            lo = lo.min(u);
            hi = hi.max(u);
        }
        assert!(lo < 0.01 && hi > 0.99, "range only reached {lo}..{hi}");
        // Zero seeds must not stick at zero.
        let mut z = Rng::new(0);
        assert_ne!(z.next_u64(), z.next_u64());
    }

    /// Every rolled value lands where the inspector's sliders can reach it,
    /// so a rolled shape is never one the panel can't then edit.
    #[test]
    fn rolls_stay_inside_the_sliders() {
        use crate::props::{Prop, range};
        let mut rng = Rng::new(99);
        for _ in 0..500 {
            let r = Roll::new(&mut rng);
            let within = |p: Prop, v: f32| {
                let (lo, hi) = range(p, spark_render::CANVAS);
                assert!((lo..=hi).contains(&v), "{p:?} rolled {v}, range {lo}..{hi}");
            };
            within(Prop::Glow, r.glow);
            within(Prop::Brightness, r.intensity);
            within(Prop::Thickness, r.thickness);
            within(Prop::Sides, r.sides as f32);
            within(Prop::Density, r.stars.0);
            within(Prop::Twinkle, r.stars.1);
            within(Prop::TwinkleRate, r.stars.2);
            assert!(r.stars.3 < STAR_FORMS.len());
            // Neon: full value somewhere, never a grey.
            let max = r.rgb.iter().cloned().fold(0.0, f32::max);
            let min = r.rgb.iter().cloned().fold(1.0, f32::min);
            assert!((max - 1.0).abs() < 1e-4, "rolled colour {:?} is dim", r.rgb);
            assert!(max - min > 0.3, "rolled colour {:?} is grey", r.rgb);
        }
    }

    /// The roll dresses every tool's shape, and only where it can: a
    /// circle gets sides ignored, a line never gets a gradient-free
    /// outline flag, and a field keeps its own star radius.
    #[test]
    fn a_roll_dresses_each_kind() {
        use crate::props::{Tool, draw_shape};
        let mut rng = Rng::new(3);
        let mut r = Roll::new(&mut rng);
        r.outline = true;
        r.rgb2 = Some([0.1, 0.2, 0.3]);
        r.sides = 7;
        let draw = |t| r.apply(draw_shape(t, [300.0, 300.0], [400.0, 360.0], 5, [1.0; 3]));

        let poly = draw(Tool::Polygon);
        assert_eq!(poly.sides(), Some(7));
        assert_eq!(poly.rgb(), r.rgb);
        assert!(poly.gradient() && poly.rgb2() == [0.1, 0.2, 0.3]);
        assert_eq!(poly.outline(), Some(true));
        assert!((poly.thickness().unwrap() - r.thickness).abs() < 1e-4);
        assert!((poly.glow_radius() - r.glow).abs() < 1e-4);

        let circle = draw(Tool::Circle);
        assert_eq!(circle.sides(), None);
        assert_eq!(circle.outline(), Some(true));

        let line = draw(Tool::Line);
        assert_eq!(line.outline(), None);
        assert!((line.thickness().unwrap() - r.thickness).abs() < 1e-4);

        let stars = draw(Tool::Stars);
        assert_eq!(stars.outline(), None);
        assert!((stars.density().unwrap() - r.stars.0).abs() < 1e-3);
        assert_eq!(stars.star_form(), Some(r.stars.3));

        // A filled roll leaves the stroke width alone.
        r.outline = false;
        let solid = r.apply(draw_shape(Tool::Box, [0.0; 2], [50.0, 50.0], 5, [1.0; 3]));
        assert_eq!(solid.outline(), Some(false));
    }
}
