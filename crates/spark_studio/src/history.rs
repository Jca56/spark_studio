//! Undo/redo: a stack of document snapshots. Shapes are tiny POD structs,
//! so whole-document snapshots at gesture boundaries stay cheap.

use spark_render::Shape;

use crate::anim::ShapeAnim;
use crate::editor::Prop;

/// Keep memory bounded; 256 × a-few-KB comps is nothing.
const MAX_DEPTH: usize = 256;

/// One undoable state: the document plus the selection to restore with it.
#[derive(Clone, PartialEq)]
pub struct Snap {
    pub shapes: Vec<Shape>,
    pub paths: Vec<Vec<[f32; 2]>>,
    pub names: Vec<String>,
    pub anim: Vec<ShapeAnim>,
    pub react: Vec<[f32; 3]>,
    /// Merge-group id per shape (0 = ungrouped).
    pub group: Vec<u32>,
    /// Eye-toggled-off shapes (kept in the document, not drawn).
    pub hidden: Vec<bool>,
    pub selection: Vec<usize>,
}

/// Which continuous gesture a change belongs to. Consecutive changes with
/// the same tag coalesce into one undo step (a slider drag, a scroll burst,
/// a run of nudge keypresses); anything discrete uses `push` instead.
#[derive(Clone, Copy, PartialEq)]
pub enum Tag {
    Prop(Prop),
    Wheel,
    KeyRotate,
    KeyGlow,
    KeyBright,
    Sides,
    Color,
    Reorder,
    /// A transform-handle drag (scale/rotate) — one undo step per drag.
    Handle,
    /// A lane keyframe drag (retime) — one undo step per drag.
    Keys,
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
