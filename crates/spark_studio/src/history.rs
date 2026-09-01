//! Undo/redo: a stack of document snapshots. Shapes are tiny POD structs,
//! so whole-document snapshots at gesture boundaries stay cheap.

use spark_render::Shape;

use crate::doc::ObjClip;
use crate::editor::{Folder, Prop};

/// Keep memory bounded; 256 × a-few-KB comps is nothing.
const MAX_DEPTH: usize = 256;

/// One undoable state: the document plus the selection to restore with it.
#[derive(Clone, PartialEq)]
pub struct Snap {
    /// The objects' base state — the document truth, never posed copies.
    pub shapes: Vec<Shape>,
    /// Stable object identity, parallel to `shapes` — restored with them so
    /// an undone delete brings an object back under the id its clips still
    /// refer to.
    pub ids: Vec<u32>,
    pub paths: Vec<Vec<[f32; 2]>>,
    pub names: Vec<String>,
    /// Each object's clips — existence spans plus their clip-local curves.
    pub clips: Vec<Vec<ObjClip>>,
    /// Effect stacks (base), parallel to `shapes`.
    pub fx: Vec<crate::fx::Stack>,
    pub react: Vec<[f32; 3]>,
    /// Merge-group id per shape (0 = ungrouped).
    pub group: Vec<u32>,
    /// Eye-toggled-off shapes (kept in the document, not drawn).
    pub hidden: Vec<bool>,
    /// Folder id per shape (0 = loose), and the folder definitions.
    pub folder: Vec<u32>,
    pub folders: Vec<Folder>,
    /// The comp's size — a document property, so changing it undoes.
    pub canvas: [f32; 2],
    /// The arrangement's comp half: placed comps and their clips.
    pub comp_assets: Vec<crate::doc::CompAsset>,
    pub comp_clips: Vec<crate::doc::Clip>,
    pub duration: Option<f32>,
    pub selection: Vec<usize>,
}

/// Which continuous gesture a change belongs to. Consecutive changes with
/// the same tag coalesce into one undo step (a slider drag, a scroll burst,
/// a run of nudge keypresses); anything discrete uses `push` instead.
#[derive(Clone, Copy, PartialEq)]
pub enum Tag {
    Prop(Prop),
    /// One effect parameter's slider — (effect id, parameter).
    Effect(u32, u8),
    KeyRotate,
    KeyGlow,
    KeyBright,
    Sides,
    Color,
    Reorder,
    /// A transform-handle drag (scale/rotate) — one undo step per drag.
    Handle,
    /// A key drag in the clip view (retime) — one undo step per drag.
    #[allow(dead_code)] // kept for the redesign; the clip view re-consumes it
    Keys,
    /// One clip being dragged or trimmed on the arrangement.
    Clip,
}

pub struct History {
    undo: Vec<Snap>,
    redo: Vec<Snap>,
    tag: Option<Tag>,
}

impl History {
    pub fn new() -> Self {
        Self {
            undo: Vec::new(),
            redo: Vec::new(),
            tag: None,
        }
    }

    /// Record a discrete change: `before` becomes its own undo step.
    pub fn push(&mut self, before: Snap) {
        if self.undo.len() == MAX_DEPTH {
            self.undo.remove(0);
        }
        self.undo.push(before);
        self.redo.clear();
        self.tag = None;
    }

    /// Record a coalescible change: pushes `before` unless it continues the
    /// gesture already on top of the stack.
    pub fn change(&mut self, tag: Tag, before: Snap) {
        if self.tag != Some(tag) {
            self.push(before);
            self.tag = Some(tag);
        }
    }

    /// End any in-progress gesture (mouse released, focus moved on) so the
    /// next change starts a fresh undo step.
    pub fn commit(&mut self) {
        self.tag = None;
    }

    /// Drop the top entry if the gesture turned out to change nothing
    /// (click-select without moving, a discarded draw speck).
    pub fn drop_noop(&mut self, current: &Snap) {
        if self.undo.last() == Some(current) {
            self.undo.pop();
        }
    }

    pub fn undo(&mut self, current: Snap) -> Option<Snap> {
        let snap = self.undo.pop()?;
        self.redo.push(current);
        self.tag = None;
        Some(snap)
    }

    pub fn redo(&mut self, current: Snap) -> Option<Snap> {
        let snap = self.redo.pop()?;
        self.undo.push(current);
        self.tag = None;
        Some(snap)
    }
}
