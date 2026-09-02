//! The comp editor: tools, selection, and direct manipulation on the canvas.
//! Status feedback prints to the terminal until SparkUI text rendering lands.
//!
//! Selection is a set: the last entry is the *primary* (what the inspector
//! shows); relative edits and moves apply to every selected shape.
//! Mutating methods return `true` when the visible state changed, so the app
//! only redraws when something actually happened.

use spark_render::Shape;

mod audio;
mod clipboard;
mod clips;
mod curves;
mod effects;
mod folders;
mod io;
mod lights;
mod precompose;
#[cfg(test)]
pub(crate) use io::{mesh_fit, mesh_shape};
mod keys;
mod mouse;
mod paths;
mod relink;
mod sel;
mod snap;
mod space;
mod style;

pub use curves::KeySpan;
pub use folders::Folder;

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
    /// The working copies the frame reads: base state with the active
    /// clip's curves applied at the playhead. Picking, handles and the
    /// inspector all see animated values because they read these.
    shapes: Vec<Shape>,
    /// The document truth, parallel to `shapes`: the objects' base state,
    /// changed by hand edits only — never by playback. `sync_to_time`
    /// restores `shapes` from here every frame before applying curves,
    /// and absorbs hand edits back (see `keys`).
    base: Vec<Shape>,
    /// Identity per object, parallel to `shapes` — stable across
    /// reordering, deletion, undo *and saves*: v2 files store ids, because
    /// clips and anything else that outlives a session name objects by id.
    ids: Vec<u32>,
    /// Next id to hand out. Never rewound (undo restores `ids`, not this),
    /// so a restored shape can't collide with one created since.
    next_id: u32,
    /// Path vertex lists (center-relative canvas units), referenced by
    /// path shapes' ids. Deleted entries leak until save/load compacts.
    paths: Vec<Vec<[f32; 2]>>,
    /// User-given layer names, parallel to `shapes` (empty = auto-label).
    names: Vec<String>,
    /// Each object's clips, parallel to `shapes`, sorted by start and
    /// never overlapping: when it exists, and how it moves (clip-local
    /// keyframes). No clip under the playhead = the object isn't there.
    clips: Vec<Vec<crate::doc::ObjClip>>,
    /// Effect stacks — the working copies, like `shapes`. What a layer
    /// optionally *does*, as opposed to what it is — see `fx.rs`.
    fx: Vec<Stack>,
    /// The effect stacks' document truth, like `base`.
    base_fx: Vec<Stack>,
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
    /// what the context menu's palette edits. Owned by the tool, never by
    /// the selection: only the swatches, the picker, and the eyedropper
    /// move it.
    color: [f32; 3],
    /// The second colour — the inspector's background swatch: what a
    /// selected shape's gradient runs to when it has one, and the far
    /// end of a gradient default to come. Swaps with `color`.
    color_b: [f32; 3],
    /// What each tool draws — the context menu's pages edit these. A mode
    /// of the hand, like the dice: session state, never in the document.
    pub defaults: crate::defaults::Defaults,
    press: [f32; 2],
    cursor: [f32; 2],
    history: History,
    /// The comp's audio track, saved with the document.
    audio_path: Option<String>,
    /// The models mesh shapes draw, saved with the document.
    assets: Vec<crate::doc::MeshAsset>,
    /// A tempo the user typed, overriding what analysis guessed.
    bpm_override: Option<f32>,
    /// Seconds per bar at the comp's working tempo — what a newborn clip's
    /// length is. Session state: the studio keeps it current from the
    /// beat grid (2.0 = the silent comp's 120 BPM).
    pub(crate) bar_s: f32,
    /// The comp's size in canvas units — which is the video's size in
    /// pixels. 1920×1080 unless the comp says otherwise; a portrait comp
    /// for a phone is 1080×1920. Saved with the document, undoable, and
    /// the one number the camera's film gate, the viewport's fit, the
    /// prop ranges and the floor all read.
    canvas: [f32; 2],
    /// The comps this one places and the clips that play them — the
    /// arrangement (see `editor/clips.rs`). Document state, undoable.
    comp_assets: Vec<crate::doc::CompAsset>,
    comp_clips: Vec<crate::doc::Clip>,
    /// The arrangement's audio half: the sounds this comp names, the
    /// clips that place them (the song's too), and each track's volume
    /// (see `editor/audio.rs`). Document state, undoable.
    sounds: Vec<crate::doc::SoundAsset>,
    aclips: Vec<crate::doc::AudioClip>,
    volumes: Vec<(u32, f32)>,
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
    /// What the curves posed each shape as at the playhead, before any hand
    /// edit — the baseline [`Editor::stamp_key`] diffs against to work out
    /// which properties actually moved, and the reference `sync_to_time`
    /// absorbs hand edits into `base` against.
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
    /// Which clip posed each shape last frame (index into its clip list),
    /// so the absorb step knows which properties were curve scratch.
    pose_clip: Vec<Option<usize>>,
    /// Whether a clip covers the playhead for each shape — rebuilt by
    /// `sync_to_time`. An absent object isn't drawn and can't be picked.
    present: Vec<bool>,
    /// Active alignment guides: (vertical?, canvas coordinate).
    guides: Vec<(bool, f32)>,
    /// The camera the viewport looks through — what picking unprojects
    /// with. The studio keeps it current; see `space.rs`.
    camera: spark_render::Camera,
    /// Ctrl+Shift+C'd style, waiting for Ctrl+Shift+V.
    style_clip: Option<StyleClip>,
    /// Ctrl+C'd objects, waiting for Ctrl+V (see `clipboard`).
    clipboard: Option<clipboard::Clipboard>,
    /// Ctrl+C'd keys from the clip view, waiting for Ctrl+V there (see
    /// `curves::copy`).
    key_clip: Option<curves::KeyClip>,
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
            base: Vec::new(),
            ids: Vec::new(),
            next_id: FIRST_SHAPE_ID,
            paths: Vec::new(),
            names: Vec::new(),
            clips: Vec::new(),
            fx: Vec::new(),
            base_fx: Vec::new(),
            group: Vec::new(),
            hidden: Vec::new(),
            folder: Vec::new(),
            folders: Vec::new(),
            selection: Vec::new(),
            range_anchor: None,
            tool: Tool::Select,
            drag: None,
            color: crate::props::gold(),
            color_b: PALETTE[1],
            defaults: crate::defaults::Defaults::default(),
            press: [0.0; 2],
            cursor: [0.0; 2],
            history: History::new(),
            audio_path: None,
            assets: Vec::new(),
            bpm_override: None,
            bar_s: 2.0,
            canvas: spark_render::CANVAS,
            comp_assets: Vec::new(),
            comp_clips: Vec::new(),
            sounds: Vec::new(),
            aclips: Vec::new(),
            volumes: Vec::new(),
            duration: None,
            random: false,
            rng: crate::random::Rng::from_clock(),
            snap_grid: false,
            smart_guides: true,
            time: 0.0,
            posed: Vec::new(),
            pose_base: Vec::new(),
            fx_base: Vec::new(),
            pose_clip: Vec::new(),
            present: Vec::new(),
            guides: Vec::new(),
            camera: spark_render::Camera::stage(spark_render::CANVAS),
            style_clip: None,
            clipboard: None,
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

    /// Tests reach straight into a clip's curves; the app goes through
    /// stamping.
    #[cfg(test)]
    pub(crate) fn clip_anim_mut(&mut self, i: usize, c: usize) -> &mut crate::anim::ShapeAnim {
        &mut self.clips[i][c].anim
    }

    /// An object's stable identity, for anything that outlives the frame
    /// — and, since v2, the session: the file stores it.
    pub fn shape_id(&self, i: usize) -> u32 {
        self.ids.get(i).copied().unwrap_or(0)
    }

    /// Where an object id currently sits in the stack, or `None` if it's
    /// gone — which is exactly what makes a stale reference harmless.
    pub fn index_of(&self, id: u32) -> Option<usize> {
        self.ids.iter().position(|&x| x == id)
    }

    /// Append an object at the top of the stack with a fresh id, default
    /// per-object state, and a **newborn clip** — one bar at the playhead,
    /// looping its own length: a thing exists where its clip is, from the
    /// moment it exists at all. Returns its index.
    ///
    /// The one place the parallel arrays grow. They were being extended by
    /// hand at every call site, which is how a new array turns into eight
    /// independent chances to forget one.
    pub(super) fn push_shape(&mut self, shape: Shape) -> usize {
        self.shapes.push(shape);
        self.base.push(shape);
        let id = self.new_id();
        self.ids.push(id);
        self.names.push(String::new());
        self.clips
            .push(vec![crate::doc::ObjClip::new(self.time, self.bar_s)]);
        self.fx.push(Stack::default());
        self.base_fx.push(Stack::default());
        self.group.push(0);
        self.hidden.push(false);
        self.folder.push(0);
        // The stamp/absorb baselines grow eagerly, so an edit made before
        // the next frame's sync still diffs against the birth pose.
        if self.pose_base.len() == self.shapes.len() - 1 {
            self.pose_base.push(shape);
            self.fx_base.push(Stack::default());
            self.pose_clip.push(None);
            self.present.push(false);
        }
        self.shapes.len() - 1
    }

    /// Drop the object at `i` from every parallel array — the mirror of
    /// [`Editor::push_shape`]. Folder normalization and posed-state clearing
    /// are the caller's, since a bulk delete wants them once at the end.
    pub(super) fn remove_shape(&mut self, i: usize) {
        self.shapes.remove(i);
        self.base.remove(i);
        self.ids.remove(i);
        self.names.remove(i);
        self.clips.remove(i);
        self.fx.remove(i);
        self.base_fx.remove(i);
        self.group.remove(i);
        self.hidden.remove(i);
        self.folder.remove(i);
        if self.pose_base.len() > i {
            self.pose_base.remove(i);
            self.fx_base.remove(i);
            self.pose_clip.remove(i);
            self.present.remove(i);
        }
    }

    fn snap(&self) -> Snap {
        Snap {
            // The document truth: base state, never the posed copies.
            shapes: self.base.clone(),
            ids: self.ids.clone(),
            paths: self.paths.clone(),
            names: self.names.clone(),
            clips: self.clips.clone(),
            fx: self.base_fx.clone(),
            group: self.group.clone(),
            hidden: self.hidden.clone(),
            folder: self.folder.clone(),
            folders: self.folders.clone(),
            canvas: self.canvas,
            assets: self.assets.clone(),
            comp_assets: self.comp_assets.clone(),
            comp_clips: self.comp_clips.clone(),
            sounds: self.sounds.clone(),
            aclips: self.aclips.clone(),
            volumes: self.volumes.clone(),
            duration: self.duration,
            selection: self.selection.clone(),
        }
    }

    fn apply(&mut self, snap: Snap) {
        self.shapes = snap.shapes.clone();
        self.base = snap.shapes;
        self.ids = snap.ids;
        self.paths = snap.paths;
        self.names = snap.names;
        self.clips = snap.clips;
        self.fx = snap.fx.clone();
        self.base_fx = snap.fx;
        self.group = snap.group;
        self.hidden = snap.hidden;
        self.folder = snap.folder;
        self.folders = snap.folders;
        self.canvas = snap.canvas;
        self.assets = snap.assets;
        self.comp_assets = snap.comp_assets;
        self.comp_clips = snap.comp_clips;
        self.sounds = snap.sounds;
        self.aclips = snap.aclips;
        self.volumes = snap.volumes;
        self.duration = snap.duration;
        self.selection = snap.selection;
        self.drag = None;
        self.clear_posed();
    }

    /// Record a coalescible change on the selection (skipped when nothing is
    /// selected, so the document can't gain no-op undo steps).
    fn record(&mut self, tag: Tag) {
        if !self.selection.is_empty() {
            // Prior edits fold into the truth first, or the snapshot
            // about to be taken would silently contain them.
            self.absorb_pending();
            let s = self.snap();
            self.history.change(tag, s);
        }
    }

    pub fn undo(&mut self) -> bool {
        self.absorb_pending();
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
        self.absorb_pending();
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
        // The gesture's edits reach the truth before the no-op check
        // reads it — a drag that ended inside one frame would otherwise
        // compare pre-absorb truth to itself and drop its own undo step.
        self.absorb_pending();
        let s = self.snap();
        self.history.drop_noop(&s);
        self.history.commit();
    }

    pub fn char_key(&mut self, key: &str, ctrl: bool, shift: bool) -> bool {
        match (ctrl, key) {
            (true, "z") if shift => self.redo(),
            (true, "z") => self.undo(),
            // Ctrl+C / Ctrl+V move whole objects (a paste lands on the
            // cursor); with Shift, the look alone.
            (true, "c") if shift => self.copy_style(),
            (true, "v") if shift => self.paste_style(),
            (true, "c") => self.copy_objects(),
            (true, "v") => self.paste_objects(self.cursor),
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

    /// The color new shapes draw with — the inspector's foreground swatch.
    pub fn color(&self) -> [f32; 3] {
        self.color
    }

    /// The second colour — the inspector's background swatch.
    pub fn color_b(&self) -> [f32; 3] {
        self.color_b
    }

    /// The pair's right-click: foreground and background trade places.
    /// Nothing is painted — the swap is of the tool's colours, the way
    /// every paint program's is.
    pub fn swap_colors(&mut self) -> bool {
        std::mem::swap(&mut self.color, &mut self.color_b);
        true
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
    #[allow(dead_code)] // kept for the redesign; the old panels were the only caller
    pub fn fx_of(&self, i: usize) -> &Stack {
        static NONE: std::sync::OnceLock<Stack> = std::sync::OnceLock::new();
        self.fx
            .get(i)
            .unwrap_or_else(|| NONE.get_or_init(Stack::default))
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
            println!(
                "tool: Polygon ({} sides)",
                self.defaults.get(Tool::Polygon).sides
            );
        } else {
            println!("tool: {tool:?}");
        }
        // The toolbar highlights the active tool, so switching is visual now.
        true
    }
}
