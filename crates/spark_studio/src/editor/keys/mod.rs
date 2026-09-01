//! Playhead evaluation and manual keyframing under the object/clip model.
//!
//! Every frame, [`Editor::sync_to_time`] runs one cycle per object:
//! **absorb → restore → apply**. Absorb folds hand edits made since the
//! last frame into `base` (the document truth) — except values the active
//! clip's curves were driving, which are preview scratch. Restore rewinds
//! the working copy to `base`. Apply finds the clip covering the playhead
//! and samples its curves at clip-local time; no clip means the object
//! is absent — not drawn, not pickable.
//!
//! Keys appear only when the Keyframe button (or `K`) stamps the selected
//! objects' pose into their **active clip**, at clip-local time. Editing a
//! keyed object between stamps is a *preview*: the pose holds while you
//! look at it, and reverts to the curves the moment the playhead moves.
//! Stamp it or lose it. No clip under the playhead, nothing to stamp into.

use super::Editor;
use crate::anim::{self, Ease, KEY_EPS, Key, Track};
pub use crate::anim::Target;
use crate::fx::Stack;

impl Editor {
    /// The playhead time all evaluation and stamping happens at. Moving it
    /// discards any un-stamped pose.
    pub fn set_time(&mut self, t: f32) {
        if (t - self.time).abs() > 1e-4 {
            self.posed.clear();
        }
        self.time = t;
    }

    pub fn time(&self) -> f32 {
        self.time
    }

    /// The index of `i`'s clip covering time `t`, if any.
    pub fn clip_at(&self, i: usize, t: f32) -> Option<usize> {
        self.clips
            .get(i)?
            .iter()
            .position(|c| c.contains(t))
    }

    /// Whether a clip covers the playhead for shape `i` — recomputed live
    /// rather than read from scratch state, so callers between syncs
    /// (drawing births, clip drags) always see the truth.
    pub fn exists_now(&self, i: usize) -> bool {
        self.clip_at(i, self.time).is_some()
    }

    /// The one evaluation cycle: absorb hand edits into `base`, restore
    /// the working copies, apply the active clip's curves at clip-local
    /// time, and refresh the stamp baseline. Objects holding an un-stamped
    /// preview pose are left alone so the preview doesn't snap back.
    pub fn sync_to_time(&mut self) {
        let n = self.shapes.len();
        if self.pose_base.len() > n {
            // A shrink or reorder: every caller of those clears the
            // scratch already — this is the belt over those braces.
            self.pose_base.clear();
            self.fx_base.clear();
            self.pose_clip.clear();
        }
        if self.pose_base.len() < n {
            // Appends (a draw in progress) extend the baselines without
            // touching existing entries, so the absorb below still sees
            // every older shape's diff — and the *new* shape's baseline is
            // its birth pose, so the rest of its draw-drag absorbs too.
            let from = self.pose_base.len();
            self.pose_base.extend_from_slice(&self.shapes[from..]);
            self.fx_base.extend_from_slice(&self.fx[from..]);
            self.pose_clip.resize(n, None);
        }
        self.present.resize(n, false);
        for i in 0..n {
            self.present[i] = self.exists_now(i);
            if self.posed.contains(&i) {
                // Frozen preview: the baseline keeps the pre-edit pose.
                continue;
            }
            self.absorb(i);
            // Restore, then apply the active clip.
            self.shapes[i] = self.base[i];
            self.fx[i] = self.base_fx[i].clone();
            let active = self.clip_at(i, self.time);
            if let Some(ci) = active {
                let lt = self.clips[i][ci].local(self.time);
                let anim = self.clips[i][ci].anim.clone();
                anim.apply(&mut self.shapes[i], &mut self.fx[i], lt);
            }
            self.pose_clip[i] = active;
            self.pose_base[i] = self.shapes[i];
            self.fx_base[i] = self.fx[i].clone();
        }
    }

    /// Fold what the hand changed since last frame into the document
    /// truth: everything the working copy holds, except the values the
    /// last frame's active clip was driving — those are curve output (or
    /// preview scratch), and only a stamp may turn them into document.
    fn absorb(&mut self, i: usize) {
        if self.shapes[i] == self.pose_base[i] && self.fx[i] == self.fx_base[i] {
            return;
        }
        let mut nb = self.shapes[i];
        let mut nfx = self.fx[i].clone();
        if let Some(ci) = self.pose_clip[i]
            && let Some(clip) = self.clips[i].get(ci)
        {
            for target in clip.anim.targets() {
                match target {
                    Target::Shape(p) => {
                        if let Some(v) = anim::prop_value(&self.base[i], p) {
                            anim::apply_prop(&mut nb, p, v);
                        }
                    }
                    Target::Effect { id, param } => {
                        if let (Some(e), Some(v)) = (
                            nfx.find_mut(id),
                            self.base_fx[i].find(id).map(|e| e.get(param as usize)),
                        ) {
                            e.set(param as usize, v);
                        }
                    }
                }
            }
        }
        self.base[i] = nb;
        self.base_fx[i] = nfx;
    }

    /// Fold every non-previewing object's pending hand edits into the
    /// document truth *now* — the gesture seams (undo, redo, end-of-drag,
    /// a new history snapshot) call this so the truth is current before
    /// history reads or compares it. Idempotent; the per-frame sync does
    /// the same fold on its own schedule.
    pub(super) fn absorb_pending(&mut self) {
        let n = self.shapes.len().min(self.pose_base.len()).min(self.fx_base.len());
        for i in 0..n {
            if !self.posed.contains(&i) {
                self.absorb(i);
            }
        }
    }

    /// After-edit hook: a keyed object was changed by hand, so its current
    /// values are a preview overriding the curves until the playhead moves.
    /// Only objects whose *active clip* has keys preview — an edit on an
    /// unkeyed clip is a plain edit, absorbed next frame.
    pub(super) fn mark_posed(&mut self, indices: &[usize]) {
        for &i in indices {
            let keyed = self
                .clip_at(i, self.time)
                .and_then(|ci| self.clips.get(i)?.get(ci))
                .is_some_and(|c| c.anim.has_keys());
            if keyed && !self.posed.contains(&i) {
                self.posed.push(i);
            }
        }
    }

    pub(super) fn mark_posed_selection(&mut self) {
        let sel = self.selection.clone();
        self.mark_posed(&sel);
    }

    /// Shape indices changed meaning (delete/reorder/load), so every scrap
    /// of index-keyed scratch state is void.
    pub(super) fn clear_posed(&mut self) {
        self.posed.clear();
        self.range_anchor = None;
        self.pose_base.clear();
        self.fx_base.clear();
        self.pose_clip.clear();
        self.present.clear();
    }

    /// The rule deciding what a stamp keys.
    ///
    /// 1. **Nothing keyed yet** — lay down `first_pose`.
    /// 2. **Something moved** — key exactly what moved.
    /// 3. **Nothing moved** — *hold*: re-stamp what is already animated at
    ///    its current value. `K` twice without touching anything is how
    ///    you ask for stillness between two moments.
    fn pick_props(keyed: Vec<Target>, moved: Vec<Target>, first_pose: Vec<Target>) -> Vec<Target> {
        if keyed.is_empty() {
            first_pose
        } else if moved.is_empty() {
            keyed
        } else {
            moved
        }
    }

    /// Stamp `targets` into `anim` at local time `t`, backfilling a
    /// holding key at `prev` for a target earning its first track — that
    /// is what makes "turn the glow up at bar 5 and press K" ramp instead
    /// of jumping.
    fn stamp_into(
        anim: &mut crate::anim::ShapeAnim,
        t: f32,
        prev: Option<f32>,
        targets: &[Target],
        value: impl Fn(Target) -> Option<f32>,
        was: impl Fn(Target) -> Option<f32>,
    ) {
        for &target in targets {
            let Some(v) = value(target) else { continue };
            let fresh = anim.tracks.iter().all(|tr| tr.target != target);
            match anim.track_mut(target) {
                Some(track) => track.upsert(t, v),
                None => anim.tracks.push(Track {
                    target,
                    keys: vec![Key {
                        t,
                        v,
                        ease: Ease::Smooth,
                    }],
                }),
            }
            if fresh
                && let (Some(at), Some(then)) = (prev, was(target))
                && let Some(track) = anim.track_mut(target)
            {
                track.upsert(at, then);
            }
        }
    }

    /// The last key time strictly before `t` on a clip already carrying
    /// `times` — where a backfilled holding key lands.
    fn hold_time(times: &[(f32, Ease)], t: f32) -> Option<f32> {
        times
            .iter()
            .rev()
            .map(|&(kt, _)| kt)
            .find(|&kt| kt < t - KEY_EPS)
    }

    /// Every target a shape could animate right now: its properties, plus
    /// one per parameter of every effect on it.
    fn shape_targets(shape: &spark_render::Shape, fx: &Stack) -> Vec<Target> {
        let mut out: Vec<Target> = anim::PROP_ORDER
            .into_iter()
            .filter(|&p| anim::prop_value(shape, p).is_some())
            .map(Target::Shape)
            .collect();
        for e in &fx.effects {
            for k in 0..e.kind.params().len() {
                out.push(Target::Effect {
                    id: e.id,
                    param: k as u8,
                });
            }
        }
        out
    }

    /// Read one target off a pose.
    pub(super) fn read(shape: &spark_render::Shape, fx: &Stack, target: Target) -> Option<f32> {
        match target {
            Target::Shape(p) => anim::prop_value(shape, p),
            Target::Effect { id, param } => fx.find(id).map(|e| e.get(param as usize)),
        }
    }

    /// Read one target off the *baseline* pose. An effect only just added
    /// isn't in the baseline; its history is what the resolver drew
    /// without it.
    fn read_base(
        live: &Stack,
        shape: &spark_render::Shape,
        fx: &Stack,
        target: Target,
    ) -> Option<f32> {
        match target {
            Target::Shape(p) => anim::prop_value(shape, p),
            Target::Effect { id, param } => match fx.find(id) {
                Some(e) => Some(e.get(param as usize)),
                None => live.find(id).map(|e| e.kind.absent(param as usize)),
            },
        }
    }

    /// The Keyframe button: stamp what the hand actually changed into each
    /// selected object's **active clip**, at clip-local time. An object
    /// with no clip under the playhead has nowhere to put a key and says
    /// so.
    pub fn stamp_key(&mut self) -> bool {
        if self.selection.is_empty() {
            println!("keyframe: nothing selected");
            return false;
        }
        let before = self.snap();
        self.history.push(before);
        let mut landed: Vec<Target> = Vec::new();
        let mut skipped = 0usize;
        for &i in &self.selection.clone() {
            let Some(ci) = self.clip_at(i, self.time) else {
                skipped += 1;
                continue;
            };
            let lt = self.clips[i][ci].local(self.time);
            let shape = self.shapes[i];
            let fx = self.fx[i].clone();
            let base = self.pose_base.get(i).copied();
            let fx_base = self.fx_base.get(i).cloned();
            let keyed = self.clips[i][ci].anim.targets();
            let prev = Self::hold_time(&self.clips[i][ci].anim.key_times(), lt);
            let moved: Vec<Target> = match (base, &fx_base) {
                (Some(was_shape), Some(was_fx)) => Self::shape_targets(&shape, &fx)
                    .into_iter()
                    .filter(|&tg| {
                        match (
                            Self::read(&shape, &fx, tg),
                            Self::read_base(&fx, &was_shape, was_fx, tg),
                        ) {
                            (Some(now), Some(then)) => match tg {
                                Target::Shape(p) => anim::changed(p, now, then),
                                Target::Effect { id, param } => fx
                                    .find(id)
                                    .and_then(|e| e.kind.params().get(param as usize))
                                    .is_some_and(|s| {
                                        (now - then).abs() > (s.max - s.min).abs() * 1e-4
                                    }),
                            },
                            _ => false,
                        }
                    })
                    .collect(),
                _ => Vec::new(),
            };
            let first: Vec<Target> = anim::FIRST_POSE
                .into_iter()
                .filter(|&p| anim::prop_value(&shape, p).is_some())
                .map(Target::Shape)
                .collect();
            let targets = Self::pick_props(keyed, moved, first);
            Self::stamp_into(
                &mut self.clips[i][ci].anim,
                lt,
                prev,
                &targets,
                |tg| Self::read(&shape, &fx, tg),
                |tg| match (base, &fx_base) {
                    (Some(s), Some(f)) => Self::read_base(&fx, &s, f, tg),
                    _ => None,
                },
            );
            for tg in targets {
                if !landed.contains(&tg) {
                    landed.push(tg);
                }
            }
            self.posed.retain(|&p| p != i);
        }
        // Stamping an unchanged pose over its own key is not an undo step.
        let cur = self.snap();
        self.history.drop_noop(&cur);
        let what: Vec<String> = landed.iter().map(|t| t.tag()).collect();
        println!(
            "keyframe @ {:.2}s — {}{}",
            self.time,
            if what.is_empty() {
                "nothing changed".to_string()
            } else {
                what.join(" ")
            },
            if skipped > 0 {
                format!(" ({skipped} selected with no clip here)")
            } else {
                String::new()
            }
        );
        !landed.is_empty()
    }
}

#[cfg(test)]
mod tests;
