//! Manual keyframing and playhead evaluation.
//!
//! Keys appear only when the Keyframe button (or `K`) stamps the selected
//! shapes' pose at the playhead. Editing a keyed shape between stamps is a
//! *preview*: the pose holds while you look at it, and reverts to the
//! curves the moment the playhead moves. Stamp it or lose it.

use super::Editor;
use crate::anim::{self, Ease, KEY_EPS, Key, KeyClip, Owner, ShapeAnim, Track};
use crate::history::Tag;
use crate::props::Prop;

impl Editor {
    /// The curves belonging to a lane owner — a shape's, or a folder
    /// transform's. The single lookup every key operation shares, so shapes
    /// and folders can never drift apart in what's supported.
    pub(crate) fn owner_anim(&self, o: Owner) -> Option<&ShapeAnim> {
        match o {
            Owner::Shape(i) => self.anim.get(i),
            Owner::Folder(id) => self.folder(id).map(|f| &f.anim),
        }
    }

    pub(crate) fn owner_anim_mut(&mut self, o: Owner) -> Option<&mut ShapeAnim> {
        match o {
            Owner::Shape(i) => self.anim.get_mut(i),
            Owner::Folder(id) => self
                .folders
                .iter_mut()
                .find(|f| f.id == id)
                .map(|f| &mut f.anim),
        }
    }

    /// Every owner that currently exists, top of stack first — the order the
    /// lanes list them in.
    pub fn key_owners(&self) -> Vec<Owner> {
        let mut out = Vec::new();
        let mut i = self.shapes.len();
        while i > 0 {
            i -= 1;
            let f = self.folder_of(i);
            if f == 0 {
                out.push(Owner::Shape(i));
                continue;
            }
            let members = self.folder_members(f);
            out.push(Owner::Folder(f));
            out.extend(members.iter().rev().map(|&m| Owner::Shape(m)));
            i = members.first().copied().unwrap_or(i);
        }
        out
    }

    /// The playhead time all evaluation and stamping happens at. Moving it
    /// discards any un-stamped pose.
    pub fn set_time(&mut self, t: f32) {
        if (t - self.time).abs() > 1e-4 {
            self.posed.clear();
            self.posed_folders.clear();
        }
        self.time = t;
    }

    pub fn time(&self) -> f32 {
        self.time
    }

    /// Ctrl+C on selected lane keys: copy each key's full property stamp,
    /// keeping relative timing. The keyframe clipboard and the style
    /// clipboard share Ctrl+V — the most recent copy wins.
    pub fn copy_keys_multi(&mut self, keys: &[(Owner, f32)]) -> bool {
        let base = keys.iter().map(|&(_, t)| t).fold(f32::INFINITY, f32::min);
        let mut out = Vec::new();
        let mut last = base;
        for &(i, t) in keys {
            let Some(a) = self.owner_anim(i) else { continue };
            let entries: Vec<_> = a
                .tracks
                .iter()
                .filter_map(|tr| {
                    tr.keys
                        .iter()
                        .find(|k| (k.t - t).abs() < KEY_EPS)
                        .map(|k| (tr.prop, k.v, k.ease))
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
                for &(prop, v, ease) in entries {
                    if !owner.animates(prop) {
                        continue;
                    }
                    let Some(a) = self.owner_anim_mut(*owner) else {
                        continue;
                    };
                    match a.track_mut(prop) {
                        Some(track) => {
                            track.upsert(at, v);
                            if let Some(k) =
                                track.keys.iter_mut().find(|k| (k.t - at).abs() < KEY_EPS)
                            {
                                k.ease = ease;
                            }
                        }
                        None => a.tracks.push(Track {
                            prop,
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
            let Some(a) = self.owner_anim_mut(i) else { continue };
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
            let Some(a) = self.owner_anim_mut(i) else { continue };
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

    /// Pose every keyed shape at the playhead. Keyed properties' base values
    /// are dead storage (the curve always wins), so baking the pose into the
    /// document is lossless — and it means selection, picking, handles, and
    /// the inspector all see animated values for free. Shapes holding an
    /// un-stamped pose are left alone so the preview doesn't snap back.
    pub fn sync_to_time(&mut self) {
        for (i, (shape, a)) in self.shapes.iter_mut().zip(&self.anim).enumerate() {
            if !self.posed.contains(&i) {
                a.apply(shape, self.time);
            }
        }
        self.sync_folders_to_time();
    }

    /// After-edit hook: a keyed shape was changed by hand, so its current
    /// values are a preview overriding the curves until the playhead moves.
    pub(super) fn mark_posed(&mut self, indices: &[usize]) {
        for &i in indices {
            if self.anim.get(i).is_some_and(|a| a.has_keys()) && !self.posed.contains(&i) {
                self.posed.push(i);
            }
        }
    }

    pub(super) fn mark_posed_selection(&mut self) {
        let sel = self.selection.clone();
        self.mark_posed(&sel);
    }

    /// Shape indices changed meaning (delete/reorder/load), so every scrap of
    /// index-keyed scratch state is void: un-stamped poses and the Shift+click
    /// range anchor alike.
    pub(super) fn clear_posed(&mut self) {
        self.posed.clear();
        self.range_anchor = None;
    }

    /// Undo/redo and load replace the folder list wholesale, so any folder
    /// preview pose is void too.
    pub(super) fn clear_posed_folders(&mut self) {
        self.posed_folders.clear();
    }

    /// The Keyframe button: stamp every selected shape's full pose — all
    /// applicable properties — as keys at the playhead.
    pub fn stamp_key(&mut self) -> bool {
        if self.selection.is_empty() {
            println!("keyframe: nothing selected");
            return false;
        }
        let before = self.snap();
        self.history.push(before);
        let t = self.time;
        for &i in &self.selection.clone() {
            for prop in anim::PROP_ORDER {
                let Some(v) = anim::prop_value(&self.shapes[i], prop) else {
                    continue;
                };
                match self.anim[i].track_mut(prop) {
                    Some(track) => track.upsert(t, v),
                    None => self.anim[i].tracks.push(Track {
                        prop,
                        keys: vec![Key {
                            t,
                            v,
                            ease: Ease::Smooth,
                        }],
                    }),
                }
            }
            self.posed.retain(|&p| p != i);
        }
        // A folder whose whole run is selected keys its transform too —
        // that's what "stamp the current pose" means when the thing you
        // grabbed was the folder header.
        let whole: Vec<u32> = self
            .folders
            .iter()
            .map(|f| f.id)
            .filter(|&id| {
                let m = self.folder_members(id);
                !m.is_empty() && m.iter().all(|i| self.selection.contains(i))
            })
            .collect();
        for id in whole {
            let vals: Vec<(Prop, f32)> = [Prop::X, Prop::Y, Prop::Rotation, Prop::Scale]
                .into_iter()
                .filter_map(|p| self.folder(id).and_then(|f| f.prop(p)).map(|v| (p, v)))
                .collect();
            if let Some(f) = self.folders.iter_mut().find(|f| f.id == id) {
                for (prop, v) in vals {
                    match f.anim.track_mut(prop) {
                        Some(track) => track.upsert(t, v),
                        None => f.anim.tracks.push(Track {
                            prop,
                            keys: vec![Key {
                                t,
                                v,
                                ease: Ease::Smooth,
                            }],
                        }),
                    }
                }
            }
            self.posed_folders.retain(|&p| p != id);
        }
        // Stamping an unchanged pose over its own key is not an undo step.
        let cur = self.snap();
        self.history.drop_noop(&cur);
        println!("keyframe @ {:.2}s ({} shapes)", t, self.selection.len());
        true
    }

    /// Keyed-property bitmask for the inspector (see [`anim::prop_bit`]).
    pub fn keyed_mask(&self, i: usize) -> u16 {
        let Some(a) = self.anim.get(i) else { return 0 };
        a.tracks
            .iter()
            .filter(|t| !t.keys.is_empty())
            .fold(0, |m, t| m | anim::prop_bit(t.prop))
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::props::Prop;
    use spark_render::Shape;

    /// Shorthand: every fixture here keys shape 0.
    const S0: Owner = Owner::Shape(0);

    /// One shape carrying an X track keyed at `times`.
    fn keyed_at(times: &[f32]) -> Editor {
        let mut e = Editor::empty();
        e.shapes.push(Shape::circle([0.0, 0.0], 10.0));
        e.names.push(String::new());
        e.anim.push(ShapeAnim {
            tracks: vec![Track {
                prop: Prop::X,
                keys: times
                    .iter()
                    .map(|&t| Key {
                        t,
                        v: t * 100.0,
                        ease: Ease::Smooth,
                    })
                    .collect(),
            }],
        });
        e.react.push([1.0; 3]);
        e.group.push(0);
        e.hidden.push(false);
        e.folder.push(0);
        e
    }

    fn times(e: &Editor) -> Vec<f32> {
        e.anim[0].tracks[0].keys.iter().map(|k| k.t).collect()
    }

    #[test]
    fn retime_group_slides_each_key_once() {
        // Two keys a 16th apart dragged by exactly one 16th: the leading key
        // must not be caught a second time by the trailing key's pass.
        let mut e = keyed_at(&[1.0, 1.25]);
        assert!(e.retime_group(&[(S0, 1.0), (S0, 1.25)], 0.25));
        assert_eq!(times(&e), vec![1.25, 1.5]);
    }

    #[test]
    fn retime_group_refuses_collision_outside_the_set() {
        // 1.5 isn't moving, so sliding 1.0 onto it would silently merge them.
        let mut e = keyed_at(&[1.0, 1.5]);
        assert!(!e.retime_group(&[(S0, 1.0)], 0.5));
        assert_eq!(times(&e), vec![1.0, 1.5]);
    }

    #[test]
    fn retime_group_keeps_keys_sorted() {
        let mut e = keyed_at(&[1.0, 2.0, 3.0]);
        // Slide the earliest key past the other two.
        assert!(e.retime_group(&[(S0, 1.0)], 2.5));
        assert_eq!(times(&e), vec![2.0, 3.0, 3.5]);
    }
}

#[cfg(test)]
mod clipboard_tests {
    use super::*;
    use crate::props::Prop;
    use spark_render::Shape;

    fn one_keyed() -> Editor {
        let mut e = Editor::empty();
        let mut sh = Shape::circle([100.0, 200.0], 20.0);
        sh.set_center([100.0, 200.0]);
        e.shapes.push(sh);
        e.names.push(String::new());
        e.anim.push(ShapeAnim::default());
        e.react.push([1.0; 3]);
        e.group.push(0);
        e.hidden.push(false);
        e.folder.push(0);
        e.selection = vec![0];
        e.set_time(1.0);
        e.stamp_key();
        e
    }

    fn key_at(e: &Editor, o: Owner, prop: Prop, t: f32) -> Option<f32> {
        e.owner_anim(o)?
            .tracks
            .iter()
            .find(|tr| tr.prop == prop)?
            .keys
            .iter()
            .find(|k| (k.t - t).abs() < KEY_EPS)
            .map(|k| k.v)
    }

    #[test]
    fn copied_keys_paste_with_their_values() {
        let mut e = one_keyed();
        assert_eq!(key_at(&e, Owner::Shape(0), Prop::X, 1.0), Some(100.0));
        e.copy_keys_multi(&[(Owner::Shape(0), 1.0)]);
        let pasted = e.paste_keys(&[5.0], 60.0).expect("paste landed");
        assert_eq!(pasted, vec![(Owner::Shape(0), 5.0)]);
        // The pasted key must carry the copied value, not an empty stamp.
        assert_eq!(key_at(&e, Owner::Shape(0), Prop::X, 5.0), Some(100.0));
        assert_eq!(key_at(&e, Owner::Shape(0), Prop::Y, 5.0), Some(200.0));
    }

    #[test]
    fn paste_keeps_relative_timing_across_a_group() {
        let mut e = one_keyed();
        e.set_time(2.0);
        e.shapes[0].set_center([400.0, 200.0]);
        e.stamp_key();
        e.copy_keys_multi(&[(Owner::Shape(0), 1.0), (Owner::Shape(0), 2.0)]);
        e.paste_keys(&[10.0], 60.0).expect("paste landed");
        // 1s apart at the source, 1s apart at the destination.
        assert_eq!(key_at(&e, Owner::Shape(0), Prop::X, 10.0), Some(100.0));
        assert_eq!(key_at(&e, Owner::Shape(0), Prop::X, 11.0), Some(400.0));
    }

    #[test]
    fn paste_past_the_track_end_is_dropped() {
        let mut e = one_keyed();
        e.copy_keys_multi(&[(Owner::Shape(0), 1.0)]);
        assert!(e.paste_keys(&[99.0], 60.0).is_none());
    }
}
