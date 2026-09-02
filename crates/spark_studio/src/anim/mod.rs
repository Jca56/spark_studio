//! Keyframe curves: per-property value tracks over **clip-local** time.
//! Evaluation is pure — `apply(shape, t)` poses a shape without touching
//! the curves — which is what keeps `frame = render(project, t)` honest.
//!
//! This module is the curve itself: keys, easing, and sampling. The
//! property table (what is animatable, how a value is read off a shape and
//! written back) lives in [`props`]. Curves live inside clips
//! ([`crate::doc::ObjClip`]) since the object/clip model landed.

mod props;
mod target;

use spark_render::Shape;

pub use props::{
    PROP_ORDER, apply_prop, changed, first_pose, keyable, parse_prop, place_axis, prop_bit,
    prop_tag, prop_value,
};
pub use target::Target;

use crate::fx::Stack;

/// Two keys closer than this (seconds) are the same key.
pub const KEY_EPS: f32 = 0.001;

/// How a key interpolates toward the *next* key.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Ease {
    Smooth,
    Linear,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Key {
    pub t: f32,
    pub v: f32,
    pub ease: Ease,
}

/// One target's keys, kept sorted by time.
#[derive(Clone, PartialEq, Debug)]
pub struct Track {
    pub target: Target,
    pub keys: Vec<Key>,
}

impl Track {
    /// Curve value at `t`: clamped to the first/last key outside the range,
    /// interpolated between the surrounding pair inside it.
    ///
    /// A smooth key used to be a smoothstep across its own segment, which
    /// zeroes the velocity at *both* ends — so a run of keys visibly stopped
    /// dead at every one of them instead of flowing through. Now a smooth
    /// segment is a cubic Hermite whose end slopes come from the keys on
    /// either side (see [`Track::tangent`]), so three keys read as one
    /// continuous move.
    pub fn sample(&self, t: f32) -> Option<f32> {
        let first = self.keys.first()?;
        if t <= first.t {
            return Some(first.v);
        }
        let last = self.keys.last()?;
        if t >= last.t {
            return Some(last.v);
        }
        let i = self.keys.partition_point(|k| k.t <= t) - 1;
        let a = self.keys[i];
        let b = self.keys[i + 1];
        let h = (b.t - a.t).max(KEY_EPS);
        let u = ((t - a.t) / h).clamp(0.0, 1.0);
        if a.ease == Ease::Linear {
            return Some(a.v + (b.v - a.v) * u);
        }
        let (m0, m1) = (self.tangent(i), self.tangent(i + 1));
        // Cubic Hermite basis.
        let (u2, u3) = (u * u, u * u * u);
        Some(
            (2.0 * u3 - 3.0 * u2 + 1.0) * a.v
                + (u3 - 2.0 * u2 + u) * h * m0
                + (-2.0 * u3 + 3.0 * u2) * b.v
                + (u3 - u2) * h * m1,
        )
    }

    /// The slope the curve passes through key `i` with, in value per second.
    ///
    /// Interior keys average the slopes on either side, which is what makes
    /// motion carry through them. The first and last key get a flat tangent
    /// so a plain two-key move still eases in and out the way it always did
    /// — the stutter was only ever about the keys in the middle.
    ///
    /// A key the curve turns around at is flattened too: taking the average
    /// there would sail the interpolation past the value that was actually
    /// stamped, and a stamped key is a promise about where the shape is.
    fn tangent(&self, i: usize) -> f32 {
        let cur = self.keys[i];
        let prev = i.checked_sub(1).map(|j| self.keys[j]);
        let next = self.keys.get(i + 1).copied();
        let (Some(p), Some(n)) = (prev, next) else {
            return 0.0;
        };
        let into = (cur.v - p.v) / (cur.t - p.t).max(KEY_EPS);
        let out = (n.v - cur.v) / (n.t - cur.t).max(KEY_EPS);
        if into * out <= 0.0 {
            0.0
        } else {
            (into + out) * 0.5
        }
    }

    /// Land a whole key: one already at its time is replaced, ease and
    /// all — a paste lands on what it lands on — otherwise it slots into
    /// time order.
    pub fn put(&mut self, key: Key) {
        match self.keys.iter_mut().find(|k| (k.t - key.t).abs() < KEY_EPS) {
            Some(k) => *k = key,
            None => {
                let at = self.keys.partition_point(|k| k.t < key.t);
                self.keys.insert(at, key);
            }
        }
    }

    /// Set the value at `t`: overwrites a key already there, otherwise
    /// inserts a new key in time order — **linear**, Alva's default
    /// (2026-09-01); a right-click in the clip view makes it smooth.
    pub fn upsert(&mut self, t: f32, v: f32) {
        match self.keys.iter_mut().find(|k| (k.t - t).abs() < KEY_EPS) {
            Some(k) => k.v = v,
            None => {
                let at = self.keys.partition_point(|k| k.t < t);
                self.keys.insert(
                    at,
                    Key {
                        t,
                        v,
                        ease: Ease::Linear,
                    },
                );
            }
        }
    }
}

/// All of one shape's tracks. Empty tracks never persist — "has a track"
/// always means "has keys".
#[derive(Clone, PartialEq, Default, Debug)]
pub struct ShapeAnim {
    pub tracks: Vec<Track>,
}

impl ShapeAnim {
    pub fn track_mut(&mut self, target: Target) -> Option<&mut Track> {
        self.tracks.iter_mut().find(|t| t.target == target)
    }

    pub fn track(&self, target: Target) -> Option<&Track> {
        self.tracks.iter().find(|t| t.target == target)
    }

    /// Every target this layer animates, in track order.
    pub fn targets(&self) -> Vec<Target> {
        self.tracks.iter().map(|t| t.target).collect()
    }

    pub fn has_keys(&self) -> bool {
        self.tracks.iter().any(|t| !t.keys.is_empty())
    }

    /// Distinct key times across every track, sorted, with each time's ease
    /// (linear wins if any co-timed key is linear — the lane marker shows it).
    pub fn key_times(&self) -> Vec<(f32, Ease)> {
        let mut out: Vec<(f32, Ease)> = Vec::new();
        for track in &self.tracks {
            for k in &track.keys {
                match out.iter_mut().find(|(t, _)| (*t - k.t).abs() < KEY_EPS) {
                    Some((_, e)) => {
                        if k.ease == Ease::Linear {
                            *e = Ease::Linear;
                        }
                    }
                    None => out.push((k.t, k.ease)),
                }
            }
        }
        out.sort_by(|a, b| a.0.total_cmp(&b.0));
        out
    }

    /// Pose `shape` and its effect stack at time `t` — keyed targets take
    /// their curve value, everything else keeps its own.
    ///
    /// Shape properties go first, in [`PROP_ORDER`] (geometry before
    /// uniform scale, look last). Effect parameters have no ordering
    /// constraint among themselves: each writes a distinct slot.
    pub fn apply(&self, shape: &mut Shape, fx: &mut Stack, t: f32) {
        for prop in PROP_ORDER {
            let Some(track) = self.track(Target::Shape(prop)) else {
                continue;
            };
            if let Some(v) = track.sample(t) {
                apply_prop(shape, prop, v);
            }
        }
        for track in &self.tracks {
            let Target::Effect { id, param } = track.target else {
                continue;
            };
            if let (Some(v), Some(e)) = (track.sample(t), fx.find_mut(id)) {
                e.set(param as usize, v);
            }
        }
    }

    /// Pose only the shape half, for callers with no effect stack to hand
    /// (a saved-shape export, a test fixture).
    #[allow(dead_code)] // kept for the redesign; the clip view / inspector re-consume it
    pub fn apply_shape(&self, shape: &mut Shape, t: f32) {
        let mut fx = Stack::default();
        self.apply(shape, &mut fx, t);
    }

    /// Keyed-property bitmask, for gold value readouts (see [`prop_bit`]).
    #[allow(dead_code)] // kept for the redesign; the old panels were the only caller
    pub fn keyed_mask(&self) -> u32 {
        self.tracks
            .iter()
            .filter(|t| !t.keys.is_empty())
            .filter_map(|t| t.target.prop())
            .fold(0, |m, p| m | prop_bit(p))
    }

    #[allow(dead_code)] // kept for the redesign; the clip view / inspector re-consume it
    pub fn prune_empty(&mut self) {
        self.tracks.retain(|t| !t.keys.is_empty());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::props::Prop;

    fn track(keys: &[(f32, f32)]) -> Track {
        Track {
            target: Target::Shape(Prop::Rotation),
            keys: keys
                .iter()
                .map(|&(t, v)| Key {
                    t,
                    v,
                    ease: Ease::Smooth,
                })
                .collect(),
        }
    }

    /// Speed across a short window, for asking whether the curve is still
    /// moving at a given moment.
    fn speed(tr: &Track, t: f32) -> f32 {
        let d = 0.005;
        (tr.sample(t + d).unwrap() - tr.sample(t - d).unwrap()) / (2.0 * d)
    }

    /// The complaint, in a test: a run of evenly-paced keys has to read as
    /// one continuous move. It used to smoothstep each segment separately,
    /// so the shape braked to a dead stop at every key and set off again.
    #[test]
    fn motion_carries_through_the_middle_keys() {
        let tr = track(&[(0.0, 0.0), (1.0, 100.0), (2.0, 200.0), (3.0, 300.0)]);
        let cruise = speed(&tr, 1.5);
        for at in [1.0f32, 2.0] {
            let s = speed(&tr, at);
            assert!(
                (s - cruise).abs() < cruise * 0.02,
                "speed dropped to {s} at the key on {at}s (cruising {cruise})"
            );
        }
        // Evenly spaced, evenly valued: the interior is a straight line.
        assert!((tr.sample(1.5).unwrap() - 150.0).abs() < 0.01);
    }

    /// A continuous spin: stamp a turn per second and the shape must keep
    /// turning the same way. Wrapping the input used to make the third key
    /// negative, and the shape unwound counter-clockwise to reach it.
    #[test]
    fn a_spin_never_turns_back() {
        use std::f32::consts::TAU;
        let tr = track(&[(0.0, 0.0), (1.0, TAU), (2.0, 2.0 * TAU), (3.0, 3.0 * TAU)]);
        let mut prev = tr.sample(0.0).unwrap();
        for step in 1..=300 {
            let v = tr.sample(step as f32 * 0.01).unwrap();
            assert!(v >= prev - 1e-4, "rotation went backwards at step {step}");
            prev = v;
        }
        assert!((prev - 3.0 * TAU).abs() < 1e-3, "ended at {prev}");
    }

    /// The ends still ease, so a plain two-key move is unchanged — the
    /// stutter was only ever about keys in the middle.
    #[test]
    fn a_two_key_move_still_eases_in_and_out() {
        let tr = track(&[(1.0, 10.0), (3.0, 20.0)]);
        assert_eq!(tr.sample(2.0), Some(15.0), "still symmetric at the middle");
        assert!(tr.sample(1.5).unwrap() < 12.5, "eases in");
        assert!(tr.sample(2.5).unwrap() > 17.5, "eases out");
        assert!(speed(&tr, 1.0).abs() < 0.05, "starts from rest");
        assert!(speed(&tr, 3.0).abs() < 0.05, "comes to rest");
    }

    /// A stamped key is a promise about where the shape is. Carrying speed
    /// through a key the curve turns around at would sail past the value
    /// that was stamped there.
    #[test]
    fn the_curve_never_overshoots_a_turning_point() {
        let tr = track(&[(0.0, 0.0), (1.0, 100.0), (2.0, 0.0)]);
        for step in 0..=200 {
            let v = tr.sample(step as f32 * 0.01).unwrap();
            assert!(
                (-0.01..=100.01).contains(&v),
                "overshot to {v} at step {step}"
            );
        }
        assert!(
            (tr.sample(1.0).unwrap() - 100.0).abs() < 1e-4,
            "hits the key"
        );
    }

    /// Linear keys stay linear — the escape hatch for motion that must not
    /// ease at all, like a loop that has to butt onto itself.
    #[test]
    fn linear_keys_are_a_straight_line() {
        let mut tr = track(&[(0.0, 0.0), (2.0, 100.0)]);
        tr.keys[0].ease = Ease::Linear;
        assert!((tr.sample(0.5).unwrap() - 25.0).abs() < 1e-4);
        assert!((tr.sample(1.5).unwrap() - 75.0).abs() < 1e-4);
    }

    /// Every key is still hit exactly, whatever the interpolation does
    /// between them.
    #[test]
    fn keys_are_hit_exactly() {
        let tr = track(&[(0.0, 5.0), (0.7, -20.0), (1.9, 3.0), (4.0, 60.0)]);
        for k in &tr.keys {
            assert!(
                (tr.sample(k.t).unwrap() - k.v).abs() < 1e-3,
                "key at {}s wanted {} got {:?}",
                k.t,
                k.v,
                tr.sample(k.t)
            );
        }
    }
}
