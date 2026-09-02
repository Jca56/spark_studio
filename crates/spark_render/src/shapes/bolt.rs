//! Lightning: a bolt between two points. The first of the generators
//! after the star field, and the first that rides its own **clock** —
//! the clip-local time every shape now carries (`Scene::clocks`) — so a
//! bolt re-rolls its shape as its clip plays and a looped clip crackles
//! the same way every pass.
//!
//! The bolt itself lives in `shape.wgsl` (`draw_bolt`): the segment is
//! cut into pieces, every joint is thrown sideways by a hashed amount,
//! and a few branches fork off — all from the seed and the strike
//! number, nothing stored, so `frame = render(project, t)` holds. What's
//! here is the document side: a bolt is a *line* (its ends are its
//! place, it keys by them, it picks by them) with a temper, and the
//! knobs ride the instance's float budget — `style[1]` is the core's
//! half-width like a line's, `extra` is `[seed, jag, branches, rate]`.

use super::{KIND_BOLT, Shape};

/// A fresh bolt's look: a fine hot core with a wide halo, a good wander,
/// two forks, and a crackle of a dozen re-rolls a second.
const GLOW: f32 = 18.0;
const HALF_WIDTH: f32 = 2.5;
const JAG: f32 = 30.0;
const BRANCHES: f32 = 2.0;
const RATE: f32 = 12.0;

/// The most forks a bolt throws.
pub const MAX_BRANCHES: f32 = 3.0;

impl Shape {
    /// A bolt from `from` to `to`. `seed` picks which bolt — same seed,
    /// same strike, same shape, every render.
    pub fn bolt(from: [f32; 2], to: [f32; 2], seed: f32) -> Self {
        let mut s = Self::base(KIND_BOLT, from, to);
        s.style = [GLOW, HALF_WIDTH, 0.0, 0.0];
        s.extra = [seed, JAG, BRANCHES, RATE];
        s
    }

    pub fn is_bolt(&self) -> bool {
        self.kind_rot[0] == KIND_BOLT
    }

    /// How far the bolt wanders sideways from the straight line between
    /// its ends, canvas units. Zero is a plain line with a glow.
    pub fn jag(&self) -> Option<f32> {
        self.is_bolt().then_some(self.extra[1])
    }

    pub fn set_jag(&mut self, v: f32) {
        if self.is_bolt() {
            self.extra[1] = v.clamp(0.0, 300.0);
        }
    }

    /// How many forks leave the main bolt, 0 to [`MAX_BRANCHES`].
    pub fn branches(&self) -> Option<f32> {
        self.is_bolt().then_some(self.extra[2])
    }

    pub fn set_branches(&mut self, v: f32) {
        if self.is_bolt() {
            self.extra[2] = v.clamp(0.0, MAX_BRANCHES);
        }
    }

    /// How many times a second the bolt re-rolls its shape — the
    /// crackle. Zero holds one bolt still.
    pub fn strike_rate(&self) -> Option<f32> {
        self.is_bolt().then_some(self.extra[3])
    }

    pub fn set_strike_rate(&mut self, v: f32) {
        if self.is_bolt() {
            self.extra[3] = v.clamp(0.0, 60.0);
        }
    }
}
