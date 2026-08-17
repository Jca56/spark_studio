//! The comp editor: tools, selection, and direct manipulation on the canvas.
//! Status feedback prints to the terminal until SparkUI text rendering lands.
//!
//! Selection is a set: the last entry is the *primary* (what the inspector
//! shows); relative edits and moves apply to every selected shape.
//! Mutating methods return `true` when the visible state changed, so the app
//! only redraws when something actually happened.

use std::path::Path;

use spark_render::Shape;

mod folders;
mod io;
mod keys;
mod paths;
mod sel;
mod snap;
mod style;

pub use folders::Folder;

use crate::anim::ShapeAnim;
use crate::history::{History, Snap, Tag};
pub use crate::props::{PALETTE, Prop, Tool};
use crate::props::{StyleClip, dist, draw_shape};

pub const COMP_PATH: &str = "comp.spark";

enum Drag {
    Draw,
    Move {
        last: [f32; 2],
        /// The primary's *unsnapped* center, tracking the cursor's intent.
        /// Snapping quantizes this — never the already-snapped position,
        /// which would gridlock the drag.
        free: [f32; 2],
    },
}

pub struct Editor {
    shapes: Vec<Shape>,
    /// Path vertex lists (center-relative canvas units), referenced by
    /// path shapes' ids. Deleted entries leak until save/load compacts.
    paths: Vec<Vec<[f32; 2]>>,
    /// User-given layer names, parallel to `shapes` (empty = auto-label).
    names: Vec<String>,
    /// Keyframe curves, parallel to `shapes`.
    anim: Vec<ShapeAnim>,
    /// Audio-reaction amounts per shape: [bass→scale, bass→glow,
    /// mid/onset→bright], 1.0 = the classic wobble, 0 = unmoved.
    react: Vec<[f32; 3]>,
    /// Merge-group id per shape (0 = ungrouped). Members select, move,
    /// and transform as one; each keeps its own style and geometry.
    group: Vec<u32>,
    /// Eye-toggled-off shapes: kept, saved, listed — just not drawn and
    /// not clickable on canvas.
    hidden: Vec<bool>,
    /// Folder id per shape (0 = loose), parallel to `shapes`. Members are
    /// kept contiguous — see `editor/folders.rs`.
    folder: Vec<u32>,
    /// Folder definitions, ordered to follow the stack.
    folders: Vec<Folder>,
    /// Selected shape indices; the last entry is the primary.
    selection: Vec<usize>,
    /// Where Shift+click spans *from*: the last plain/ctrl layer click. Kept
    /// separate from the primary so repeated Shift+clicks re-span from one
    /// fixed origin instead of walking it along.
    range_anchor: Option<usize>,
    tool: Tool,
    drag: Option<Drag>,
    /// The current color (linear RGB) — what the next shape draws with and
    /// what the color home edits. Owned by the tool, never by the selection:
    /// only the swatches, the picker, and the eyedropper move it.
    color: [f32; 3],
    sides: u32,
    press: [f32; 2],
    cursor: [f32; 2],
    history: History,
    /// The comp's audio track, saved with the document.
    audio_path: Option<String>,
    /// Snap the dragged shape's center to the 60-unit canvas grid.
    pub snap_grid: bool,
    /// Snap to canvas center and other shapes' centers while dragging.
    pub smart_guides: bool,
    /// Playhead time (seconds) — where evaluation and stamping happen.
    time: f32,
    /// Keyed shapes holding an un-stamped hand pose (see `editor/keys.rs`).
    posed: Vec<usize>,
    /// Same, for folder transforms — by folder id, since folders survive
    /// reordering while shape indices don't.
    posed_folders: Vec<u32>,
    /// Active alignment guides: (vertical?, canvas coordinate).
    guides: Vec<(bool, f32)>,
    /// Ctrl+C'd style, waiting for Ctrl+V.
    style_clip: Option<StyleClip>,
    /// Ctrl+C'd keyframe — the most recent copy (style or key) wins Ctrl+V.
    key_clip: Option<crate::anim::KeyClip>,
}

impl Editor {
    pub fn new() -> Self {
        let mut editor = Self::empty();
        if Path::new(COMP_PATH).exists() {
            editor.load(COMP_PATH);
            // The startup load is the baseline, not an undoable edit.
            editor.history = History::new();
        }
        editor
    }

    /// A blank document, untouched by disk — what `new` starts from.
    fn empty() -> Self {
        Self {
            shapes: Vec::new(),
            paths: Vec::new(),
            names: Vec::new(),
            anim: Vec::new(),
            react: Vec::new(),
            group: Vec::new(),
            hidden: Vec::new(),
            folder: Vec::new(),
            folders: Vec::new(),
            selection: Vec::new(),
            range_anchor: None,
            tool: Tool::Select,
            drag: None,
            color: PALETTE[0],
            sides: 5,
            press: [0.0; 2],
            cursor: [0.0; 2],
            history: History::new(),
            audio_path: None,
            snap_grid: false,
            smart_guides: true,
            time: 0.0,
            posed: Vec::new(),
            posed_folders: Vec::new(),
            guides: Vec::new(),
            style_clip: None,
            key_clip: None,
        }
    }

    fn snap(&self) -> Snap {
        Snap {
            shapes: self.shapes.clone(),
            paths: self.paths.clone(),
            names: self.names.clone(),
            anim: self.anim.clone(),
            react: self.react.clone(),
            group: self.group.clone(),
            hidden: self.hidden.clone(),
            folder: self.folder.clone(),
            folders: self.folders.clone(),
            selection: self.selection.clone(),
        }
    }

    fn apply(&mut self, snap: Snap) {
        self.shapes = snap.shapes;
        self.paths = snap.paths;
        self.names = snap.names;
        self.anim = snap.anim;
        self.react = snap.react;
        self.group = snap.group;
        self.hidden = snap.hidden;
        self.folder = snap.folder;
        self.folders = snap.folders;
        self.selection = snap.selection;
        self.drag = None;
        self.clear_posed();
        self.clear_posed_folders();
    }

    /// Record a coalescible change on the selection (skipped when nothing is
    /// selected, so the document can't gain no-op undo steps).
    fn record(&mut self, tag: Tag) {
        if !self.selection.is_empty() {
            let s = self.snap();
            self.history.change(tag, s);
        }
    }

    pub fn undo(&mut self) -> bool {
        let cur = self.snap();
        match self.history.undo(cur) {
            Some(s) => {
                self.apply(s);
                println!("undo");
                true
            }
            None => {
                println!("nothing to undo");
                false
            }
        }
    }

    pub fn redo(&mut self) -> bool {
        let cur = self.snap();
        match self.history.redo(cur) {
            Some(s) => {
                self.apply(s);
                println!("redo");
                true
            }
            None => {
                println!("nothing to redo");
                false
            }
        }
    }

    /// A mouse release ended whatever gesture was running; the next change
    /// starts a fresh undo step. Gestures that ended where they started
    /// (a layer dragged back to its slot) leave no undo step behind.
    pub fn end_gesture(&mut self) {
        let s = self.snap();
        self.history.drop_noop(&s);
        self.history.commit();
    }

    /// Window-space cursor (physical px) -> canvas units through the
    /// canvas view's mapping, then drive any active drag.
    pub fn set_cursor(&mut self, px: f64, py: f64, map: crate::view::CanvasMap) -> bool {
        let (scale, ox, oy) = map;
        let now = [(px as f32 - ox) / scale, (py as f32 - oy) / scale];
        self.cursor = now;
        let free_target = if let Some(Drag::Move { last, free }) = &mut self.drag {
            let d = [now[0] - last[0], now[1] - last[1]];
            *last = now;
            free[0] += d[0];
            free[1] += d[1];
            Some(*free)
        } else {
            None
        };
        if let Some(free) = free_target {
            self.move_selection_to(free);
            return true;
        }
        match &mut self.drag {
            Some(Drag::Draw) => {
                if let Some(&i) = self.selection.last() {
                    self.shapes[i] = draw_shape(self.tool, self.press, now, self.sides, self.color);
                }
                true
            }
            _ => false,
        }
    }

    /// Ctrl+click toggles membership in the selection; a plain click on an
    /// already-selected shape keeps the set (so groups drag together).
    pub fn mouse_down(&mut self, ctrl: bool) -> bool {
        if self.tool == Tool::Select {
            let hit = self.pick(self.cursor);
            let old = self.selection.clone();
            match hit {
                Some(i) if ctrl => {
                    self.history.commit();
                    self.toggle_index(i);
                }
                Some(i) => {
                    if !self.selection.contains(&i) {
                        self.selection = vec![i];
                        self.expand_groups();
                    }
                    // Pre-move state; dropped again at mouse_up if nothing
                    // moved.
                    let s = self.snap();
                    self.history.push(s);
                    let free = self.shapes[self.primary().unwrap_or(i)].center();
                    self.drag = Some(Drag::Move {
                        last: self.cursor,
                        free,
                    });
                }
                None if !ctrl => self.selection.clear(),
                None => {}
            }
            old != self.selection
        } else {
            self.press = self.cursor;
            let s = self.snap();
            self.history.push(s);
            self.shapes.push(draw_shape(
                self.tool,
                self.press,
                self.cursor,
                self.sides,
                self.color,
            ));
            self.names.push(String::new());
            self.anim.push(ShapeAnim::default());
            self.react.push([1.0; 3]);
            self.group.push(0);
            self.hidden.push(false);
            self.folder.push(0);
            self.selection = vec![self.shapes.len() - 1];
            self.drag = Some(Drag::Draw);
            true
        }
    }

    pub fn mouse_up(&mut self) -> bool {
        let mut dirty = false;
        if let Some(Drag::Draw) = self.drag {
            // A click with no drag leaves an accidental speck — discard it.
            if dist(self.press, self.cursor) < 3.0
                && let Some(&i) = self.selection.last()
            {
                self.shapes.remove(i);
                self.names.remove(i);
                self.anim.remove(i);
                self.react.remove(i);
                self.group.remove(i);
                self.hidden.remove(i);
                self.folder.remove(i);
                self.selection.clear();
                dirty = true;
            }
        }
        if self.drag.take().is_some() {
            // Discarded specks and moves that never moved undo to nothing —
            // drop the snapshot the gesture pushed.
            let s = self.snap();
            self.history.drop_noop(&s);
        }
        self.guides.clear();
        self.history.commit();
        dirty
    }

    fn pick(&self, p: [f32; 2]) -> Option<usize> {
        for (i, s) in self.shapes.iter().enumerate().rev() {
            if self.shape_hidden(i) {
                continue;
            }
            let posed = self.posed_shape(i, *s);
            let d = if posed.is_path() {
                self.path_pick(&posed, p)
            } else {
                posed.pick_distance(p)
            };
            if d <= 14.0 {
                return Some(i);
            }
        }
        None
    }

    pub fn wheel(&mut self, dy: f32, rotate: bool) -> bool {
        if self.selection.is_empty() {
            return false;
        }
        self.record(Tag::Wheel);
        let factor = (1.0 + dy * 0.08).clamp(0.5, 2.0);
        let rot = dy * 0.06;
        for i in self.selection.clone() {
            if rotate {
                self.shapes[i].rotate_by(rot);
            } else {
                self.scale_index(i, factor);
            }
        }
        self.mark_posed_selection();
        true
    }

    pub fn char_key(&mut self, key: &str, ctrl: bool, shift: bool) -> bool {
        match (ctrl, key) {
            (true, "z") if shift => self.redo(),
            (true, "z") => self.undo(),
            (true, "c") => self.copy_style(),
            (true, "v") => self.paste_style(),
            (false, "1") => self.set_tool(Tool::Select),
            (false, "2") => self.set_tool(Tool::Circle),
            (false, "3") => self.set_tool(Tool::Box),
            (false, "4") => self.set_tool(Tool::Polygon),
            (false, "5") => self.set_tool(Tool::Line),
            (true, "d") => self.duplicate_selected(),
            (false, "k") => self.stamp_key(),
            (false, "q") => self.nudge(Tag::KeyRotate, |s| s.rotate_by(-0.0873)),
            (false, "e") => self.nudge(Tag::KeyRotate, |s| s.rotate_by(0.0873)),
            (false, "[") => self.adjust_sides(-1),
            (false, "]") => self.adjust_sides(1),
            (false, "p") => self.convert_to_path(),
            (false, "o") => self.toggle_path_closed(),
            (false, "=") | (false, "+") => self.add_vertex(),
            (false, "-") => self.remove_vertex(),
            (false, "c") => self.cycle_color(),
            (false, "i") => self.eyedrop_primary(),
            (false, "t") => {
                let flip = self
                    .primary()
                    .and_then(|i| self.shapes[i].outline())
                    .map(|o| !o);
                match flip {
                    Some(on) => self.set_outline(on),
                    None => false,
                }
            }
            (false, "a") => self.nudge(Tag::KeyGlow, |s| s.add_glow(4.0)),
            (false, "z") => self.nudge(Tag::KeyGlow, |s| s.add_glow(-4.0)),
            (false, "w") => self.nudge(Tag::KeyBright, |s| s.add_intensity(0.1)),
            (false, "s") => self.nudge(Tag::KeyBright, |s| s.add_intensity(-0.1)),
            (false, "x") => self.delete_selected(),
            _ => false,
        }
    }

    /// A keyboard adjustment: coalesces with the run of same-tag presses.
    fn nudge(&mut self, tag: Tag, f: impl Fn(&mut Shape)) -> bool {
        self.record(tag);
        let changed = self.with_selected(f);
        if changed {
            self.mark_posed_selection();
        }
        changed
    }

    pub fn tool(&self) -> Tool {
        self.tool
    }

    /// The primary selection — the shape whose card the controls target.
    pub fn primary(&self) -> Option<usize> {
        self.selection.last().copied()
    }

    /// The color new shapes draw with, and what the color home shows.
    pub fn color(&self) -> [f32; 3] {
        self.color
    }

    /// Which palette swatch to ring, if the current color is one of them.
    pub fn palette_match(&self) -> Option<usize> {
        PALETTE.iter().position(|p| *p == self.color)
    }

    /// Load a color as current without painting anything — the eyedropper
    /// and arming a gradient chip both want the color, not the edit.
    pub fn load_color(&mut self, rgb: [f32; 3]) -> bool {
        if rgb == self.color {
            return false;
        }
        self.color = rgb;
        true
    }

    /// The eyedropper: take a shape's color as the current one without
    /// touching the shape or the selection.
    pub fn eyedrop(&mut self, i: usize) -> bool {
        let Some(rgb) = self.shapes.get(i).map(|s| s.rgb()) else {
            return false;
        };
        println!("picked color {:?}", rgb.map(|c| (c * 255.0).round() as u8));
        self.load_color(rgb)
    }

    /// Alt+click on the canvas: eyedrop whatever is under the cursor.
    pub fn eyedrop_at_cursor(&mut self) -> bool {
        match self.pick(self.cursor) {
            Some(i) => self.eyedrop(i),
            None => false,
        }
    }

    /// `I`: eyedrop the primary selection — handy once a shape is already
    /// picked and you just want its color for the next one.
    pub fn eyedrop_primary(&mut self) -> bool {
        match self.primary() {
            Some(i) => self.eyedrop(i),
            None => false,
        }
    }

    pub fn choose_tool(&mut self, tool: Tool) {
        self.set_tool(tool);
    }

    /// Latest cursor position in canvas units.
    pub fn cursor(&self) -> [f32; 2] {
        self.cursor
    }

    pub fn shapes(&self) -> &[Shape] {
        &self.shapes
    }

    /// A shape's audio-reaction amounts (1.0 each = the classic wobble).
    pub fn react(&self, i: usize) -> [f32; 3] {
        self.react.get(i).copied().unwrap_or([1.0; 3])
    }

    pub fn selection(&self) -> &[usize] {
        &self.selection
    }

    /// Apply an edit to every selected shape.
    fn with_selected(&mut self, f: impl Fn(&mut Shape)) -> bool {
        if self.selection.is_empty() {
            return false;
        }
        for &i in &self.selection {
            f(&mut self.shapes[i]);
        }
        true
    }

    fn set_tool(&mut self, tool: Tool) -> bool {
        self.tool = tool;
        if tool == Tool::Polygon {
            println!("tool: Polygon ({} sides)", self.sides);
        } else {
            println!("tool: {tool:?}");
        }
        // The toolbar highlights the active tool, so switching is visual now.
        true
    }
}
