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
    /// Fades every member on top of its own opacity. Honest limitation:
    /// this multiplies *into* each member rather than compositing the
    /// folder as one image, so overlapping members show through each other
    /// halfway down a fade. Doing it properly means rendering the folder to
    /// its own texture — the same seam comp-level post FX will need.
    pub opacity: f32,
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
            opacity: 1.0,
        }
    }

    /// Nothing to compose — members draw exactly as posed.
    pub fn is_identity(&self) -> bool {
        self.x == 0.0
            && self.y == 0.0
            && self.rotation == 0.0
            && self.scale == 1.0
            && self.opacity == 1.0
    }

    #[allow(dead_code)] // kept for the redesign; the clip view / inspector re-consume it
    pub fn prop(&self, prop: Prop) -> Option<f32> {
        match prop {
            Prop::X => Some(self.x),
            Prop::Y => Some(self.y),
            Prop::Rotation => Some(self.rotation),
            Prop::Scale => Some(self.scale),
            Prop::Opacity => Some(self.opacity),
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
            // Unlike scale, all the way to zero is the point, and it is
            // recoverable: the slider stays where the shapes were.
            Prop::Opacity => self.opacity = v.clamp(0.0, 1.0),
            _ => {}
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
        // Multiplied, not assigned: a member already faded to 40% inside a
        // folder at 50% is at 20%, not back up at 50%.
        if self.opacity != 1.0 {
            shape.set_opacity(shape.opacity() * self.opacity);
        }
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
    #[allow(dead_code)] // kept for the redesign; the old panels were the only caller
    pub fn set_folder_prop(&mut self, id: u32, prop: Prop, v: f32) -> bool {
        if self.folder(id).is_none() {
            return false;
        }
        let s = self.snap();
        self.history.change(crate::history::Tag::Prop(prop), s);
        if let Some(f) = self.folders.iter_mut().find(|f| f.id == id) {
            f.set_prop(prop, v);
        }
        true
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
        self.folders.push(Folder::new(
            id,
            format!("Folder {}", self.folders.len() + 1),
        ));
        self.normalize_folders();
        println!("foldered {n} layer(s)");
        true
    }

    /// Drop a layer into a folder (0 = pull it back out to loose).
    ///
    /// Currently has no route in the UI: dragging a card onto a folder
    /// header did this, and was unreliable enough to be a hazard. Kept for
    /// the version that replaces it — the model is sound, the gesture
    /// wasn't. See also [`Editor::new_folder_from_selection`], which works.
    #[allow(dead_code)]
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
    ///
    /// Currently has no route in the UI: it used to be a bare right-click on
    /// the folder header, which is a destructive edit with no confirmation
    /// and no menu around it — a trap rather than a control. Kept for the
    /// affordance that replaces it.
    #[allow(dead_code)]
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

    #[allow(dead_code)] // kept for the redesign; the old panels were the only caller
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

    #[allow(dead_code)] // kept for the redesign; the old panels were the only caller
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

    #[allow(dead_code)] // kept for the redesign; the old panels were the only caller
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
        self.base = order.iter().map(|&i| self.base[i]).collect();
        self.ids = order.iter().map(|&i| self.ids[i]).collect();
        self.names = order.iter().map(|&i| self.names[i].clone()).collect();
        self.clips = order.iter().map(|&i| self.clips[i].clone()).collect();
        self.fx = order.iter().map(|&i| self.fx[i].clone()).collect();
        self.base_fx = order.iter().map(|&i| self.base_fx[i].clone()).collect();
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
        // The keyframe clipboard rides shape *ids*, which a permutation
        // doesn't touch — reordering used to throw a copy away.
    }

    /// Drag a folder header: slide its whole run to where `target` sits.
    /// Folders move as one block — that's what contiguity buys.
    #[allow(dead_code)] // kept for the redesign; the old panels were the only caller
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
mod tests;
