//! Editing a clip's curves by hand — the clip view's road into the
//! document (④ of the object/clip build order). Stamping (`keys`) is
//! how keys are *made*; this is how they are moved, retimed, added on
//! the line, eased and deleted afterwards.
//!
//! Every edit is undoable. A drag coalesces under `Tag::Keys` — one
//! undo step per gesture, like a slider — and the discrete edits push
//! their own step. Keys never cross their neighbours: a drag stops at
//! the key beside it rather than swapping order underneath a curve
//! that is being looked at. Curves are document truth (`clips`), so
//! nothing here touches the working copies; the next frame's
//! `sync_to_time` re-poses the object from the edited curve.

use super::Editor;
use crate::anim::{Ease, KEY_EPS, Key, ShapeAnim, Target};
use crate::history::Tag;

/// The least two keys may sit apart after a drag — a key can never land
/// *on* its neighbour, which would fold two keys into one.
const APART: f32 = KEY_EPS * 2.0;

impl Editor {
    /// Object `i`'s clip `c`'s curves, read-only.
    pub fn clip_anim(&self, i: usize, c: usize) -> Option<&ShapeAnim> {
        self.clips.get(i)?.get(c).map(|cl| &cl.anim)
    }

    /// Start (or continue) a key-drag undo step: prior hand edits fold
    /// into the truth first, or the snapshot would silently carry them.
    fn record_keys(&mut self) {
        self.absorb_pending();
        let s = self.snap();
        self.history.change(Tag::Keys, s);
    }

    /// A discrete curve edit: its own undo step.
    fn push_keys(&mut self) {
        self.absorb_pending();
        let s = self.snap();
        self.history.push(s);
    }

    /// A curve changed under the object: an un-stamped preview pose
    /// would hide the edit until the playhead moved, so it is dropped.
    fn unpose(&mut self, i: usize) {
        self.posed.retain(|&p| p != i);
    }

    /// Where a key at index `k` on `track` may move to: between its
    /// neighbours, never before local zero.
    fn window(keys: &[Key], k: usize) -> (f32, f32) {
        let lo = k.checked_sub(1).map(|p| keys[p].t + APART).unwrap_or(0.0);
        let hi = keys.get(k + 1).map(|n| n.t - APART).unwrap_or(f32::MAX);
        (lo, hi.max(lo))
    }

    /// Fit a value to what the target can take — a shape property's own
    /// fit (angles and sizes keep their freedom), an effect parameter's
    /// declared range.
    fn fit_value(&self, i: usize, target: Target, v: f32) -> f32 {
        match target {
            Target::Shape(p) => crate::props::fit(p, v, self.canvas),
            Target::Effect { id, param } => self
                .base_fx
                .get(i)
                .and_then(|s| s.find(id))
                .and_then(|e| e.kind.params().get(param as usize))
                .map(|s| v.clamp(s.min, s.max))
                .unwrap_or(v),
        }
    }

    /// Move key `k` of `target` on object `i`'s clip `c` to local time
    /// `t` and value `v` — a drag in the clip view. Time clamps between
    /// the neighbouring keys; the value fits its target. Coalesces into
    /// one undo step per drag. False when nothing changed.
    pub fn move_key(
        &mut self,
        i: usize,
        c: usize,
        target: Target,
        k: usize,
        t: f32,
        v: f32,
    ) -> bool {
        let v = self.fit_value(i, target, v);
        let Some(track) = self
            .clips
            .get(i)
            .and_then(|l| l.get(c))
            .and_then(|cl| cl.anim.track(target))
        else {
            return false;
        };
        let Some(&old) = track.keys.get(k) else {
            return false;
        };
        let (lo, hi) = Self::window(&track.keys, k);
        let t = t.clamp(lo, hi);
        if (t - old.t).abs() < 1e-6 && (v - old.v).abs() < 1e-6 {
            return false;
        }
        self.record_keys();
        if let Some(key) = self.clips[i][c]
            .anim
            .track_mut(target)
            .and_then(|tr| tr.keys.get_mut(k))
        {
            key.t = t;
            key.v = v;
        }
        self.unpose(i);
        true
    }

    /// Every key at local time `from` — across every track of the clip —
    /// moves to `to`, clamped so none of them crosses a neighbour on its
    /// own track. The key strip's drag. Returns where they landed.
    pub fn retime_keys_at(&mut self, i: usize, c: usize, from: f32, to: f32) -> Option<f32> {
        let clip = self.clips.get(i)?.get(c)?;
        let mut lo = 0.0f32;
        let mut hi = f32::MAX;
        let mut any = false;
        for tr in &clip.anim.tracks {
            if let Some(k) = tr.keys.iter().position(|k| (k.t - from).abs() < KEY_EPS) {
                let (l, h) = Self::window(&tr.keys, k);
                lo = lo.max(l);
                hi = hi.min(h);
                any = true;
            }
        }
        if !any {
            return None;
        }
        let to = to.clamp(lo, hi.max(lo));
        if (to - from).abs() < 1e-6 {
            return Some(from);
        }
        self.record_keys();
        for tr in &mut self.clips[i][c].anim.tracks {
            for k in &mut tr.keys {
                if (k.t - from).abs() < KEY_EPS {
                    k.t = to;
                }
            }
        }
        self.unpose(i);
        Some(to)
    }

    /// Add a key on `target` at local time `t`, at the value the curve
    /// already has there — a double-click on the line adds a handle
    /// without moving the line. A target with no track yet gets one,
    /// its first key at the object's value as it stands (what `K`
    /// would stamp): that is how a setting the view lists dim comes to
    /// life. Its own undo step; the new key's index.
    pub fn add_key(&mut self, i: usize, c: usize, target: Target, t: f32) -> Option<usize> {
        let t = t.max(0.0);
        let anim = self.clip_anim(i, c)?;
        let v = match anim.track(target) {
            Some(track) => {
                if track.keys.iter().any(|k| (k.t - t).abs() < APART) {
                    return None;
                }
                track.sample(t)?
            }
            None => Self::read(&self.shapes[i], &self.fx[i], target)?,
        };
        self.push_keys();
        let anim = &mut self.clips[i][c].anim;
        match anim.track_mut(target) {
            Some(tr) => tr.upsert(t, v),
            None => anim.tracks.push(crate::anim::Track {
                target,
                keys: vec![Key {
                    t,
                    v,
                    ease: Ease::Smooth,
                }],
            }),
        }
        let at = anim
            .track(target)
            .and_then(|tr| tr.keys.iter().position(|k| (k.t - t).abs() < KEY_EPS));
        self.unpose(i);
        at
    }

    /// Delete key `k` of `target`. The last key on a track takes the
    /// track with it — an empty track never persists. Undoable.
    pub fn delete_key(&mut self, i: usize, c: usize, target: Target, k: usize) -> bool {
        let has = self
            .clip_anim(i, c)
            .and_then(|a| a.track(target))
            .is_some_and(|tr| k < tr.keys.len());
        if !has {
            return false;
        }
        self.push_keys();
        if let Some(tr) = self.clips[i][c].anim.track_mut(target) {
            tr.keys.remove(k);
        }
        self.clips[i][c].anim.prune_empty();
        self.unpose(i);
        true
    }

    /// Delete every key at local time `t`, on every track — the strip's
    /// Delete. Undoable.
    pub fn delete_keys_at(&mut self, i: usize, c: usize, t: f32) -> bool {
        let any = self
            .clip_anim(i, c)
            .is_some_and(|a| a.key_times().iter().any(|(kt, _)| (kt - t).abs() < KEY_EPS));
        if !any {
            return false;
        }
        self.push_keys();
        for tr in &mut self.clips[i][c].anim.tracks {
            tr.keys.retain(|k| (k.t - t).abs() >= KEY_EPS);
        }
        self.clips[i][c].anim.prune_empty();
        self.unpose(i);
        true
    }

    /// Flip key `k`'s ease between Smooth and Linear — how it runs
    /// toward the *next* key. Undoable.
    pub fn toggle_key_ease(&mut self, i: usize, c: usize, target: Target, k: usize) -> bool {
        let Some(old) = self
            .clip_anim(i, c)
            .and_then(|a| a.track(target))
            .and_then(|tr| tr.keys.get(k))
            .map(|key| key.ease)
        else {
            return false;
        };
        self.push_keys();
        if let Some(key) = self.clips[i][c]
            .anim
            .track_mut(target)
            .and_then(|tr| tr.keys.get_mut(k))
        {
            key.ease = match old {
                Ease::Smooth => Ease::Linear,
                Ease::Linear => Ease::Smooth,
            };
        }
        self.unpose(i);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::props::{Prop, Tool};

    /// A circle with an X curve of three keys on its one clip.
    fn keyed() -> (Editor, usize) {
        let mut e = Editor::empty();
        e.set_time(0.0);
        e.sync_to_time();
        e.choose_tool(Tool::Circle);
        e.set_cursor_canvas([300.0, 300.0]);
        e.mouse_down(false);
        e.set_cursor_canvas([380.0, 300.0]);
        e.mouse_up();
        e.choose_tool(Tool::Select);
        let i = e.primary().expect("drawn");
        for (t, x) in [(0.0, 300.0), (0.5, 600.0), (1.0, 900.0)] {
            e.set_time(t);
            e.sync_to_time();
            e.set_prop(Prop::X, x);
            assert!(e.stamp_key());
        }
        e.end_gesture();
        (e, i)
    }

    fn x_keys(e: &Editor, i: usize) -> Vec<(f32, f32)> {
        e.clip_anim(i, 0)
            .and_then(|a| a.track(Target::Shape(Prop::X)))
            .map(|tr| tr.keys.iter().map(|k| (k.t, k.v)).collect())
            .unwrap_or_default()
    }

    /// A dragged key moves in both time and value, stops at its
    /// neighbours, and the whole drag is one undo step.
    #[test]
    fn a_key_drags_between_its_neighbours_as_one_step() {
        let (mut e, i) = keyed();
        let x = Target::Shape(Prop::X);
        assert!(e.move_key(i, 0, x, 1, 0.6, 650.0));
        assert!(e.move_key(i, 0, x, 1, 0.7, 700.0));
        // Past the last key: stops just short of it.
        assert!(e.move_key(i, 0, x, 1, 5.0, 700.0));
        e.end_gesture();
        let keys = x_keys(&e, i);
        assert!(
            keys[1].0 < 1.0 && keys[1].0 > 0.99,
            "clamped at {}",
            keys[1].0
        );
        assert_eq!(keys[1].1, 700.0);
        // Before the first key: stops just after it.
        assert!(e.move_key(i, 0, x, 1, -3.0, 700.0));
        e.end_gesture();
        assert!(x_keys(&e, i)[1].0 > 0.0);
        e.undo();
        assert!(x_keys(&e, i)[1].0 > 0.99, "the second drag undid");
        e.undo();
        assert_eq!(x_keys(&e, i)[1], (0.5, 600.0), "the first drag undid whole");
    }

    /// The curve re-poses the object the moment a key moves — no
    /// stamping, no playhead nudge needed.
    #[test]
    fn moving_a_key_moves_the_shape() {
        let (mut e, i) = keyed();
        e.set_time(0.5);
        e.sync_to_time();
        assert!((e.shapes()[i].center()[0] - 600.0).abs() < 1e-3);
        assert!(e.move_key(i, 0, Target::Shape(Prop::X), 1, 0.5, 100.0));
        e.sync_to_time();
        assert!((e.shapes()[i].center()[0] - 100.0).abs() < 1e-3);
    }

    /// The strip retimes every key at a moment together, and stops where
    /// the tightest track would cross a neighbour.
    #[test]
    fn a_moment_retimes_across_every_track() {
        let (mut e, i) = keyed();
        // A second track with a key at 0.5 and a tight neighbour at 0.6.
        e.clip_anim_mut(i, 0).tracks.push(crate::anim::Track {
            target: Target::Shape(Prop::Opacity),
            keys: vec![
                Key {
                    t: 0.5,
                    v: 1.0,
                    ease: Ease::Smooth,
                },
                Key {
                    t: 0.6,
                    v: 2.0,
                    ease: Ease::Smooth,
                },
            ],
        });
        let landed = e.retime_keys_at(i, 0, 0.5, 0.9).expect("keys at 0.5");
        assert!(
            landed < 0.6 && landed > 0.59,
            "stopped at Opacity's neighbour: {landed}"
        );
        let xs = x_keys(&e, i);
        assert!((xs[1].0 - landed).abs() < 1e-6, "X moved with it");
        assert_eq!(e.retime_keys_at(i, 0, 3.0, 4.0), None, "nothing there");
        e.end_gesture();
        e.undo();
        assert_eq!(x_keys(&e, i)[1].0, 0.5);
    }

    /// A key added on the line lands on the curve's own value, so the
    /// motion is unchanged until it is dragged; the last key deleted
    /// takes the track away.
    #[test]
    fn keys_add_on_the_line_and_delete_down_to_nothing() {
        let (mut e, i) = keyed();
        let x = Target::Shape(Prop::X);
        let before = e
            .clip_anim(i, 0)
            .unwrap()
            .track(x)
            .unwrap()
            .sample(0.25)
            .unwrap();
        let k = e.add_key(i, 0, x, 0.25).expect("added");
        assert_eq!(k, 1);
        let keys = x_keys(&e, i);
        assert_eq!(keys.len(), 4);
        assert!((keys[1].1 - before).abs() < 1e-4, "on the line");
        assert_eq!(e.add_key(i, 0, x, 0.25), None, "not twice");
        for _ in 0..4 {
            assert!(e.delete_key(i, 0, x, 0));
        }
        assert!(e.clip_anim(i, 0).unwrap().track(x).is_none(), "track gone");
        assert!(!e.delete_key(i, 0, x, 0));
        e.undo();
        assert_eq!(x_keys(&e, i).len(), 1, "undo brings the last key back");
    }

    /// A setting with no curve yet gets one from a double-click: its
    /// first key holds the object's value as it stands.
    #[test]
    fn a_first_key_lands_on_the_objects_value() {
        let (mut e, i) = keyed();
        let op = Target::Shape(Prop::Opacity);
        assert!(e.clip_anim(i, 0).unwrap().track(op).is_none());
        e.set_time(0.5);
        e.sync_to_time();
        e.set_prop(Prop::Opacity, 0.4);
        let k = e.add_key(i, 0, op, 0.5).expect("a fresh track");
        assert_eq!(k, 0);
        let tr = e.clip_anim(i, 0).unwrap().track(op).unwrap();
        assert_eq!(tr.keys.len(), 1);
        assert!((tr.keys[0].v - 0.4).abs() < 1e-5, "the value on screen");
        e.undo();
        assert!(
            e.clip_anim(i, 0).unwrap().track(op).is_none(),
            "undo takes the track"
        );
    }

    /// Delete at a moment clears every track's key there.
    #[test]
    fn a_moment_deletes_across_every_track() {
        let (mut e, i) = keyed();
        e.clip_anim_mut(i, 0).tracks.push(crate::anim::Track {
            target: Target::Shape(Prop::Opacity),
            keys: vec![Key {
                t: 0.5,
                v: 1.0,
                ease: Ease::Smooth,
            }],
        });
        assert!(e.delete_keys_at(i, 0, 0.5));
        assert_eq!(x_keys(&e, i).len(), 2);
        assert!(
            e.clip_anim(i, 0)
                .unwrap()
                .track(Target::Shape(Prop::Opacity))
                .is_none()
        );
        assert!(!e.delete_keys_at(i, 0, 0.5));
    }

    /// Ease flips and flips back; a linear key is a straight line.
    #[test]
    fn ease_toggles_and_undoes() {
        let (mut e, i) = keyed();
        let x = Target::Shape(Prop::X);
        assert!(e.toggle_key_ease(i, 0, x, 0));
        let tr = e.clip_anim(i, 0).unwrap().track(x).unwrap();
        assert_eq!(tr.keys[0].ease, Ease::Linear);
        assert!((tr.sample(0.25).unwrap() - 450.0).abs() < 1e-3);
        assert!(e.toggle_key_ease(i, 0, x, 0));
        assert_eq!(
            e.clip_anim(i, 0).unwrap().track(x).unwrap().keys[0].ease,
            Ease::Smooth
        );
        e.undo();
        assert_eq!(
            e.clip_anim(i, 0).unwrap().track(x).unwrap().keys[0].ease,
            Ease::Linear
        );
    }

    /// A transform key goes where the drag puts it — the old Z ceiling
    /// snapped a key the gizmo had placed at 2800 back down to 1400 the
    /// moment it was touched.
    #[test]
    fn transform_keys_have_no_walls() {
        let (mut e, i) = keyed();
        let z = Target::Shape(Prop::Z);
        e.set_time(0.5);
        e.sync_to_time();
        e.set_prop(Prop::Z, 2800.0);
        assert!(e.stamp_keys(Some((i, &[z])), false));
        assert!(e.move_key(i, 0, z, 0, 0.5, 2800.0 + 100.0));
        let v = e.clip_anim(i, 0).unwrap().track(z).unwrap().keys[0].v;
        assert_eq!(v, 2900.0);
        assert!(e.move_key(i, 0, Target::Shape(Prop::X), 0, 0.0, -500.0));
        let x = e
            .clip_anim(i, 0)
            .unwrap()
            .track(Target::Shape(Prop::X))
            .unwrap()
            .keys[0]
            .v;
        assert_eq!(x, -500.0, "off the canvas is a place");
    }

    /// Effect parameters fit their declared range on a drag.
    #[test]
    fn effect_keys_fit_their_range() {
        let (mut e, i) = keyed();
        e.select(Some(i));
        assert!(e.add_effect(crate::fx::EffectKind::React));
        let id = e
            .fx_of(i)
            .find_kind(crate::fx::EffectKind::React)
            .unwrap()
            .id;
        e.set_time(0.0);
        e.sync_to_time();
        e.set_effect_param(i, id, 0, 0.5);
        assert!(e.stamp_key());
        let tg = Target::Effect { id, param: 0 };
        assert!(e.move_key(i, 0, tg, 0, 0.0, 99.0));
        let v = e.clip_anim(i, 0).unwrap().track(tg).unwrap().keys[0].v;
        assert_eq!(v, 20.0, "clamped to React's ceiling");
    }
}
