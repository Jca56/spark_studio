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

mod copy;
mod group;

pub use copy::{KeyClip, KeySpan};

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
                    ease: Ease::Linear,
                }],
            }),
        }
        let at = anim
            .track(target)
            .and_then(|tr| tr.keys.iter().position(|k| (k.t - t).abs() < KEY_EPS));
        self.unpose(i);
        at
    }

    /// Re-stamp key `k` of `target` at the setting's value as it stands
    /// — `K` with a key picked in the clip view updates *that* key rather
    /// than planting one at the playhead (Alva, 2026-09-01: type X in
    /// its box, press K, the picked key takes it). Its own undo step;
    /// false when the key already holds the value.
    pub fn restamp_key(&mut self, i: usize, c: usize, target: Target, k: usize) -> bool {
        let Some(v) = Self::read(&self.shapes[i], &self.fx[i], target) else {
            return false;
        };
        let Some(key) = self
            .clip_anim(i, c)
            .and_then(|a| a.track(target))
            .and_then(|tr| tr.keys.get(k))
            .copied()
        else {
            return false;
        };
        if (key.v - v).abs() < 1e-6 {
            return false;
        }
        self.push_keys();
        if let Some(key) = self.clips[i][c]
            .anim
            .track_mut(target)
            .and_then(|tr| tr.keys.get_mut(k))
        {
            key.v = v;
        }
        self.unpose(i);
        true
    }

    /// Every key at moment `t`, re-stamped at its setting's value as it
    /// stands — `K` with a moment picked on the strip. Undoable; false
    /// when nothing changed.
    pub fn restamp_keys_at(&mut self, i: usize, c: usize, t: f32) -> bool {
        let Some(anim) = self.clip_anim(i, c) else {
            return false;
        };
        let updates: Vec<(Target, usize, f32)> = anim
            .tracks
            .iter()
            .filter_map(|tr| {
                let k = tr.keys.iter().position(|k| (k.t - t).abs() < KEY_EPS)?;
                let v = Self::read(&self.shapes[i], &self.fx[i], tr.target)?;
                ((tr.keys[k].v - v).abs() > 1e-6).then_some((tr.target, k, v))
            })
            .collect();
        if updates.is_empty() {
            return false;
        }
        self.push_keys();
        for (target, k, v) in updates {
            if let Some(key) = self.clips[i][c]
                .anim
                .track_mut(target)
                .and_then(|tr| tr.keys.get_mut(k))
            {
                key.v = v;
            }
        }
        self.unpose(i);
        true
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
}

#[cfg(test)]
mod tests;
