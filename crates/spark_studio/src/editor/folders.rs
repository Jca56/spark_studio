//! Layer folders: organizational grouping for the layer stack.
//!
//! A folder is *not* a merge group. Merging makes several shapes behave as
//! one object on the canvas; a folder only tidies the list — collapse it,
//! name it, hide it, drag layers in and out. The two are independent, and a
//! shape can be in both.
//!
//! **The stack invariant:** a folder's members are always contiguous in the
//! shape list. The layer list *is* the draw order, so a folder can only
//! honestly be a run of it — [`Editor::normalize_folders`] re-establishes
//! that after any edit rather than trying to maintain it incrementally.
//!
//! Folders carry their own `hidden` flag, separate from each member's, so
//! hiding a folder and expanding it again doesn't forget which members were
//! individually hidden. Built to grow a transform later: a contiguous run
//! with its own identity is exactly what a transform parent needs.

use spark_render::Shape;

use super::Editor;
use crate::anim::ShapeAnim;
use crate::props::Prop;

#[derive(Clone, PartialEq, Debug)]
pub struct Folder {
    pub id: u32,
    pub name: String,
    /// Collapsed folders hide their members from the *list*; the shapes
    /// still draw on the canvas.
    pub collapsed: bool,
    /// The folder's own eye, gating every member on top of their own.
    pub hidden: bool,
    /// Transform applied on top of every member's own pose, about the
    /// folder's pivot. Identity is (0, 0, 0, 1) — a fresh folder changes
    /// nothing until you move it.
    pub x: f32,
    pub y: f32,
    pub rotation: f32,
    pub scale: f32,
    /// The folder transform's keyframe curves (X/Y/Rotation/Scale only).
    pub anim: ShapeAnim,
}

impl Folder {
    pub fn new(id: u32, name: String) -> Self {
        Self {
            id,
            name,
            collapsed: false,
            hidden: false,
            x: 0.0,
            y: 0.0,
            rotation: 0.0,
            scale: 1.0,
            anim: ShapeAnim::default(),
        }
    }

    /// Nothing to compose — members draw exactly as posed.
    pub fn is_identity(&self) -> bool {
        self.x == 0.0 && self.y == 0.0 && self.rotation == 0.0 && self.scale == 1.0
    }

    pub fn prop(&self, prop: Prop) -> Option<f32> {
        match prop {
            Prop::X => Some(self.x),
            Prop::Y => Some(self.y),
            Prop::Rotation => Some(self.rotation),
            Prop::Scale => Some(self.scale),
            _ => None,
        }
    }

    pub fn set_prop(&mut self, prop: Prop, v: f32) {
        match prop {
            Prop::X => self.x = v,
            Prop::Y => self.y = v,
            Prop::Rotation => self.rotation = v,
            // A folder scaled to nothing would be unrecoverable by dragging.
            Prop::Scale => self.scale = v.max(0.01),
            _ => {}
        }
    }

    /// Pose the folder's transform at `t` from its own curves.
    pub fn apply_anim(&mut self, t: f32) {
        for prop in [Prop::X, Prop::Y, Prop::Rotation, Prop::Scale] {
            let Some(track) = self.anim.tracks.iter().find(|tr| tr.prop == prop) else {
                continue;
            };
            if let Some(v) = track.sample(t) {
                self.set_prop(prop, v);
            }
        }
    }

    /// Compose this folder's transform onto one member, about `pivot`.
    /// Scale first, then rotate, then translate — the order the layer cards'
    /// own X/Y/R/S already implies, so a folder at identity is a no-op.
    pub fn compose(&self, shape: &mut Shape, pivot: [f32; 2]) {
        let c = shape.center();
        let mut p = [
            pivot[0] + (c[0] - pivot[0]) * self.scale,
            pivot[1] + (c[1] - pivot[1]) * self.scale,
        ];
        if self.scale != 1.0 {
            shape.scale_by(self.scale);
        }
        if self.rotation != 0.0 {
            let (sn, cs) = self.rotation.sin_cos();
            let d = [p[0] - pivot[0], p[1] - pivot[1]];
            p = [
                pivot[0] + d[0] * cs - d[1] * sn,
                pivot[1] + d[0] * sn + d[1] * cs,
            ];
            shape.rotate_by(self.rotation);
        }
        shape.set_center([p[0] + self.x, p[1] + self.y]);
    }
}

impl Editor {
    pub fn folder_of(&self, i: usize) -> u32 {
        self.folder.get(i).copied().unwrap_or(0)
    }

    pub fn folder(&self, id: u32) -> Option<&Folder> {
        self.folders.iter().find(|f| f.id == id)
    }

    /// Every shape in a folder, in stack order.
    pub fn folder_members(&self, id: u32) -> Vec<usize> {
        if id == 0 {
            return Vec::new();
        }
        self.folder
            .iter()
            .enumerate()
            .filter(|&(_, &f)| f == id)
            .map(|(i, _)| i)
            .collect()
    }

    /// Whether shape `i` draws: its own eye and its folder's both have to be
    /// open. The single truth for drawing, picking and card dimming.
    pub fn shape_hidden(&self, i: usize) -> bool {
        if self.hidden.get(i).copied().unwrap_or(false) {
            return true;
        }
        let f = self.folder_of(i);
        f != 0 && self.folder(f).is_some_and(|f| f.hidden)
    }

    /// A folder's pivot: the mean of its members' centers, so rotating and
    /// scaling a folder turns it about its own middle rather than the canvas
    /// origin. Derived, never stored — moving a member moves the pivot.
    pub fn folder_pivot(&self, id: u32) -> [f32; 2] {
        let members = self.folder_members(id);
        if members.is_empty() {
            return [0.0, 0.0];
        }
        let n = members.len() as f32;
        let (mut sx, mut sy) = (0.0, 0.0);
        for &i in &members {
            let c = self.shapes[i].center();
            sx += c[0];
            sy += c[1];
        }
        [sx / n, sy / n]
    }

    /// Set one of a folder's transform properties (the header's scrub strip).
    /// Coalesces into one undo step per drag.
    pub fn set_folder_prop(&mut self, id: u32, prop: Prop, v: f32) -> bool {
        if self.folder(id).is_none() {
            return false;
        }
        let s = self.snap();
        self.history.change(crate::history::Tag::Prop(prop), s);
        if let Some(f) = self.folders.iter_mut().find(|f| f.id == id) {
            f.set_prop(prop, v);
        }
        self.posed_folders.retain(|&p| p != id);
        self.posed_folders.push(id);
        true
    }

    /// Pose every folder's transform at the playhead, skipping ones holding
    /// an un-stamped hand pose — the same preview rule shapes get.
    pub(super) fn sync_folders_to_time(&mut self) {
        let t = self.time;
        let posed = self.posed_folders.clone();
        for f in &mut self.folders {
            if !posed.contains(&f.id) {
                f.apply_anim(t);
            }
        }
    }

    /// Ctrl+Shift+N: wrap the selection in a fresh folder.
    pub fn new_folder_from_selection(&mut self) -> bool {
        if self.selection.is_empty() {
            println!("select layers to put in a folder");
            return false;
        }
        let s = self.snap();
        self.history.push(s);
        let id = self.folders.iter().map(|f| f.id).max().unwrap_or(0) + 1;
        let n = self.selection.len();
        for &i in &self.selection.clone() {
            self.folder[i] = id;
        }
        self.folders
            .push(Folder::new(id, format!("Folder {}", self.folders.len() + 1)));
        self.normalize_folders();
        println!("foldered {n} layer(s)");
        true
    }

    /// Drop a layer into a folder (0 = pull it back out to loose).
    pub fn set_shape_folder(&mut self, i: usize, id: u32) -> bool {
        if i >= self.shapes.len() || self.folder_of(i) == id {
            return false;
        }
        if id != 0 && self.folder(id).is_none() {
            return false;
        }
        let s = self.snap();
        self.history.change(crate::history::Tag::Reorder, s);
        self.folder[i] = id;
        self.normalize_folders();
        true
    }

    /// Dissolve a folder, leaving its shapes loose and in place.
    pub fn dissolve_folder(&mut self, id: u32) -> bool {
        if self.folder(id).is_none() {
            return false;
        }
        let s = self.snap();
        self.history.push(s);
        for f in &mut self.folder {
            if *f == id {
                *f = 0;
            }
        }
        self.normalize_folders();
        println!("unfoldered");
        true
    }

    pub fn toggle_folder_collapsed(&mut self, id: u32) -> bool {
        // Collapsing is a view state, not a document edit — no undo step.
        match self.folders.iter_mut().find(|f| f.id == id) {
            Some(f) => {
                f.collapsed = !f.collapsed;
                true
            }
            None => false,
        }
    }

    pub fn toggle_folder_hidden(&mut self, id: u32) -> bool {
        if self.folder(id).is_none() {
            return false;
        }
        let s = self.snap();
        self.history.push(s);
        if let Some(f) = self.folders.iter_mut().find(|f| f.id == id) {
            f.hidden = !f.hidden;
        }
        true
    }

    pub fn rename_folder(&mut self, id: u32, name: String) -> bool {
        let Some(f) = self.folders.iter().find(|f| f.id == id) else {
            return false;
        };
        if f.name == name {
            return false;
        }
        let s = self.snap();
        self.history.push(s);
        if let Some(f) = self.folders.iter_mut().find(|f| f.id == id) {
            f.name = name;
        }
        true
    }

    /// Select every shape in a folder — clicking the folder row grabs its
    /// contents, so Delete and the transforms act on the whole thing.
    pub fn select_folder(&mut self, id: u32) -> bool {
        let members = self.folder_members(id);
        if members.is_empty() {
            return false;
        }
        self.history.commit();
        let old = std::mem::take(&mut self.selection);
        self.selection = members;
        self.range_anchor = self.selection.last().copied();
        self.expand_groups();
        old != self.selection
    }

    /// Re-establish the stack invariant: each folder's members contiguous,
    /// anchored where its lowest member already sits, relative order kept.
    /// Also drops folders that have run out of members.
    pub(super) fn normalize_folders(&mut self) {
        self.folders.retain(|f| self.folder.contains(&f.id));
        let mut order: Vec<usize> = Vec::with_capacity(self.shapes.len());
        let mut placed: Vec<u32> = Vec::new();
        for i in 0..self.shapes.len() {
            let f = self.folder_of(i);
            if f == 0 {
                order.push(i);
            } else if !placed.contains(&f) {
                placed.push(f);
                order.extend(
                    self.folder
                        .iter()
                        .enumerate()
                        .filter(|&(_, &g)| g == f)
                        .map(|(j, _)| j),
                );
            }
        }
        self.apply_order(&order);
    }

    /// Permute every parallel array (and the selection) into `order`, then
    /// re-sort `folders` to follow the new stack. A no-op order returns early
    /// so callers can run this freely.
    fn apply_order(&mut self, order: &[usize]) {
        if order.iter().enumerate().all(|(k, &i)| k == i) {
            return;
        }
        let mut inv = vec![0usize; order.len()];
        for (k, &i) in order.iter().enumerate() {
            inv[i] = k;
        }
        self.shapes = order.iter().map(|&i| self.shapes[i]).collect();
        self.names = order.iter().map(|&i| self.names[i].clone()).collect();
        self.anim = order.iter().map(|&i| self.anim[i].clone()).collect();
        self.react = order.iter().map(|&i| self.react[i]).collect();
        self.group = order.iter().map(|&i| self.group[i]).collect();
        self.hidden = order.iter().map(|&i| self.hidden[i]).collect();
        self.folder = order.iter().map(|&i| self.folder[i]).collect();
        for s in &mut self.selection {
            *s = inv[*s];
        }
        // `folders` follows the stack so the panel can walk it top-down.
        let mut seen: Vec<u32> = Vec::new();
        for i in 0..self.folder.len() {
            let f = self.folder_of(i);
            if f != 0 && !seen.contains(&f) {
                seen.push(f);
            }
        }
        self.folders
            .sort_by_key(|f| seen.iter().position(|&s| s == f.id).unwrap_or(usize::MAX));
        self.clear_posed();
        // Keyframe clipboard entries point at shape indices that just moved.
        self.key_clip = None;
    }

    /// Drag a folder header: slide its whole run to where `target` sits.
    /// Folders move as one block — that's what contiguity buys.
    pub fn move_folder(&mut self, id: u32, target: usize) -> bool {
        let members = self.folder_members(id);
        let (Some(&lo), Some(&hi)) = (members.first(), members.last()) else {
            return false;
        };
        // Dropping onto your own contents is not a move.
        if target >= lo && target <= hi || target >= self.shapes.len() {
            return false;
        }
        let rest: Vec<usize> = (0..self.shapes.len())
            .filter(|i| !members.contains(i))
            .collect();
        let Some(at) = rest.iter().position(|&i| i == target) else {
            return false;
        };
        // Landing above the run inserts before the target, below it after —
        // so the folder ends up where the cursor actually is.
        let at = if target < lo { at } else { at + 1 };
        let mut order: Vec<usize> = rest[..at].to_vec();
        order.extend_from_slice(&members);
        order.extend_from_slice(&rest[at..]);
        let s = self.snap();
        self.history.change(crate::history::Tag::Reorder, s);
        self.apply_order(&order);
        true
    }
}

#[cfg(test)]
pub(super) mod tests_support {
    use super::*;
    use spark_render::Shape;

    pub(crate) fn stack(n: usize) -> Editor {
        let mut e = Editor::empty();
        for k in 0..n {
            e.shapes.push(Shape::circle([k as f32 * 10.0, 0.0], 10.0));
            e.names.push(format!("s{k}"));
            e.anim.push(crate::anim::ShapeAnim::default());
            e.react.push([1.0; 3]);
            e.group.push(0);
            e.hidden.push(false);
            e.folder.push(0);
        }
        e
    }

    pub(crate) fn names(e: &Editor) -> Vec<&str> {
        e.names.iter().map(String::as_str).collect()
    }

    #[test]
    fn foldering_pulls_members_contiguous() {
        // Scattered picks must end up as one run, anchored at the lowest.
        let mut e = stack(6);
        e.selection = vec![1, 3, 5];
        assert!(e.new_folder_from_selection());
        assert_eq!(names(&e), vec!["s0", "s1", "s3", "s5", "s2", "s4"]);
        assert_eq!(e.folder_members(1), vec![1, 2, 3]);
    }

    #[test]
    fn foldering_remaps_the_selection() {
        let mut e = stack(6);
        e.selection = vec![1, 3, 5];
        e.new_folder_from_selection();
        // Same shapes, new indices — not the stale 1/3/5.
        let mut sel = e.selection.clone();
        sel.sort_unstable();
        assert_eq!(sel, vec![1, 2, 3]);
    }

    #[test]
    fn contiguous_folders_are_left_alone() {
        let mut e = stack(4);
        e.selection = vec![1, 2];
        e.new_folder_from_selection();
        assert_eq!(names(&e), vec!["s0", "s1", "s2", "s3"]);
    }

    #[test]
    fn folder_hidden_gates_its_members() {
        let mut e = stack(3);
        e.selection = vec![0, 1];
        e.new_folder_from_selection();
        assert!(!e.shape_hidden(0));
        e.toggle_folder_hidden(1);
        assert!(e.shape_hidden(0) && e.shape_hidden(1));
        assert!(!e.shape_hidden(2), "loose layers are unaffected");
        // The member's own eye is untouched, so expanding restores it.
        e.toggle_folder_hidden(1);
        assert!(!e.shape_hidden(0));
    }

    #[test]
    fn dissolving_keeps_the_shapes() {
        let mut e = stack(4);
        e.selection = vec![0, 2];
        e.new_folder_from_selection();
        assert!(e.dissolve_folder(1));
        assert_eq!(e.shapes.len(), 4);
        assert!(e.folders.is_empty());
        assert!(e.folder.iter().all(|&f| f == 0));
    }

    #[test]
    fn emptying_a_folder_drops_it() {
        let mut e = stack(3);
        e.selection = vec![0];
        e.new_folder_from_selection();
        assert_eq!(e.folders.len(), 1);
        // Pull the only member back out.
        assert!(e.set_shape_folder(0, 0));
        assert!(e.folders.is_empty(), "an empty folder shouldn't linger");
    }

    #[test]
    fn moving_a_layer_in_joins_the_run() {
        let mut e = stack(5);
        e.selection = vec![0, 1];
        e.new_folder_from_selection();
        // s4 joins the folder and gets pulled down beside its members.
        assert!(e.set_shape_folder(4, 1));
        assert_eq!(names(&e), vec!["s0", "s1", "s4", "s2", "s3"]);
        assert_eq!(e.folder_members(1), vec![0, 1, 2]);
    }
}

#[cfg(test)]
mod reorder_tests {
    use super::tests_support::*;

    #[test]
    fn reordering_carries_the_folder_with_the_shape() {
        // The folder id has to travel with its shape, or the arrays desync
        // and layers silently change folders when you drag the list.
        let mut e = stack(5);
        e.selection = vec![3, 4];
        e.new_folder_from_selection();
        let before: Vec<_> = e
            .names
            .iter()
            .zip(&e.folder)
            .map(|(n, f)| (n.clone(), *f))
            .collect();
        e.move_layer(0, 2);
        let after: Vec<_> = e
            .names
            .iter()
            .zip(&e.folder)
            .map(|(n, f)| (n.clone(), *f))
            .collect();
        for (name, folder) in &before {
            let got = after.iter().find(|(n, _)| n == name).map(|(_, f)| *f);
            assert_eq!(got, Some(*folder), "{name} changed folder on reorder");
        }
    }

    #[test]
    fn reordering_into_a_folder_run_is_pulled_back_out() {
        let mut e = stack(5);
        e.selection = vec![3, 4];
        e.new_folder_from_selection();
        // Drop a loose layer into the middle of the folder's run.
        e.move_layer(0, 4);
        // It must not have joined the folder, and the run stays whole.
        let members = e.folder_members(1);
        assert_eq!(members.len(), 2);
        assert_eq!(members[1], members[0] + 1, "folder run must stay contiguous");
    }
}

#[cfg(test)]
mod transform_tests {
    use super::tests_support::*;
    use crate::props::Prop;

    /// Two shapes at x=0 and x=100, y=0 — pivot lands at (50, 0).
    fn pair() -> crate::editor::Editor {
        let mut e = stack(2);
        e.shapes[0].set_center([0.0, 0.0]);
        e.shapes[1].set_center([100.0, 0.0]);
        e.selection = vec![0, 1];
        e.new_folder_from_selection();
        e
    }

    #[test]
    fn identity_folder_changes_nothing() {
        let e = pair();
        for i in 0..2 {
            let before = e.shapes[i];
            let after = e.posed_shape(i, before);
            assert_eq!(after.center(), before.center());
            assert_eq!(after.size(), before.size());
        }
    }

    #[test]
    fn folder_offset_moves_every_member() {
        let mut e = pair();
        e.set_folder_prop(1, Prop::X, 30.0);
        e.set_folder_prop(1, Prop::Y, -10.0);
        assert_eq!(e.posed_shape(0, e.shapes[0]).center(), [30.0, -10.0]);
        assert_eq!(e.posed_shape(1, e.shapes[1]).center(), [130.0, -10.0]);
    }

    #[test]
    fn folder_scale_works_about_the_pivot() {
        let mut e = pair();
        assert_eq!(e.folder_pivot(1), [50.0, 0.0]);
        e.set_folder_prop(1, Prop::Scale, 2.0);
        // Members spread from the pivot, and each grows.
        assert_eq!(e.posed_shape(0, e.shapes[0]).center(), [-50.0, 0.0]);
        assert_eq!(e.posed_shape(1, e.shapes[1]).center(), [150.0, 0.0]);
        assert!((e.posed_shape(0, e.shapes[0]).size() - e.shapes[0].size() * 2.0).abs() < 0.01);
    }

    #[test]
    fn folder_rotation_orbits_the_pivot() {
        let mut e = pair();
        e.set_folder_prop(1, Prop::Rotation, std::f32::consts::PI);
        // A half turn about (50,0) swaps the two members' positions.
        let a = e.posed_shape(0, e.shapes[0]).center();
        let b = e.posed_shape(1, e.shapes[1]).center();
        assert!((a[0] - 100.0).abs() < 0.01 && a[1].abs() < 0.01, "{a:?}");
        assert!((b[0] - 0.0).abs() < 0.01 && b[1].abs() < 0.01, "{b:?}");
    }

    #[test]
    fn folder_scale_never_collapses_to_zero() {
        let mut e = pair();
        e.set_folder_prop(1, Prop::Scale, -5.0);
        assert!(e.folder(1).unwrap().scale > 0.0, "a 0 scale can't be dragged back");
    }

    #[test]
    fn folder_transform_keys_and_poses() {
        let mut e = pair();
        e.set_time(0.0);
        e.set_folder_prop(1, Prop::X, 0.0);
        e.stamp_key();
        e.set_time(2.0);
        e.set_folder_prop(1, Prop::X, 100.0);
        e.stamp_key();
        // Halfway between: smoothstep(0.5) = 0.5, so dead centre.
        e.set_time(1.0);
        e.sync_to_time();
        assert!((e.folder(1).unwrap().x - 50.0).abs() < 0.01);
    }

    #[test]
    fn moving_a_folder_takes_its_whole_run() {
        let mut e = stack(5);
        e.selection = vec![3, 4];
        e.new_folder_from_selection();
        // Folder holds s3,s4 at the top; drop it onto s0 at the bottom.
        assert!(e.move_folder(1, 0));
        assert_eq!(names(&e), vec!["s3", "s4", "s0", "s1", "s2"]);
        assert_eq!(e.folder_members(1), vec![0, 1]);
    }

    #[test]
    fn moving_a_folder_onto_itself_is_refused() {
        let mut e = stack(4);
        e.selection = vec![1, 2];
        e.new_folder_from_selection();
        assert!(!e.move_folder(1, 1), "dropping on your own contents is a no-op");
    }
}
