//! Star fields: one instance, any number of stars.
//!
//! The scatter itself lives in `shape.wgsl` — a grid of cells, each holding
//! one hashed star, so a fragment only ever visits its own 3x3 neighbourhood
//! and density costs nothing. What's here is the document side of it: the
//! knobs, and where they ride in the instance's fixed float budget
//! (`style[1..3]` for size and density, `extra` for the rest).
//!
//! Every accessor is `None` off a star field, so the inspector asks a shape
//! what it has rather than switching on its kind.

use super::{KIND_STARS, Shape};

/// Star forms a field can scatter, in `extra[3]` order.
pub const STAR_FORMS: [&str; 3] = ["Dot", "Sparkle", "Cross"];

/// A fresh star field's look. Tuned so the first drag already reads as a
/// night sky rather than something that needs four sliders before it does.
const GLOW: f32 = 14.0;
const SIZE: f32 = 4.0;
/// Stars across the canvas's width — a full-canvas field at 10 is roughly
/// 55 of them. Spacing is absolute, so a small field is a smaller patch of
/// the same sky rather than a denser one.
///
/// 10 because Alva reckons that's the good amount and 20 is already a lot
/// (2026-08-17); the slider still runs to 120 for dust.
const DENSITY: f32 = 10.0;
const TWINKLE: f32 = 0.6;
const RATE: f32 = 3.0;

impl Shape {
    /// A star field filling the box `center ± half`. `seed` picks which
    /// scatter you get — same seed, same sky, every render.
    pub fn stars(center: [f32; 2], half: [f32; 2], seed: f32) -> Self {
        let mut s = Self::base(KIND_STARS, center, half);
        s.style = [GLOW, SIZE, DENSITY, 0.0];
        s.extra = [seed, TWINKLE, RATE, 0.0];
        s
    }

    /// Stars across the canvas's width. Absolute, not per-field — see
    /// [`DENSITY`].
    pub fn density(&self) -> Option<f32> {
        self.is_stars().then_some(self.style[2])
    }

    pub fn set_density(&mut self, n: f32) {
        if self.is_stars() {
            self.style[2] = n.clamp(2.0, 120.0);
        }
    }

    /// Which scatter you get. Same seed, same sky, every render — that's what
    /// keeps `frame = render(project, t)` true of a field of five hundred
    /// stars nobody placed.
    pub fn seed(&self) -> Option<f32> {
        self.is_stars().then_some(self.extra[0])
    }

    pub fn set_seed(&mut self, s: f32) {
        if self.is_stars() {
            self.extra[0] = s.clamp(0.0, 100.0);
        }
    }

    /// How hard the stars pulse, 0 = a still sky.
    pub fn twinkle(&self) -> Option<f32> {
        self.is_stars().then_some(self.extra[1])
    }

    pub fn set_twinkle(&mut self, v: f32) {
        if self.is_stars() {
            self.extra[1] = v.clamp(0.0, 1.0);
        }
    }

    /// Twinkle speed in radians per second — each star rides its own phase,
    /// so they never pulse in lockstep.
    pub fn twinkle_rate(&self) -> Option<f32> {
        self.is_stars().then_some(self.extra[2])
    }

    pub fn set_twinkle_rate(&mut self, v: f32) {
        if self.is_stars() {
            self.extra[2] = v.clamp(0.0, 12.0);
        }
    }

    /// Index into [`STAR_FORMS`]: dot, sparkle, or diffraction cross.
    pub fn star_form(&self) -> Option<usize> {
        self.is_stars().then(|| self.extra[3] as usize)
    }

    pub fn set_star_form(&mut self, form: usize) {
        if self.is_stars() {
            self.extra[3] = form.min(STAR_FORMS.len() - 1) as f32;
        }
    }}
