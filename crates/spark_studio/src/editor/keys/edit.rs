//! Editing keys that already exist: the keyframe clipboard, retiming and
//! copying a selected group, deletion, and the smooth/linear flip.
//!
//! Everything here works on `(Owner, time)` pairs — a lane's own address for
//! a key — and every one of them is all-or-nothing: a move that would land a
//! key on top of one outside the moving set is refused rather than silently
//! merging the two. Stamping and evaluation live in the parent module.

use super::Editor;
use crate::anim::{self, Ease, KEY_EPS, Key, KeyClip, Owner, Track};
use crate::history::Tag;

impl Editor {
    /// Ctrl+C on selected lane keys: copy each key's full property stamp,
    /// keeping relative timing. The keyframe clipboard and the style
    /// clipboard share Ctrl+V — the most recent copy wins.
    pub fn copy_keys_multi(&mut self, keys: &[(Owner, f32)]) -> bool {
        let base = keys.iter().map(|&(_, t)| t).fold(f32::INFINITY, f32::min);
        let mut out = Vec::new();
        let mut last = base;
        for &(i, t) in keys {
            let Some(a) = self.owner_anim(i) else {
                continue;
            };
            let entries: Vec<_> = a
                .tracks
                .iter()
                .filter_map(|tr| {
                    tr.keys
                        .iter()
                        .find(|k| (k.t - t).abs() < KEY_EPS)
                        .map(|k| (tr.target, k.v, k.ease))
                })
                .collect();
            if !entries.is_empty() {
                out.push((i, t - base, entries));
                last = last.max(t);
            }
        }
        if out.is_empty() {
            return false;
        }
        println!("copied {} keyframe(s)", out.len());
        self.key_clip = Some(KeyClip {
            keys: out,
            span: last - base,
            base,
        });
        self.style_clip = None;
        false
    }

    pub fn has_key_clip(&self) -> bool {
        self.key_clip.is_some()
    }

    /// The clipboard's (span, absolute base time), for repeat-paste math.
    pub fn key_clip_shape(&self) -> Option<(f32, f32)> {
        self.key_clip.as_ref().map(|c| (c.span, c.base))
    }

    /// Ctrl+V: stamp the clipboard once per base time (one base = a plain
    /// paste, several = repeat-paste), overwriting co-timed keys. Keys past
    /// `max_t` are dropped. Returns the pasted keys for selection.
    pub fn paste_keys(&mut self, bases: &[f32], max_t: f32) -> Option<Vec<(Owner, f32)>> {
        let clip = self.key_clip.clone()?;
        let before = self.snap();
        self.history.push(before);
        let mut pasted = Vec::new();
        for &base in bases {
            for (owner, rel, entries) in &clip.keys {
                let at = base + rel;
                if at > max_t + KEY_EPS || self.owner_anim(*owner).is_none() {
                    continue;
                }
                for &(target, v, ease) in entries {
                    if !owner.animates(target) {
                        continue;
                    }
                    let Some(a) = self.owner_anim_mut(*owner) else {
                        continue;
                    };
                    match a.track_mut(target) {
                        Some(track) => {
                            track.upsert(at, v);
                            if let Some(k) =
                                track.keys.iter_mut().find(|k| (k.t - at).abs() < KEY_EPS)
                            {
                                k.ease = ease;
                            }
                        }
                        None => a.tracks.push(Track {
                            target,
                            keys: vec![Key { t: at, v, ease }],
                        }),
                    }
                }
                pasted.push((*owner, at));
            }
        }
        let cur = self.snap();
        self.history.drop_noop(&cur);
        if pasted.is_empty() {
            println!("paste: nothing landed");
            return None;
        }
        println!("pasted {} keyframe(s)", pasted.len());
        Some(pasted)
    }

    /// Slide every listed key by `dt`, all-or-nothing: refused when any
    /// destination collides with a key outside the moving set. One undo
    /// step per drag.
    pub fn retime_group(&mut self, keys: &[(Owner, f32)], dt: f32) -> bool {
        if keys.is_empty() || dt.abs() < KEY_EPS {
            return false;
        }
        for &(i, t) in keys {
            let Some(a) = self.owner_anim(i) else {
                return false;
            };
            let to = t + dt;
            for track in &a.tracks {
                if track
                    .keys
                    .iter()
                    .any(|k| (k.t - to).abs() < KEY_EPS && !anim::key_list_has(keys, i, k.t))
                {
                    return false;
                }
            }
        }
        let s = self.snap();
        self.history.change(Tag::Keys, s);
        // One pass per track against the shape's whole moving set. Walking
        // the (shape, time) pairs instead would let a key moved by an early
        // pair match a later one and slide twice — collapsing keys spaced
        // exactly `dt` apart, which the 16th grid makes the common case.
        let mut owners: Vec<Owner> = keys.iter().map(|&(i, _)| i).collect();
        owners.dedup();
        for i in owners {
            let times: Vec<f32> = keys
                .iter()
                .filter(|&&(j, _)| j == i)
                .map(|&(_, t)| t)
                .collect();
            let Some(a) = self.owner_anim_mut(i) else {
                continue;
            };
            for track in &mut a.tracks {
                for k in &mut track.keys {
                    if times.iter().any(|&t| (k.t - t).abs() < KEY_EPS) {
                        k.t += dt;
                    }
                }
                track.keys.sort_by(|a, b| a.t.total_cmp(&b.t));
            }
        }
        true
    }

    /// Copy every listed key to `+dt` (Alt+drag of a group), all-or-nothing.
    pub fn copy_group(&mut self, keys: &[(Owner, f32)], dt: f32) -> bool {
        if keys.is_empty() || dt.abs() < KEY_EPS {
            return false;
        }
        for &(i, t) in keys {
            let Some(a) = self.owner_anim(i) else {
                return false;
            };
            let to = t + dt;
            for track in &a.tracks {
                if track.keys.iter().any(|k| (k.t - to).abs() < KEY_EPS) {
                    return false;
                }
            }
        }
        let s = self.snap();
        self.history.change(Tag::Keys, s);
        for &(i, t) in keys {
            let Some(a) = self.owner_anim_mut(i) else {
                continue;
            };
            for track in &mut a.tracks {
                let Some(src) = track
                    .keys
                    .iter()
                    .find(|k| (k.t - t).abs() < KEY_EPS)
                    .copied()
                else {
                    continue;
                };
                let at = track.keys.partition_point(|k| k.t < t + dt);
                track.keys.insert(at, Key { t: t + dt, ..src });
            }
        }
        println!("copied {} keyframe(s)", keys.len());
        true
    }

    /// Delete every listed key in one undo step.
    pub fn delete_keys_group(&mut self, keys: &[(Owner, f32)]) -> bool {
        if keys.is_empty() {
            return false;
        }
        let s = self.snap();
        self.history.push(s);
        for &(i, t) in keys {
            let Some(a) = self.owner_anim_mut(i) else {
                continue;
            };
            for track in &mut a.tracks {
                track.keys.retain(|k| (k.t - t).abs() >= KEY_EPS);
            }
            a.prune_empty();
        }
        let cur = self.snap();
        self.history.drop_noop(&cur);
        println!("deleted {} keyframe(s)", keys.len());
        true
    }

    /// The shape's nearest key time strictly after (or before) `t` — the
    /// playhead's next stop for , / . navigation.
    pub fn adjacent_key(&self, i: Owner, t: f32, forward: bool) -> Option<f32> {
        let times = self.owner_anim(i)?.key_times();
        if forward {
            times.iter().map(|&(kt, _)| kt).find(|&kt| kt > t + KEY_EPS)
        } else {
            times
                .iter()
                .rev()
                .map(|&(kt, _)| kt)
                .find(|&kt| kt < t - KEY_EPS)
        }
    }

    /// Keys stamped before `min` (the first bar) predate the bar-1 origin —
    /// pull them up to it so nothing hides behind the timeline sidebar.
    /// Runs once per audio load, outside undo.
    pub fn clamp_keys_to(&mut self, min: f32) {
        let mut moved = 0usize;
        let folder_anims = self.folders.iter_mut().map(|f| &mut f.anim);
        for a in self.anim.iter_mut().chain(folder_anims) {
            for track in &mut a.tracks {
                for k in &mut track.keys {
                    if k.t < min - KEY_EPS {
                        k.t = min;
                        moved += 1;
                    }
                }
                track.keys.sort_by(|x, y| x.t.total_cmp(&y.t));
                track.keys.dedup_by(|b, a| (a.t - b.t).abs() < KEY_EPS);
            }
        }
        if moved > 0 {
            println!("pulled {moved} pre-bar-1 key(s) up to bar 1");
        }
    }

    /// Right-click on a lane marker: delete every key at that time.
    pub fn delete_keys_at(&mut self, i: Owner, t: f32) -> bool {
        let Some(a) = self.owner_anim(i) else {
            return false;
        };
        if !a
            .tracks
            .iter()
            .any(|tr| tr.keys.iter().any(|k| (k.t - t).abs() < KEY_EPS))
        {
            return false;
        }
        let s = self.snap();
        self.history.push(s);
        if let Some(a) = self.owner_anim_mut(i) {
            for track in &mut a.tracks {
                track.keys.retain(|k| (k.t - t).abs() >= KEY_EPS);
            }
            a.prune_empty();
        }
        println!("deleted key @ {t:.2}s");
        true
    }

    /// Ctrl+click on a lane marker: flip every co-timed key between smooth
    /// and linear (diamond ↔ square).
    pub fn toggle_ease_at(&mut self, i: Owner, t: f32) -> bool {
        let Some(a) = self.owner_anim(i) else {
            return false;
        };
        let Some(cur) = a.tracks.iter().find_map(|tr| {
            tr.keys
                .iter()
                .find(|k| (k.t - t).abs() < KEY_EPS)
                .map(|k| k.ease)
        }) else {
            return false;
        };
        let next = match cur {
            Ease::Smooth => Ease::Linear,
            Ease::Linear => Ease::Smooth,
        };
        let s = self.snap();
        self.history.push(s);
        if let Some(a) = self.owner_anim_mut(i) {
            for track in &mut a.tracks {
                for k in &mut track.keys {
                    if (k.t - t).abs() < KEY_EPS {
                        k.ease = next;
                    }
                }
            }
        }
        println!("key ease: {next:?} @ {t:.2}s");
        true
    }
}
