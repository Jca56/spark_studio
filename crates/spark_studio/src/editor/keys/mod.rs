//! Manual keyframing and playhead evaluation: who owns curves, where the
//! playhead is, stamping a pose, and posing the document at a time.
//!
//! Keys appear only when the Keyframe button (or `K`) stamps the selected
//! shapes' pose at the playhead. Editing a keyed shape between stamps is a
//! *preview*: the pose holds while you look at it, and reverts to the
//! curves the moment the playhead moves. Stamp it or lose it.
//!
//! Editing keys that already exist — clipboard, retime, delete, ease — is
//! [`edit`](self::edit)'s job.

mod edit;

use super::Editor;
use crate::anim::{self, Ease, KEY_EPS, Key, Owner, ShapeAnim, Track};
use crate::props::Prop;

/// A folder transform's four animatable axes, in the order its baseline
/// (`Editor::folder_base`) stores them. [`crate::anim::Owner::animates`]
/// agrees with this list.
pub(crate) const FOLDER_PROPS: [Prop; 4] = [Prop::X, Prop::Y, Prop::Rotation, Prop::Scale];

impl Editor {
    /// The curves belonging to a lane owner — a shape's, or a folder
    /// transform's. The single lookup every key operation shares, so shapes
    /// and folders can never drift apart in what's supported.
    /// Both owner kinds address by id, so an owner that no longer exists
    /// resolves to `None` rather than to whatever shape took its slot.
    pub(crate) fn owner_anim(&self, o: Owner) -> Option<&ShapeAnim> {
        match o {
            Owner::Shape(id) => self.anim.get(self.index_of(id)?),
            Owner::Folder(id) => self.folder(id).map(|f| &f.anim),
        }
    }

    pub(crate) fn owner_anim_mut(&mut self, o: Owner) -> Option<&mut ShapeAnim> {
        match o {
            Owner::Shape(id) => {
                let i = self.index_of(id)?;
                self.anim.get_mut(i)
            }
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
                out.push(self.owner(i));
                continue;
            }
            let members = self.folder_members(f);
            out.push(Owner::Folder(f));
            out.extend(members.iter().rev().map(|&m| self.owner(m)));
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

    /// Pose every keyed shape at the playhead. Keyed properties' base values
    /// are dead storage (the curve always wins), so baking the pose into the
    /// document is lossless — and it means selection, picking, handles, and
    /// the inspector all see animated values for free. Shapes holding an
    /// un-stamped pose are left alone so the preview doesn't snap back.
    pub fn sync_to_time(&mut self) {
        // The baseline grows with the stack. A shape holding a preview pose
        // keeps whatever it had — that frozen entry is what `stamp_key`
        // diffs the hand pose against.
        if self.pose_base.len() != self.shapes.len() {
            self.pose_base.clear();
            self.pose_base.extend_from_slice(&self.shapes);
        }
        for (i, (shape, a)) in self.shapes.iter_mut().zip(&self.anim).enumerate() {
            if !self.posed.contains(&i) {
                a.apply(shape, self.time);
                self.pose_base[i] = *shape;
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
        // The stamping baseline is index-keyed too. An entry left behind
        // after a reorder describes some *other* shape, and diffing against
        // it would key properties nobody touched.
        self.pose_base.clear();
    }

    /// Undo/redo and load replace the folder list wholesale, so any folder
    /// preview pose is void too.
    pub(super) fn clear_posed_folders(&mut self) {
        self.posed_folders.clear();
        self.folder_base.clear();
    }

    /// The rule deciding what a stamp keys, shared by shapes and folder
    /// transforms.
    ///
    /// 1. **Nothing keyed yet** — lay down `first_pose`. A thing has to have
    ///    a pose before it can have a change.
    /// 2. **Something moved** — key exactly what moved. Stamping the whole
    ///    property list every time is what made one `K` freeze a shape
    ///    forever: afterwards glow, sides, thickness and the rest were all
    ///    curve-driven too, so posing by hand could only ever preview.
    /// 3. **Nothing moved** — *hold*: re-stamp what is already animated at
    ///    its current value. Pressing `K` twice without touching anything is
    ///    how you ask for stillness between two moments, and it would
    ///    otherwise do nothing at all.
    fn pick_props(keyed: Vec<Prop>, moved: Vec<Prop>, first_pose: Vec<Prop>) -> Vec<Prop> {
        if keyed.is_empty() {
            first_pose
        } else if moved.is_empty() {
            keyed
        } else {
            moved
        }
    }

    /// Stamp `props` into `anim` at `t`.
    ///
    /// `value` reads the current pose and `was` the baseline the curves held
    /// before the hand edit. A property earning its *first* track has
    /// nothing to move from — the change would read as a flat line — so it
    /// is anchored with a holding key at `prev`, the owner's previous key
    /// time, carrying its old value. That backfill is what makes "turn the
    /// glow up at bar 5 and press K" actually ramp instead of jumping.
    fn stamp_into(
        anim: &mut ShapeAnim,
        t: f32,
        prev: Option<f32>,
        props: &[Prop],
        value: impl Fn(Prop) -> Option<f32>,
        was: impl Fn(Prop) -> Option<f32>,
    ) {
        for &prop in props {
            let Some(v) = value(prop) else { continue };
            let fresh = anim.tracks.iter().all(|tr| tr.prop != prop);
            match anim.track_mut(prop) {
                Some(track) => track.upsert(t, v),
                None => anim.tracks.push(Track {
                    prop,
                    keys: vec![Key {
                        t,
                        v,
                        ease: Ease::Smooth,
                    }],
                }),
            }
            if fresh
                && let (Some(at), Some(then)) = (prev, was(prop))
                && let Some(track) = anim.track_mut(prop)
            {
                track.upsert(at, then);
            }
        }
    }

    /// The last key time strictly before `t` on an owner already carrying
    /// `times` — where a backfilled holding key lands.
    fn hold_time(times: &[(f32, Ease)], t: f32) -> Option<f32> {
        times
            .iter()
            .rev()
            .map(|&(kt, _)| kt)
            .find(|&kt| kt < t - KEY_EPS)
    }

    /// The Keyframe button: stamp what the hand actually changed, at the
    /// playhead. See [`Editor::pick_props`] for which properties that is.
    pub fn stamp_key(&mut self) -> bool {
        if self.selection.is_empty() {
            println!("keyframe: nothing selected");
            return false;
        }
        let before = self.snap();
        self.history.push(before);
        let t = self.time;
        // Which properties actually earned keys — now that a stamp is a
        // diff rather than a snapshot, "what did K just do" is a real
        // question and the terminal is where it gets answered.
        let mut landed: Vec<Prop> = Vec::new();
        for &i in &self.selection.clone() {
            // Read before mutating: the backfill anchors to where this shape
            // was last posed, which its own new keys would move.
            let prev = Self::hold_time(&self.anim[i].key_times(), t);
            let shape = self.shapes[i];
            let base = self.pose_base.get(i).copied();
            let keyed: Vec<Prop> = self.anim[i].tracks.iter().map(|tr| tr.prop).collect();
            // No baseline (the stack changed under us) means nothing counts
            // as moved, which falls through to the hold.
            let moved: Vec<Prop> = base
                .map(|was| {
                    anim::PROP_ORDER
                        .into_iter()
                        .filter(|&p| {
                            match (anim::prop_value(&shape, p), anim::prop_value(&was, p)) {
                                (Some(now), Some(then)) => anim::changed(p, now, then),
                                _ => false,
                            }
                        })
                        .collect()
                })
                .unwrap_or_default();
            let first: Vec<Prop> = anim::FIRST_POSE
                .into_iter()
                .filter(|&p| anim::prop_value(&shape, p).is_some())
                .collect();
            let props = Self::pick_props(keyed, moved, first);
            Self::stamp_into(
                &mut self.anim[i],
                t,
                prev,
                &props,
                |p| anim::prop_value(&shape, p),
                |p| base.and_then(|w| anim::prop_value(&w, p)),
            );
            for p in props {
                if !landed.contains(&p) {
                    landed.push(p);
                }
            }
            self.posed.retain(|&p| p != i);
        }
        // A folder whose whole run is selected keys its transform too —
        // that's what "stamp the current pose" means when the thing you
        // grabbed was the folder header. Same three-case rule.
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
            let base = self
                .folder_base
                .iter()
                .find(|(f, _)| *f == id)
                .map(|&(_, b)| b);
            let Some(f) = self.folders.iter_mut().find(|f| f.id == id) else {
                continue;
            };
            // Read the pose out before touching `f.anim`: the stamp takes a
            // mutable borrow of that field, so the closures can't also hold
            // the folder itself.
            let now: Vec<(Prop, f32)> = FOLDER_PROPS
                .into_iter()
                .filter_map(|p| f.prop(p).map(|v| (p, v)))
                .collect();
            let val = |p: Prop| now.iter().find(|(q, _)| *q == p).map(|&(_, v)| v);
            let was = |p: Prop| {
                let k = FOLDER_PROPS.iter().position(|q| *q == p)?;
                base.map(|b| b[k])
            };
            let prev = Self::hold_time(&f.anim.key_times(), t);
            let keyed: Vec<Prop> = f.anim.tracks.iter().map(|tr| tr.prop).collect();
            let moved: Vec<Prop> = FOLDER_PROPS
                .into_iter()
                .filter(|&p| match (val(p), was(p)) {
                    (Some(n), Some(then)) => anim::changed(p, n, then),
                    _ => false,
                })
                .collect();
            let props = Self::pick_props(keyed, moved, FOLDER_PROPS.to_vec());
            Self::stamp_into(&mut f.anim, t, prev, &props, val, was);
            for p in props {
                if !landed.contains(&p) {
                    landed.push(p);
                }
            }
            self.posed_folders.retain(|&p| p != id);
        }
        // Stamping an unchanged pose over its own key is not an undo step.
        let cur = self.snap();
        self.history.drop_noop(&cur);
        landed.sort_by_key(|p| anim::PROP_ORDER.iter().position(|q| q == p).unwrap_or(99));
        let what: Vec<&str> = landed.iter().map(|&p| anim::prop_tag(p)).collect();
        println!(
            "keyframe @ {:.2}s — {} ({} shape{})",
            t,
            if what.is_empty() {
                "nothing changed".to_string()
            } else {
                what.join(" ")
            },
            self.selection.len(),
            if self.selection.len() == 1 { "" } else { "s" }
        );
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
}

#[cfg(test)]
mod tests;
