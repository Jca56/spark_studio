//! The vortex: an accretion disk around a void — the Voidstep artwork,
//! moving (Alva, 2026-09-02: "how can I get a swirly thing like
//! that?"). A region like a star field's; inside it the shader draws a
//! black hole, a hot ring hugging it, and streaks that spiral around
//! the ring and fade toward the edge, smeared by our own noise so it
//! reads as paint rather than geometry, spinning on the shape's clock.
//!
//! The picture lives in `shape.wgsl` (`draw_vortex`); this is the
//! document side. Knobs ride the instance's float budget: `style[1]`
//! is the ring's width as a percentage of the disk's radius (the
//! thickness slot, so the inspector's row is the same row), `style[2]`
//! the grain, `extra` is `[seed, hole, twist, spin]`. The gradient pair
//! colours it: the first colour at the ring, the second at the edge.

use super::{KIND_VORTEX, Shape};

/// A fresh vortex's look: a void a third of the disk across, a ring a
/// tenth of the radius wide, a good twist, a slow spin, painterly grain.
const GLOW: f32 = 30.0;
const RING: f32 = 12.0;
const GRAIN: f32 = 0.7;
const HOLE: f32 = 0.32;
const TWIST: f32 = 2.5;
const SPIN: f32 = 0.6;

impl Shape {
    /// A vortex filling the box `center ± half` (the disk is the
    /// inscribed circle). `seed` picks which streaks.
    pub fn vortex(center: [f32; 2], half: [f32; 2], seed: f32) -> Self {
        let mut s = Self::base(KIND_VORTEX, center, half);
        s.style = [GLOW, RING, GRAIN, 0.0];
        s.extra = [seed, HOLE, TWIST, SPIN];
        s
    }

    pub fn is_vortex(&self) -> bool {
        self.kind_rot[0] == KIND_VORTEX
    }

    /// The void's radius as a fraction of the disk's, 0 to 0.9.
    pub fn hole(&self) -> Option<f32> {
        self.is_vortex().then_some(self.extra[1])
    }

    pub fn set_hole(&mut self, v: f32) {
        if self.is_vortex() {
            self.extra[1] = v.clamp(0.0, 0.9);
        }
    }

    /// How tightly the streaks spiral: zero is rays, either sign winds
    /// the other way.
    pub fn twist(&self) -> Option<f32> {
        self.is_vortex().then_some(self.extra[2])
    }

    pub fn set_twist(&mut self, v: f32) {
        if self.is_vortex() {
            self.extra[2] = v.clamp(-8.0, 8.0);
        }
    }

    /// How fast the disk turns, radians a second on the shape's clock.
    pub fn spin(&self) -> Option<f32> {
        self.is_vortex().then_some(self.extra[3])
    }

    pub fn set_spin(&mut self, v: f32) {
        if self.is_vortex() {
            self.extra[3] = v.clamp(-6.0, 6.0);
        }
    }

    /// How fine and how broken-up the streaks are, 0 smooth to 1 rough.
    pub fn grain(&self) -> Option<f32> {
        self.is_vortex().then_some(self.style[2])
    }

    pub fn set_grain(&mut self, v: f32) {
        if self.is_vortex() {
            self.style[2] = v.clamp(0.0, 1.0);
        }
    }
}
