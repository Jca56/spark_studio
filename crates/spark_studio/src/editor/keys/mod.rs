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
use crate::anim::{self, Ease, Key, Owner, ShapeAnim, Track};
use crate::props::Prop;

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
}

#[cfg(test)]
mod tests;
