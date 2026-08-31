//! The comp editor: tools, selection, and direct manipulation on the canvas.
//! Status feedback prints to the terminal until SparkUI text rendering lands.
//!
//! Selection is a set: the last entry is the *primary* (what the inspector
//! shows); relative edits and moves apply to every selected shape.
//! Mutating methods return `true` when the visible state changed, so the app
//! only redraws when something actually happened.

use spark_render::Shape;

mod clips;
mod effects;
mod folders;
mod io;
mod precompose;
mod lights;
#[cfg(test)]
pub(crate) use io::{mesh_fit, mesh_shape};
mod keys;
mod mouse;
mod paths;
mod sel;
mod snap;
mod space;
mod style;

pub use folders::Folder;

use crate::anim::ShapeAnim;
use crate::fx::Stack;
use crate::history::{History, Snap, Tag};
use crate::props::StyleClip;
pub use crate::props::{PALETTE, Prop, Tool};
use mouse::Drag;

/// What an unsaved comp is called until Save As gives it a real name. A
/// session opens on one of these: Spark starts on a blank page rather than
/// reopening whatever was last in the working directory.
pub const UNTITLED: &str = "untitled.spark";

/// The first shape id handed out. Ids start at 1 so 0 can stay the "no
/// shape" value, matching folder id 0 meaning "loose".
pub(crate) const FIRST_SHAPE_ID: u32 = 1;

pub struct Editor {
    shapes: Vec<Shape>,
    /// Identity per shape, parallel to `shapes` — stable across reordering,
    /// deletion and undo, which stack indices are not.
    ///
    /// Anything that outlives a single frame and refers to a shape refers to
    /// it by id: keyframe lane owners, the key selection, the expanded lane.
    /// Holding indices meant a reorder silently repointed a key selection at
    /// whatever shape had slid into that slot. Session-local — the document
    /// format stores order, not identity, so a load hands out fresh ids.
    ids: Vec<u32>,
    /// Next id to hand out. Never rewound (undo restores `ids`, not this),
    /// so a restored shape can't collide with one created since.
    next_id: u32,
    /// Path vertex lists (center-relative canvas units), referenced by
    /// path shapes' ids. Deleted entries leak until save/load compacts.
    paths: Vec<Vec<[f32; 2]>>,
    /// User-given layer names, parallel to `shapes` (empty = auto-label).
    names: Vec<String>,
    /// Keyframe curves, parallel to `shapes`.
    anim: Vec<ShapeAnim>,
    /// Effect stacks, parallel to `shapes`. What a layer optionally *does*,
    /// as opposed to what it is — see `fx.rs`.
    fx: Vec<Stack>,
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
    /// The models mesh shapes draw, saved with the document.
    assets: Vec<crate::doc::MeshAsset>,
    /// A tempo the user typed, overriding what analysis guessed.
    bpm_override: Option<f32>,
    /// The comp's size in canvas units — which is the video's size in
    /// pixels. 1920×1080 unless the comp says otherwise; a portrait comp
    /// for a phone is 1080×1920. Saved with the document, undoable, and
    /// the one number the camera's film gate, the viewport's fit, the
    /// prop ranges and the floor all read.
    canvas: [f32; 2],
    /// The comps this one places and the clips that play them — the
    /// arrangement (see `editor/clips.rs`). Document state, undoable.
    comp_assets: Vec<crate::doc::CompAsset>,
    clips: Vec<crate::doc::Clip>,
    /// An explicit comp length in seconds — the loop period when this
    /// comp is placed as a clip. `None` derives it from the last key.
    duration: Option<f32>,
    /// The dice: every new shape rolls its own look (see `random.rs`).
    /// Session state like the snap toggles — a mode of the hand, not of
    /// the document.
    pub random: bool,
    rng: crate::random::Rng,
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
    /// What the curves posed each shape as at the playhead, before any hand
    /// edit — the baseline [`Editor::stamp_key`] diffs against to work out
    /// which properties actually moved.
    ///
    /// Scratch: rebuilt by `sync_to_time` every frame for shapes that are
    /// *not* holding a preview pose, and deliberately frozen for the ones
    /// that are, since remembering the pre-edit value is the entire job.
    /// Never serialized and never in a history snapshot. Cleared whenever
    /// indices stop meaning what they did, so a stale entry can't be
    /// mistaken for a baseline.
    pose_base: Vec<Shape>,
    /// The same baseline for effect parameters, so a stamp can tell which
    /// knob the hand actually moved.
    fx_base: Vec<Stack>,
    /// The same baseline per folder id: `[x, y, rotation, scale]`. Kept
    /// beside `folders` rather than on `Folder`, which is compared field by
    /// field to detect no-op undo steps — scratch state in there would make
    /// an unchanged document look changed.
    folder_base: Vec<(u32, [f32; 5])>,
    /// Active alignment guides: (vertical?, canvas coordinate).
    guides: Vec<(bool, f32)>,
    /// The camera the viewport looks through — what picking unprojects
    /// with. The studio keeps it current; see `space.rs`.
    camera: spark_render::Camera,
    /// Ctrl+C'd style, waiting for Ctrl+V.
    style_clip: Option<StyleClip>,
    /// Ctrl+C'd keyframe — the most recent copy (style or key) wins Ctrl+V.
    key_clip: Option<crate::anim::KeyClip>,
}

impl Editor {
    /// A blank document, untouched by disk.
    ///
    /// Spark used to reopen `comp.spark` from the working directory on every
    /// launch, which made whichever comp happened to sit there the implicit
    /// home project. A session starts on a blank page; File > Open picks the
    /// comp you actually meant.
    pub(crate) fn empty() -> Self {
        Self {
            shapes: Vec::new(),
            ids: Vec::new(),
            next_id: FIRST_SHAPE_ID,
            paths: Vec::new(),
            names: Vec::new(),
            anim: Vec::new(),
            fx: Vec::new(),
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
            assets: Vec::new(),
            bpm_override: None,
            canvas: spark_render::CANVAS,
            comp_assets: Vec::new(),
            clips: Vec::new(),
            duration: None,
            random: false,
            rng: crate::random::Rng::from_clock(),
            snap_grid: false,
            smart_guides: true,
            time: 0.0,
            posed: Vec::new(),
            posed_folders: Vec::new(),
            pose_base: Vec::new(),
            fx_base: Vec::new(),
            folder_base: Vec::new(),
            guides: Vec::new(),
            camera: spark_render::Camera::stage(spark_render::CANVAS),
            style_clip: None,
            key_clip: None,
        }
    }

    /// Hand out a fresh shape id. Monotonic across undo, so a shape restored
    /// by undo can never share an id with one created after it was deleted.
    pub(super) fn new_id(&mut self) -> u32 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    /// Tests reach straight into a shape's curves; the app goes through
    /// stamping.
    #[cfg(test)]
    pub(crate) fn anim_of_mut(&mut self, i: usize) -> &mut crate::anim::ShapeAnim {
        &mut self.anim[i]
    }

    /// A shape's stable identity, for anything that outlives the frame.
    pub fn shape_id(&self, i: usize) -> u32 {
        self.ids.get(i).copied().unwrap_or(0)
    }

    /// Where a shape id currently sits in the stack, or `None` if it's gone
    /// — which is exactly what makes a stale key selection harmless.
    pub fn index_of(&self, id: u32) -> Option<usize> {
        self.ids.iter().position(|&x| x == id)
    }

    /// The keyframe-lane owner for a stack index.
    pub fn owner(&self, i: usize) -> crate::anim::Owner {
        crate::anim::Owner::Shape(self.shape_id(i))
    }

    /// Append a shape at the top of the stack with a fresh id and default
    /// per-shape state, returning its index.
    ///
    /// The one place the parallel arrays grow. They were being extended by
    /// hand at every call site, which is how a new array (`ids`) turns into
    /// six independent chances to forget one.
    pub(super) fn push_shape(&mut self, shape: Shape) -> usize {
        self.shapes.push(shape);
        let id = self.new_id();
        self.ids.push(id);
        self.names.push(String::new());
        self.anim.push(ShapeAnim::default());
        self.fx.push(Stack::default());
        self.react.push([1.0; 3]);
        self.group.push(0);
        self.hidden.push(false);
        self.folder.push(0);
        self.shapes.len() - 1
    }

    /// Drop the shape at `i` from every parallel array — the mirror of
    /// [`Editor::push_shape`]. Folder normalization and posed-state clearing
    /// are the caller's, since a bulk delete wants them once at the end.
    pub(super) fn remove_shape(&mut self, i: usize) {
        self.shapes.remove(i);
        self.ids.remove(i);
        self.names.remove(i);
        self.anim.remove(i);
        self.fx.remove(i);
        self.react.remove(i);
        self.group.remove(i);
        self.hidden.remove(i);
        self.folder.remove(i);
    }

    fn snap(&self) -> Snap {
        Snap {
            shapes: self.shapes.clone(),
            ids: self.ids.clone(),
            paths: self.paths.clone(),
            names: self.names.clone(),
            anim: self.anim.clone(),
            fx: self.fx.clone(),
            react: self.react.clone(),
            group: self.group.clone(),
            hidden: self.hidden.clone(),
            folder: self.folder.clone(),
            folders: self.folders.clone(),
            canvas: self.canvas,
            comp_assets: self.comp_assets.clone(),
            clips: self.clips.clone(),
            duration: self.duration,
            selection: self.selection.clone(),
        }
    }

    fn apply(&mut self, snap: Snap) {
        self.shapes = snap.shapes;
        self.ids = snap.ids;
        self.paths = snap.paths;
        self.names = snap.names;
        self.anim = snap.anim;
        self.fx = snap.fx;
        self.react = snap.react;
        self.group = snap.group;
        self.hidden = snap.hidden;
        self.folder = snap.folder;
        self.folders = snap.folders;
        self.canvas = snap.canvas;
        self.comp_assets = snap.comp_assets;
        self.clips = snap.clips;
        self.duration = snap.duration;
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
            (false, "6") => self.set_tool(Tool::Stars),
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
            (false, "a") => self.nudge_glow(4.0),
            (false, "z") => self.nudge_glow(-4.0),
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

    /// A layer's effect stack, for the panels that list it.
    pub fn fx_of(&self, i: usize) -> &Stack {
        static NONE: std::sync::OnceLock<Stack> = std::sync::OnceLock::new();
        self.fx
            .get(i)
            .unwrap_or_else(|| NONE.get_or_init(Stack::default))
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
