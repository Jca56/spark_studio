//! Several keys at once — what a Shift-click, a band drag or Ctrl+A
//! picks in the clip view (Alva, 2026-09-01: "I can't highlight/select
//! more than one keyframe at a time!"). A set is `(target, index)`
//! pairs; the verbs here move, delete, ease and re-stamp the whole set
//! as one, and a single key's value can be set outright (the menu's
//! value box).
//!
//! A moved set never crosses the keys it leaves behind: the drag is
//! clamped so no picked key reaches an *unpicked* neighbour on its own
//! track — picked neighbours travel together, so they can't collide.
//! Indices stay valid through a move for the same reason.

use super::Editor;
use crate::anim::{Ease, KEY_EPS, Target};

/// The least a moved key may sit from a neighbour it isn't travelling
/// with — the single-key drag's own rule.
const APART: f32 = KEY_EPS * 2.0;

/// The keys in `set` on `target`, by index, ascending.
fn on_track(set: &[(Target, usize)], target: Target) -> Vec<usize> {
    let mut ks: Vec<usize> = set
        .iter()
        .filter(|(t, _)| *t == target)
        .map(|(_, k)| *k)
        .collect();
    ks.sort_unstable();
    ks.dedup();
    ks
}

impl Editor {
    /// Move every key in `set` by `dt` seconds and `dv` units, as one —
    /// clamped so none crosses an unpicked neighbour on its track, and
    /// each value fitted to its target. Coalesces into one undo step
    /// per drag. False when nothing moved.
    pub fn shift_keys(&mut self, i: usize, c: usize, set: &[(Target, usize)], dt: f32, dv: f32) -> bool {
        let Some(anim) = self.clip_anim(i, c) else {
            return false;
        };
        // How far the set may travel in time before a picked key meets
        // an unpicked one: the tightest window across every track.
        let (mut lo, mut hi) = (f32::MIN, f32::MAX);
        let mut any = false;
        for tr in &anim.tracks {
            let picked = on_track(set, tr.target);
            for &k in &picked {
                let Some(key) = tr.keys.get(k) else {
                    continue;
                };
                any = true;
                let before = (0..k).rev().find(|p| !picked.contains(p));
                let after = (k + 1..tr.keys.len()).find(|n| !picked.contains(n));
                let floor = before.map(|p| tr.keys[p].t + APART).unwrap_or(0.0);
                lo = lo.max(floor - key.t);
                if let Some(n) = after {
                    hi = hi.min(tr.keys[n].t - APART - key.t);
                }
            }
        }
        if !any {
            return false;
        }
        let dt = dt.clamp(lo.min(hi), hi.max(lo));
        if dt.abs() < 1e-6 && dv.abs() < 1e-6 {
            return false;
        }
        self.record_keys();
        let targets: Vec<Target> = self.clips[i][c].anim.tracks.iter().map(|t| t.target).collect();
        for target in targets {
            let picked = on_track(set, target);
            if picked.is_empty() {
                continue;
            }
            let fits: Vec<(usize, f32)> = picked
                .iter()
                .filter_map(|&k| {
                    let key = self.clips[i][c].anim.track(target)?.keys.get(k)?;
                    Some((k, self.fit_value(i, target, key.v + dv)))
                })
                .collect();
            if let Some(tr) = self.clips[i][c].anim.track_mut(target) {
                for (k, v) in fits {
                    if let Some(key) = tr.keys.get_mut(k) {
                        key.t = (key.t + dt).max(0.0);
                        key.v = v;
                    }
                }
            }
        }
        self.unpose(i);
        true
    }

    /// Delete every key in `set`. Tracks left empty go with their last
    /// key. One undo step; false when none of them existed.
    pub fn delete_keys(&mut self, i: usize, c: usize, set: &[(Target, usize)]) -> bool {
        let Some(anim) = self.clip_anim(i, c) else {
            return false;
        };
        let any = set
            .iter()
            .any(|&(t, k)| anim.track(t).is_some_and(|tr| k < tr.keys.len()));
        if !any {
            return false;
        }
        self.push_keys();
        let targets: Vec<Target> = self.clips[i][c].anim.tracks.iter().map(|t| t.target).collect();
        for target in targets {
            let picked = on_track(set, target);
            if let Some(tr) = self.clips[i][c].anim.track_mut(target) {
                // Highest first, so each removal leaves the lower indices true.
                for &k in picked.iter().rev() {
                    if k < tr.keys.len() {
                        tr.keys.remove(k);
                    }
                }
            }
        }
        self.clips[i][c].anim.prune_empty();
        self.unpose(i);
        true
    }

    /// Delete every key on the clip — Ctrl+X with nothing picked, after
    /// the copy has taken them. One undo step; false on an empty clip.
    pub fn clear_keys(&mut self, i: usize, c: usize) -> bool {
        if !self.clip_anim(i, c).is_some_and(|a| a.has_keys()) {
            return false;
        }
        self.push_keys();
        self.clips[i][c].anim.tracks.clear();
        self.unpose(i);
        true
    }

    /// Give every key in `set` the same ease. One undo step; false when
    /// they all had it already.
    pub fn set_keys_ease(&mut self, i: usize, c: usize, set: &[(Target, usize)], ease: Ease) -> bool {
        let Some(anim) = self.clip_anim(i, c) else {
            return false;
        };
        let changes = set
            .iter()
            .any(|&(t, k)| anim.track(t).and_then(|tr| tr.keys.get(k)).is_some_and(|key| key.ease != ease));
        if !changes {
            return false;
        }
        self.push_keys();
        for &(t, k) in set {
            if let Some(key) = self.clips[i][c].anim.track_mut(t).and_then(|tr| tr.keys.get_mut(k)) {
                key.ease = ease;
            }
        }
        self.unpose(i);
        true
    }

    /// The ease the set shares, if it shares one.
    pub fn keys_ease(&self, i: usize, c: usize, set: &[(Target, usize)]) -> Option<Ease> {
        let anim = self.clip_anim(i, c)?;
        let mut eases = set
            .iter()
            .filter_map(|&(t, k)| anim.track(t)?.keys.get(k).map(|key| key.ease));
        let first = eases.next()?;
        eases.all(|e| e == first).then_some(first)
    }

    /// Re-stamp every key in `set` at its setting's value as it stands —
    /// `K` with a set picked. One undo step; false when nothing changed.
    pub fn restamp_keys(&mut self, i: usize, c: usize, set: &[(Target, usize)]) -> bool {
        let Some(anim) = self.clip_anim(i, c) else {
            return false;
        };
        let updates: Vec<(Target, usize, f32)> = set
            .iter()
            .filter_map(|&(t, k)| {
                let key = anim.track(t)?.keys.get(k)?;
                let v = Self::read(&self.shapes[i], &self.fx[i], t)?;
                ((key.v - v).abs() > 1e-6).then_some((t, k, v))
            })
            .collect();
        if updates.is_empty() {
            return false;
        }
        self.push_keys();
        for (t, k, v) in updates {
            if let Some(key) = self.clips[i][c].anim.track_mut(t).and_then(|tr| tr.keys.get_mut(k)) {
                key.v = v;
            }
        }
        self.unpose(i);
        true
    }

    /// Set key `k`'s value outright — the menu's value box. Fitted to
    /// the target; its own undo step; false when it already held it.
    pub fn set_key_value(&mut self, i: usize, c: usize, target: Target, k: usize, v: f32) -> bool {
        let v = self.fit_value(i, target, v);
        let Some(old) = self
            .clip_anim(i, c)
            .and_then(|a| a.track(target))
            .and_then(|tr| tr.keys.get(k))
            .map(|key| key.v)
        else {
            return false;
        };
        if (old - v).abs() < 1e-6 {
            return false;
        }
        self.push_keys();
        if let Some(key) = self.clips[i][c].anim.track_mut(target).and_then(|tr| tr.keys.get_mut(k)) {
            key.v = v;
        }
        self.unpose(i);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::anim::{Key, Track};
    use crate::props::{Prop, Tool};

    /// A circle whose X has keys at 0, 0.5, 1.0, 1.5 (0, 100, 200, 300)
    /// and whose Y has one at 0.5.
    fn keyed() -> (Editor, usize, Target, Target) {
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
        let (x, y) = (Target::Shape(Prop::X), Target::Shape(Prop::Y));
        let key = |t: f32, v: f32| Key { t, v, ease: Ease::Linear };
        e.clip_anim_mut(i, 0).tracks.push(Track {
            target: x,
            keys: vec![key(0.0, 0.0), key(0.5, 100.0), key(1.0, 200.0), key(1.5, 300.0)],
        });
        e.clip_anim_mut(i, 0).tracks.push(Track {
            target: y,
            keys: vec![key(0.5, 50.0)],
        });
        (e, i, x, y)
    }

    fn times(e: &Editor, i: usize, t: Target) -> Vec<f32> {
        e.clip_anim(i, 0).unwrap().track(t).map(|tr| tr.keys.iter().map(|k| k.t).collect()).unwrap_or_default()
    }

    /// A set moves together in time and value, stops where a picked key
    /// would meet an unpicked one, and undoes as one step.
    #[test]
    fn a_set_moves_as_one_and_stops_at_the_keys_it_leaves() {
        let (mut e, i, x, y) = keyed();
        let set = [(x, 1), (x, 2), (y, 0)];
        assert!(e.shift_keys(i, 0, &set, 0.2, 10.0));
        assert_eq!(times(&e, i, x), vec![0.0, 0.7, 1.2, 1.5]);
        assert_eq!(times(&e, i, y), vec![0.7]);
        let vals: Vec<f32> = e.clip_anim(i, 0).unwrap().track(x).unwrap().keys.iter().map(|k| k.v).collect();
        assert_eq!(vals, vec![0.0, 110.0, 210.0, 300.0]);
        // Asked past the key at 1.5, the set stops just short of it — the
        // picked pair keeps its own spacing.
        assert!(e.shift_keys(i, 0, &set, 5.0, 0.0));
        let t = times(&e, i, x);
        assert!(t[2] < 1.5 && t[2] > 1.49, "stopped at {}", t[2]);
        assert!((t[2] - t[1] - 0.5).abs() < 1e-5, "the pair kept its spacing");
        e.end_gesture();
        assert!(e.undo());
        assert_eq!(times(&e, i, x), vec![0.0, 0.5, 1.0, 1.5], "one drag, one undo");
        // Nothing in the set: nothing to do.
        assert!(!e.shift_keys(i, 0, &[(x, 9)], 1.0, 0.0));
    }

    /// Deleting a set takes every key in it whatever order they were
    /// picked in; easing a set eases them all; the shared ease is known
    /// only when it is shared; a re-stamp writes the value as it stands;
    /// a value can be set outright.
    #[test]
    fn a_set_deletes_eases_and_restamps_together() {
        let (mut e, i, x, y) = keyed();
        assert_eq!(e.keys_ease(i, 0, &[(x, 0), (x, 1)]), Some(Ease::Linear));
        assert!(e.set_keys_ease(i, 0, &[(x, 1), (y, 0)], Ease::Smooth));
        assert_eq!(e.keys_ease(i, 0, &[(x, 1), (y, 0)]), Some(Ease::Smooth));
        assert_eq!(e.keys_ease(i, 0, &[(x, 0), (x, 1)]), None, "mixed");
        assert!(!e.set_keys_ease(i, 0, &[(x, 1)], Ease::Smooth), "already smooth");
        assert!(e.set_key_value(i, 0, x, 1, 150.0));
        assert_eq!(e.clip_anim(i, 0).unwrap().track(x).unwrap().keys[1].v, 150.0);
        assert!(!e.set_key_value(i, 0, x, 1, 150.0));
        // The object stands at X 300 (its draw): a re-stamp of key 0 takes it.
        assert!(e.restamp_keys(i, 0, &[(x, 0)]));
        assert_eq!(e.clip_anim(i, 0).unwrap().track(x).unwrap().keys[0].v, 300.0);
        // Delete the middle two of X and Y's only key, picked out of order.
        assert!(e.delete_keys(i, 0, &[(y, 0), (x, 2), (x, 1)]));
        assert_eq!(times(&e, i, x), vec![0.0, 1.5]);
        assert!(e.clip_anim(i, 0).unwrap().track(y).is_none(), "an empty track never persists");
        assert!(e.undo());
        assert_eq!(times(&e, i, x).len(), 4);
        assert!(!e.delete_keys(i, 0, &[(x, 40)]));
    }
}
