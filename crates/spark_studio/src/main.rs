mod align;
mod anim;
mod app;
mod arrange;
mod chrome;
mod clipview;
mod comps;
mod context;
mod cursor;
mod defaults;
mod doc;
mod drag;
mod editor;
mod export;
mod fx;
mod gizmo;
mod handles;
mod help;
mod history;
mod hotkeys;
mod input;
mod inspector;
mod left;
mod lights;
mod menu;
mod meshes;
mod overlay;
mod picker;
mod primitives;
mod project;
mod props;
mod random;
mod reaction;
mod relink;
mod render;
mod scene;
mod sound;
mod status;
// Kept whole for the redesign: the scrub fields' text editing rode this,
// and the new UI's number entry will again.
#[allow(dead_code)]
mod textbox;
mod timeline;
mod transport;
mod view;
mod viewpoint;

use std::path::PathBuf;
use std::sync::Arc;

use editor::Editor;
use spark_render::{Gpu, ShapePass, Stage};
use spark_text::Text;
use spark_ui::{Layout, Menu, TitleAction, TitleBar, UiPass};
use winit::event_loop::{ControlFlow, EventLoop, EventLoopProxy};
use winit::keyboard::ModifiersState;
use winit::window::Window;

/// An in-progress transform-handle drag on the canvas.
enum HandleDrag {
    /// Corner: uniform scale; ref_dist is the last cursor→pivot distance.
    Scale {
        center: [f32; 2],
        ref_dist: f32,
    },
    Width,
    Height,
    Rotate {
        center: [f32; 2],
        prev: f32,
    },
    /// A path vertex being dragged, by index.
    Vertex(usize),
    /// One end of a line (0 = start, 1 = end); the other holds.
    End(usize),
}

/// Results posted back to the event loop from worker threads.
enum AppEvent {
    /// The file picker closed: the chosen path, or `None` on cancel.
    Picked(picker::Purpose, Option<PathBuf>),
    /// Off-thread decode + analysis of the given path finished.
    AudioLoaded(String, Result<spark_audio::Track, String>),
    /// A sound (asset id, path) decoded off-thread — or didn't.
    SoundLoaded(u32, String, Result<spark_audio::Sound, String>),
    /// A mesh file was read and its textures decoded off-thread: the
    /// asset it is (`None` for a fresh import, assigned on arrival), its
    /// path, and what came of it.
    MeshLoaded(Option<u32>, String, Result<meshes::Loaded, String>),
    /// FFmpeg finished with the export: the file it wrote, or why not.
    Exported(Result<String, String>),
}

/// App icon baked to raw RGBA (64x64) from assets/spark_studio_icon.svg —
/// no image decoding at runtime.
const APP_ICON: &[u8] = include_bytes!("../assets/spark_icon_64.rgba");

/// Where analysis bakes live: `$XDG_CACHE_HOME/spark-studio`, or
/// `~/.cache/spark-studio`. `None` — no home at all — just means no
/// cache, never an error.
fn cache_dir() -> Option<std::path::PathBuf> {
    std::env::var_os("XDG_CACHE_HOME")
        .map(std::path::PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| std::path::PathBuf::from(h).join(".cache")))
        .map(|b| b.join("spark-studio"))
}

/// Bottom-panel height (logical px) a fresh session opens with — and what
/// double-clicking the resize bar snaps back to.
pub(crate) const DEFAULT_TIMELINE_H: f32 = 360.0;

struct Studio {
    window: Option<Arc<Window>>,
    gpu: Option<Gpu>,
    shape_pass: Option<ShapePass>,
    /// The document's picture between frames: re-rendered only when the
    /// shape pass's inputs change, blitted otherwise (see `Stage`).
    stage: Option<Stage>,
    /// The comp's imported models on the GPU, by asset id.
    meshes: std::collections::HashMap<u32, meshes::MeshAssetGpu>,
    /// Mesh files still being read on worker threads.
    mesh_loading: usize,
    /// Mesh assets whose file couldn't be read: their rows say so, and
    /// they aren't retried until relinked (see `relink`).
    mesh_missing: Vec<u32>,
    ui_pass: Option<UiPass>,
    /// A second UiPass with its own buffers for the frame's base coat
    /// (gutter + checkerboard) — instance buffers are per-pass, so one
    /// UiPass can't draw both before and after the shape pass.
    bg_pass: Option<UiPass>,
    text: Option<Text>,
    editor: Editor,
    modifiers: ModifiersState,
    cursor_px: (f64, f64),
    title_hover: Option<TitleAction>,
    title_pressed: Option<TitleAction>,
    /// The transport's tempo field while it is being typed into.
    bpm_edit: Option<String>,
    /// Which menu-bar menu is open (index into the menus array), if any.
    menu_open: Option<usize>,
    menu_hover: Option<usize>,
    menu_anchor_hover: Option<usize>,
    /// The right-click context menu: where it was opened (physical px),
    /// while it's up; what was under the cursor then, and where that was
    /// in canvas units (a paste lands there); the hovered tool-rail
    /// button; the page widget under the cursor; and a slider drag in
    /// progress (see `context`).
    ctx_menu: Option<[f32; 2]>,
    ctx_target: context::Target,
    ctx_at: [f32; 2],
    ctx_hover: Option<usize>,
    ctx_over: Option<context::Hit>,
    ctx_drag: Option<context::Drag>,
    /// The context menu's value box being typed into (the clip view's
    /// key page).
    ctx_edit: Option<textbox::TextBox>,
    /// The right panel: scroll, the picker's HSV, hover, drags, and a
    /// field being typed into (see `inspector`).
    inspector: inspector::State,
    /// The left panel: its tab, hover, and an effect row being dragged
    /// onto an object (see `left`).
    left: left::State,
    wordmark_w: f32,
    /// Measured anchor label widths ("File", "View"), cached between frames.
    anchor_ws: [f32; 4],
    menu_item_w: f32,
    /// File > Export Video, while it runs (see `export`). The editor is
    /// read-only until it finishes or Esc cancels it.
    export: Option<export::Job>,
    /// The .spark files this comp's clips play, keyed by comp asset id.
    subcomps: std::collections::HashMap<u32, comps::PlacedComp>,
    /// Next GPU-map key for a placed comp's meshes (starts far above any
    /// id a document hands out — see `comps::SUB_MESH_BASE`).
    sub_mesh_next: u32,
    /// The selected clips on the arrangement — objects', comps',
    /// audio; the last is the primary. A drag carries them all.
    selected_clips: Vec<arrange::ClipRef>,
    /// A clip being dragged or trimmed.
    clip_drag: Option<arrange::ClipDrag>,
    /// A track row being dragged up or down the sidebar.
    row_drag: Option<arrange::RowDrag>,
    /// How many rows the sidebar listed last frame — one more and the
    /// list scrolls to the newcomer, which lands at the bottom.
    rows_seen: usize,
    /// Last clip click, for double-click-opens-the-comp.
    last_clip_click: Option<(arrange::ClipRef, std::time::Instant)>,
    /// The last press on a track's name, for the double-click that opens
    /// its clip view.
    last_head_click: Option<(u32, std::time::Instant)>,
    /// The clip curve view, while the bottom panel is one (see
    /// `clipview`): which clip, its window on local time, the pick.
    clip_view: Option<clipview::State>,
    /// What the last export came to — and any other one-line notice —
    /// for the status strip until the next click.
    export_note: Option<String>,
    /// Editing a placed comp: the project waits here, whole, until the
    /// title's breadcrumb goes Back.
    comp_stack: Vec<project::Crumb>,
    /// What the last save/load serialized to — the dirty check's truth.
    /// Session lines (loop, playhead, tab) are left out of both sides,
    /// so moving the playhead never marks a project unsaved.
    saved_baseline: String,
    /// A discard waiting for its confirming second gesture (quit/New/
    /// Open pressed once with unsaved changes).
    pending_discard: Option<(project::Discard, std::time::Instant)>,
    /// Where a just-opened project left off, waiting for its track to
    /// finish analyzing (which resets the loop) before being applied.
    restore_session: Option<doc::Session>,
    /// View menu: pure-black stage background.
    view_black: bool,
    /// View > Half-Res Playback: render the stage at half size while the
    /// song runs. Session state like the other View toggles.
    half_res_play: bool,
    /// View menu: which Spark cursor is active (None = system arrow).
    cursor_choice: Option<usize>,
    /// The baked cursors once the compositor accepted them.
    custom_cursors: [Option<winit::window::CustomCursor>; 2],
    handle_drag: Option<HandleDrag>,
    /// The comp file Save writes to and the title bar displays.
    current_file: String,
    proxy: EventLoopProxy<AppEvent>,
    /// A picker window is up; don't spawn a second one.
    picker_busy: bool,
    audio: Option<spark_audio::Track>,
    /// Full path of the currently loaded track.
    audio_file: Option<String>,
    /// Basename of the track being decoded/analyzed right now.
    audio_loading: Option<String>,
    player: Option<spark_audio::Player>,
    /// Opening the output device failed once this session; the comp
    /// runs on its own clock and doesn't retry every frame.
    player_failed: bool,
    /// The voices last handed to the player, by their numbers — pushed
    /// again only when they change (see `sound::sync_voices`).
    voices_key: Option<sound::VoicesKey>,
    /// The sounds the comp names, decoded (or not) — see `sound`.
    sounds: std::collections::HashMap<u32, sound::Slot>,
    /// A volume box being dragged.
    vol_drag: Option<sound::VolDrag>,
    /// Playing with no track loaded: the wall-time clock the playhead rides
    /// (see [`transport::SilentClock`]). `None` means stopped. Only ever
    /// consulted when there is no `player` — a track's own cursor wins.
    silent_play: Option<transport::SilentClock>,
    transport_hover: bool,
    /// Hovering the keyframe-stamp button.
    key_hover: bool,
    /// Playhead scrubbing lands on the grid while this is on.
    snap_playhead: bool,
    /// The grid: a bar or a fraction of it — picked in the timeline's
    /// menu.
    grid_div: timeline::Grid,
    /// The song's waveform laid faintly across the whole grid.
    wave_overlay: bool,
    /// Visible slice of song time; reset when a track loads.
    time_view: timeline::TimeView,
    /// Scroll offset (physical px) for the arrangement's track rows.
    lanes_scroll: f32,
    /// Dragging the playhead along the time axis (ruler or lanes).
    timeline_scrub: bool,
    /// Loop region (seconds, bar-quantized), set by Shift+dragging the
    /// ruler; `loop_on` gates whether playback actually cycles it.
    loop_region: Option<(f32, f32)>,
    loop_on: bool,
    /// A drag on the loop brace in progress (see `transport::LoopDrag`).
    loop_drag: Option<transport::LoopDrag>,
    /// Where the stage sits in the viewport: zoom + pan over the gutter
    /// fit. Ctrl+wheel zooms at the cursor, middle-drag pans, Ctrl+0
    /// resets.
    canvas_view: view::CanvasView,
    /// A middle-button canvas pan in progress: the last cursor position.
    canvas_pan: Option<(f64, f64)>,
    /// A drag on the 3D transform gizmo, and the part under the cursor.
    gizmo_drag: Option<gizmo::Drag>,
    gizmo_hover: Option<gizmo::Part>,
    /// Which half of the gizmo is up: arrows or rings (`R`).
    gizmo_mode: gizmo::Mode,
    /// The fly view, while it's up, and where its camera was parked when
    /// it last closed (see `viewpoint`).
    fly: Option<viewpoint::Fly>,
    fly_park: viewpoint::Fly,
    /// The fly keys held (WASD, Q/E), and when they last moved the eye.
    fly_keys: viewpoint::FlyKeys,
    fly_last: Option<std::time::Instant>,
    /// A left press on empty space in the fly view, becoming a look.
    look: Option<viewpoint::Look>,
    /// View > 3D Floor: the floor grid in the comp viewer too.
    floor: bool,
    /// Hovered toolbar zoom button: 0 minus, 1 plus, 2 the 100% button.
    zoom_hover: Option<u8>,
    /// User-set bottom-panel height (logical px); the toolbar's top edge
    /// drags it, and a double-click there snaps back to the default.
    timeline_h: f32,
    /// The border drag in progress / hovered (row-resize cursor).
    panel_resize: bool,
    resize_hover: bool,
    /// Last press on the resize bar and the panel height at that moment —
    /// a quick second press with the height unmoved resets to default.
    last_resize_click: Option<(std::time::Instant, f32)>,
}

impl Studio {
    fn new(proxy: EventLoopProxy<AppEvent>) -> Self {
        let editor = Editor::empty();
        let saved_baseline = doc::serialize(&editor.to_doc());
        Self {
            window: None,
            gpu: None,
            shape_pass: None,
            stage: None,
            meshes: std::collections::HashMap::new(),
            mesh_loading: 0,
            mesh_missing: Vec::new(),
            ui_pass: None,
            bg_pass: None,
            text: None,
            editor,
            modifiers: ModifiersState::empty(),
            cursor_px: (0.0, 0.0),
            title_hover: None,
            title_pressed: None,
            bpm_edit: None,
            menu_open: None,
            menu_hover: None,
            menu_anchor_hover: None,
            ctx_menu: None,
            ctx_target: context::Target::Empty,
            ctx_at: [0.0; 2],
            ctx_hover: None,
            ctx_over: None,
            ctx_drag: None,
            ctx_edit: None,
            inspector: inspector::State::new(),
            left: left::State::new(),
            wordmark_w: 0.0,
            anchor_ws: [0.0; 4],
            menu_item_w: 0.0,
            export: None,
            export_note: None,
            comp_stack: Vec::new(),
            saved_baseline,
            pending_discard: None,
            restore_session: None,
            subcomps: std::collections::HashMap::new(),
            sub_mesh_next: comps::SUB_MESH_BASE,
            selected_clips: Vec::new(),
            clip_drag: None,
            row_drag: None,
            rows_seen: 0,
            last_clip_click: None,
            last_head_click: None,
            clip_view: None,
            view_black: false,
            half_res_play: false,
            cursor_choice: Some(0),
            custom_cursors: [None, None],
            handle_drag: None,
            current_file: editor::UNTITLED.to_string(),
            proxy,
            picker_busy: false,
            audio: None,
            audio_file: None,
            audio_loading: None,
            player: None,
            player_failed: false,
            voices_key: None,
            sounds: std::collections::HashMap::new(),
            vol_drag: None,
            silent_play: None,
            transport_hover: false,
            key_hover: false,
            snap_playhead: false,
            grid_div: timeline::Grid::default(),
            wave_overlay: false,
            // A comp keeps time before it has a song — see `Studio::grid`.
            time_view: timeline::TimeView::bars(
                &spark_audio::BeatGrid {
                    bpm: transport::SILENT_BPM,
                    first_bar: 0.0,
                },
                transport::OPEN_END,
                16.0,
            ),
            lanes_scroll: 0.0,
            timeline_scrub: false,
            loop_region: None,
            loop_on: false,
            loop_drag: None,
            canvas_view: view::CanvasView::new(),
            canvas_pan: None,
            gizmo_drag: None,
            gizmo_hover: None,
            gizmo_mode: gizmo::Mode::default(),
            fly: None,
            fly_park: viewpoint::Fly::new(spark_render::CANVAS),
            fly_keys: viewpoint::FlyKeys::default(),
            fly_last: None,
            look: None,
            floor: false,
            zoom_hover: None,
            timeline_h: DEFAULT_TIMELINE_H,
            panel_resize: false,
            resize_hover: false,
            last_resize_click: None,
        }
    }

    /// Decode + analyze a track off-thread; the result arrives as an
    /// [`AppEvent::AudioLoaded`].
    pub(crate) fn import_audio(&mut self, path: PathBuf) {
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.to_string_lossy().into_owned());
        self.audio_loading = Some(name);
        let proxy = self.proxy.clone();
        std::thread::spawn(move || {
            let path_str = path.to_string_lossy().into_owned();
            let cache = cache_dir();
            let result =
                spark_audio::Track::load_cached(&path, cache.as_deref()).map_err(|e| e.to_string());
            let _ = proxy.send_event(AppEvent::AudioLoaded(path_str, result));
        });
    }

    /// Load the comp's saved audio track if it isn't already loaded —
    /// and drop the one it no longer names.
    pub(crate) fn sync_audio(&mut self) {
        let Some(p) = self.editor.audio_path().map(str::to_string) else {
            if self.audio.is_some() && self.comp_stack.is_empty() {
                self.audio = None;
                self.audio_file = None;
            }
            return;
        };
        if self.audio_file.as_deref() == Some(p.as_str()) || self.audio_loading.is_some() {
            return;
        }
        self.import_audio(PathBuf::from(p));
    }

    fn scale(&self) -> f32 {
        self.window
            .as_ref()
            .map(|w| w.scale_factor() as f32)
            .unwrap_or(1.0)
    }

    fn layout(&self) -> Option<Layout> {
        let gpu = self.gpu.as_ref()?;
        let (w, h) = gpu.size();
        Some(Layout::compute(w, h, self.scale(), self.timeline_h))
    }

    /// The canvas-units → window-px mapping for this frame's layout.
    fn canvas_map(&self, layout: &Layout) -> view::CanvasMap {
        self.canvas_view.map(layout.viewport, self.editor.canvas())
    }

    fn title_bar(&self) -> Option<TitleBar> {
        Some(TitleBar::new(
            self.layout()?.title,
            self.scale(),
            self.wordmark_w,
        ))
    }

    fn menus(&self) -> Option<[Menu; 4]> {
        Some(menu::build(
            &self.layout()?,
            self.scale(),
            self.anchor_ws,
            self.menu_item_w,
        ))
    }

    fn request_redraw(&self) {
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }

    /// Launch the file picker unless one is already up.
    fn spawn_picker(&mut self, purpose: picker::Purpose) {
        if !self.picker_busy {
            self.picker_busy = true;
            picker::spawn(self.proxy.clone(), purpose, &self.current_file);
        }
    }
}

fn main() {
    help::banner();
    let event_loop = EventLoop::<AppEvent>::with_user_event()
        .build()
        .expect("create event loop");
    event_loop.set_control_flow(ControlFlow::Wait);
    let mut studio = Studio::new(event_loop.create_proxy());
    event_loop.run_app(&mut studio).expect("run event loop");
}
