mod anim;
mod browser;
mod chrome;
mod colorhome;
mod cursor;
mod doc;
mod drag;
mod editor;
mod fx;
mod handles;
mod help;
mod history;
mod hotkeys;
mod input;
mod lanes;
mod layers;
mod materials;
mod menu;
mod picker;
mod project;
mod props;
mod random;
mod render;
mod status;
mod textbox;
mod timeline;
mod transport;
mod view;

use std::path::PathBuf;
use std::sync::Arc;

use editor::{Editor, Prop, Tool};
use props::TOOLS;
use spark_render::{Gpu, ShapePass, Stage};
use spark_text::Text;
use spark_ui::{IconBar, Layout, Menu, TitleAction, TitleBar, UiPass};
use winit::application::ApplicationHandler;
use winit::event::{ElementState, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy};
use winit::keyboard::ModifiersState;
use winit::window::{Window, WindowId};

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
}

/// What a scrub field / typed value is editing: the primary selection's
/// shape, or a folder's transform.
#[derive(Clone, Copy, PartialEq)]
pub(crate) enum ScrubTarget {
    Shape,
    Folder(u32),
}

/// Which picker surface a drag started on.
#[derive(Clone, Copy, PartialEq, Eq)]
enum PickerDrag {
    Sv,
    Hue,
    Alpha,
}

/// A rubber-band drag over the lanes: press corner, current corner, and
/// the selection it extends when Shift was down at press.
pub(crate) struct BoxSel {
    pub(crate) x0: f32,
    pub(crate) y0: f32,
    pub(crate) x1: f32,
    pub(crate) y1: f32,
    /// Only becomes a box after the cursor actually travels; a still click
    /// is a seek.
    pub(crate) moved: bool,
    pub(crate) prev: Vec<(anim::Owner, f32)>,
}

/// Results posted back to the event loop from worker threads.
enum AppEvent {
    /// The file picker closed: the chosen path, or `None` on cancel.
    Picked(picker::Purpose, Option<PathBuf>),
    /// Off-thread decode + analysis of the given path finished.
    AudioLoaded(String, Result<spark_audio::Track, String>),
}

/// App icon baked to raw RGBA (64x64) from assets/spark_studio_icon.svg —
/// no image decoding at runtime.
const APP_ICON: &[u8] = include_bytes!("../assets/spark_icon_64.rgba");

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
    tool_hover: Option<Tool>,
    /// A detail slider being dragged, and whose it is — a folder's fade
    /// rides the same machinery as a shape's, the way its scrub fields do.
    slider_drag: Option<(ScrubTarget, Prop)>,
    /// An effect parameter slider being dragged: (shape, effect, parameter).
    fx_slider_drag: Option<(usize, u32, u8)>,
    /// The material playground (View > Materials): open, which of the seven
    /// surfaces is being tuned, how far the panel is scrolled, and any knob
    /// currently under the cursor.
    materials_open: bool,
    material_tab: materials::Tab,
    material_pick: usize,
    /// The color slot being typed into, and the hex typed so far.
    material_edit: Option<(materials::Edit, String)>,
    /// The playground colour the right panel's picker is painting. Separate
    /// from `material_edit`, which is only the typed buffer: pressing Enter
    /// closes the code field but must not take the picker's hold on the
    /// colour away with it.
    material_target: Option<materials::Edit>,
    /// The transport's tempo field while it is being typed into.
    bpm_edit: Option<String>,
    material_drag: Option<materials::Knob>,
    /// Current stack index of the layer row being dragged to reorder.
    layer_drag: Option<usize>,
    /// A folder header being dragged to reorder — the whole run moves.
    folder_drag: Option<u32>,
    /// Which menu-bar menu is open (index into the menus array), if any.
    menu_open: Option<usize>,
    menu_hover: Option<usize>,
    menu_anchor_hover: Option<usize>,
    wordmark_w: f32,
    /// Measured anchor label widths ("File", "View"), cached between frames.
    anchor_ws: [f32; 2],
    menu_item_w: f32,
    /// View menu: pure-black stage background.
    view_black: bool,
    /// View > Half-Res Playback: render the stage at half size while the
    /// song runs. Session state like the other View toggles.
    half_res_play: bool,
    /// View menu: which Spark cursor is active (None = system arrow).
    cursor_choice: Option<usize>,
    /// The baked cursors once the compositor accepted them.
    custom_cursors: [Option<winit::window::CustomCursor>; 2],
    /// In-progress layer rename buffer (double-click a layer row to start,
    /// Enter commits).
    rename: Option<String>,
    /// Last layer-row click, for double-click detection.
    last_layer_click: Option<(usize, std::time::Instant)>,
    /// Same, for folder headers.
    last_folder_click: Option<(u32, std::time::Instant)>,
    /// The rename in `rename` targets this folder rather than a layer.
    rename_folder: Option<u32>,
    /// Scroll offset (physical px) for the layer-cards list.
    layers_scroll: f32,
    /// The one cog-expanded layer card (shape index), if any.
    card_open: Option<usize>,
    /// Which half the expanded card is showing — the cog's settings, or the
    /// effects button's stack.
    card_tab: layers::CardTab,
    /// A scrub-field drag: property, last cursor y, and whether the drag
    /// actually moved (a clean click opens the field for typing instead).
    scrub_drag: Option<(ScrubTarget, Prop, f64, bool)>,
    /// A scrub field being text-edited: the target, property, typed buffer.
    field_edit: Option<(ScrubTarget, Prop, textbox::TextBox)>,
    /// Where each char boundary of the edited field sits on screen, as
    /// `(byte offset, x)`. Rebuilt every redraw, because only the frame
    /// loop holds the text engine that can measure it — clicks and drags
    /// then map a pixel to a caret without needing a font.
    field_caret_xs: Vec<(usize, f32)>,
    /// A click-drag selecting inside the edited field.
    field_drag: bool,
    /// The browser row under the cursor.
    fx_browser_hover: Option<fx::EffectKind>,
    /// An effect being dragged out of the browser, and the layer card it
    /// would land on. The drag names its target, so it works regardless of
    /// what happens to be selected.
    fx_drag: Option<fx::EffectKind>,
    fx_drop: Option<usize>,
    /// The card button under the cursor, if any — what gets the hover wash.
    /// Only ever a button (see `CardHit::hoverable`).
    card_hover: Option<layers::CardHit>,
    handle_drag: Option<HandleDrag>,
    /// Color picker: open flag doubles as the H/S/V state.
    picker_hsv: Option<[f32; 3]>,
    /// Palette/picker edits hit gradient endpoint B instead of the base
    /// color (only meaningful while the primary's gradient is on).
    grad_edit_b: bool,
    picker_drag: Option<PickerDrag>,
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
    /// Playing with no track loaded: the wall-time clock the playhead rides
    /// (see [`transport::SilentClock`]). `None` means stopped. Only ever
    /// consulted when there is no `player` — a track's own cursor wins.
    silent_play: Option<transport::SilentClock>,
    transport_hover: bool,
    /// Hovering the keyframe-stamp button.
    key_hover: bool,
    /// The bottom-panel tab currently showing.
    timeline_tab: timeline::Tab,
    /// Playhead scrubbing lands on quarter-bars (beats) while this is on.
    snap_playhead: bool,
    /// Visible slice of song time; reset when a track loads.
    time_view: timeline::TimeView,
    /// Scroll offset (physical px) for the timeline's keyframe lanes.
    lanes_scroll: f32,
    /// The one cog-expanded lane in the Keys tab, if any.
    lane_open: Option<anim::Owner>,
    /// Dragging the playhead along the time axis (ruler or lanes).
    timeline_scrub: bool,
    /// A lane key drag: (owner, the keys' current time, and whether the
    /// first move should copy instead of retime — Alt+drag).
    key_drag: Option<(anim::Owner, f32, bool)>,
    /// The highlighted lane keys: (owner, key time) each. Delete removes
    /// them instead of the shape; group drags move them together.
    selected_keys: Vec<(anim::Owner, f32)>,
    /// A rubber-band selection in the lanes, in progress.
    box_sel: Option<BoxSel>,
    /// Loop region (seconds, bar-quantized), set by Shift+dragging the
    /// ruler; `loop_on` gates whether playback actually cycles it.
    loop_region: Option<(f32, f32)>,
    loop_on: bool,
    /// A Shift+drag on the ruler in progress: the anchor bar.
    loop_drag: Option<f32>,
    /// Where the stage sits in the viewport: zoom + pan over the gutter
    /// fit. Ctrl+wheel zooms at the cursor, middle-drag pans, Ctrl+0
    /// resets.
    canvas_view: view::CanvasView,
    /// A middle-button canvas pan in progress: the last cursor position.
    canvas_pan: Option<(f64, f64)>,
    /// Hovered zoom-bar button: 0 minus, 1 plus, 2 the 100% button.
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
        Self {
            window: None,
            gpu: None,
            shape_pass: None,
            stage: None,
            ui_pass: None,
            bg_pass: None,
            text: None,
            editor: Editor::empty(),
            modifiers: ModifiersState::empty(),
            cursor_px: (0.0, 0.0),
            title_hover: None,
            title_pressed: None,
            tool_hover: None,
            slider_drag: None,
            fx_slider_drag: None,
            materials_open: false,
            material_tab: materials::Tab::default(),
            material_pick: 0,
            material_edit: None,
            material_target: None,
            bpm_edit: None,
            material_drag: None,
            layer_drag: None,
            folder_drag: None,
            menu_open: None,
            menu_hover: None,
            menu_anchor_hover: None,
            wordmark_w: 0.0,
            anchor_ws: [0.0; 2],
            menu_item_w: 0.0,
            view_black: false,
            half_res_play: false,
            cursor_choice: Some(0),
            custom_cursors: [None, None],
            rename: None,
            last_layer_click: None,
            last_folder_click: None,
            rename_folder: None,
            layers_scroll: 0.0,
            card_open: None,
            card_tab: layers::CardTab::default(),
            scrub_drag: None,
            field_edit: None,
            field_caret_xs: Vec::new(),
            field_drag: false,
            fx_browser_hover: None,
            fx_drag: None,
            fx_drop: None,
            card_hover: None,
            handle_drag: None,
            picker_hsv: None,
            grad_edit_b: false,
            picker_drag: None,
            current_file: editor::UNTITLED.to_string(),
            proxy,
            picker_busy: false,
            audio: None,
            audio_file: None,
            audio_loading: None,
            player: None,
            silent_play: None,
            transport_hover: false,
            key_hover: false,
            timeline_tab: timeline::Tab::Wave,
            snap_playhead: false,
            // A comp keeps time before it has a song — see `Studio::grid`.
            time_view: timeline::TimeView::bars(
                &spark_audio::BeatGrid {
                    bpm: transport::SILENT_BPM,
                    first_bar: 0.0,
                },
                transport::SILENT_DURATION,
                16.0,
            ),
            lanes_scroll: 0.0,
            lane_open: None,
            timeline_scrub: false,
            key_drag: None,
            selected_keys: Vec::new(),
            box_sel: None,
            loop_region: None,
            loop_on: false,
            loop_drag: None,
            canvas_view: view::CanvasView::new(),
            canvas_pan: None,
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
            let result = spark_audio::Track::load(&path).map_err(|e| e.to_string());
            let _ = proxy.send_event(AppEvent::AudioLoaded(path_str, result));
        });
    }

    /// Load the comp's saved audio track if it isn't already loaded.
    pub(crate) fn sync_audio(&mut self) {
        let Some(p) = self.editor.audio_path().map(str::to_string) else {
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
        self.canvas_view.map(layout.viewport, self.scale())
    }

    fn title_bar(&self) -> Option<TitleBar> {
        Some(TitleBar::new(
            self.layout()?.title,
            self.scale(),
            self.wordmark_w,
        ))
    }

    fn toolbar(&self) -> Option<IconBar<Tool>> {
        Some(IconBar::new(self.layout()?.tools, self.scale(), &TOOLS))
    }

    fn menus(&self) -> Option<[Menu; 2]> {
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

    /// Push the picker's current H/S/V onto every selected shape (as
    /// linear RGB).
    fn apply_picker(&mut self) {
        let Some([h, s, v]) = self.picker_hsv else {
            return;
        };
        let srgb = spark_ui::picker::hsv_to_rgb(h, s, v);
        let lin = [
            spark_ui::picker::srgb_to_linear(srgb[0]),
            spark_ui::picker::srgb_to_linear(srgb[1]),
            spark_ui::picker::srgb_to_linear(srgb[2]),
        ];
        match self.chrome_target() {
            // The square and the hue bar say nothing about transparency, so
            // they must not quietly reset it: whatever alpha the colour
            // already carries is carried through.
            Some(t) => {
                let was = materials::color_of(t, self.material_pick)[3];
                materials::set_color(t, self.material_pick, [lin[0], lin[1], lin[2], was]);
            }
            None => {
                self.editor.set_rgb_selection(lin, self.grad_edit_b);
            }
        }
    }

    /// Set the opacity of the chrome colour the picker has hold of.
    pub(crate) fn apply_alpha(&mut self, a: f32) {
        let Some(t) = self.chrome_target() else {
            return;
        };
        let mut c = materials::color_of(t, self.material_pick);
        c[3] = a.clamp(0.0, 1.0);
        materials::set_color(t, self.material_pick, c);
    }

    /// Finish an in-progress rename against whichever thing it targets —
    /// a folder header or a layer card.
    pub(crate) fn commit_rename(&mut self, buf: String) -> bool {
        match self.rename_folder.take() {
            Some(id) => self.editor.rename_folder(id, buf),
            None => self.editor.rename_primary(buf),
        }
    }

    /// Hold the open picker's H/S/V on the current color. Anything that can
    /// move the current color from *outside* the picker (swatch, eyedropper,
    /// gradient chip, `C`) has to call this, or the square drifts away from
    /// the bar and the next drag yanks the color somewhere unrelated.
    pub(crate) fn sync_picker(&mut self) {
        if self.picker_hsv.is_some() {
            let rgb = match self.chrome_target() {
                Some(t) => {
                    let c = materials::color_of(t, self.material_pick);
                    [c[0], c[1], c[2]]
                }
                None => self.editor.color(),
            };
            self.picker_hsv = Some(input::hsv_of_linear(rgb));
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

impl ApplicationHandler<AppEvent> for Studio {
    fn user_event(&mut self, _event_loop: &ActiveEventLoop, event: AppEvent) {
        self.app_event(event);
    }

    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let attrs = Window::default_attributes()
            .with_title("Spark Studio")
            .with_decorations(false)
            .with_maximized(true);
        let window = Arc::new(event_loop.create_window(attrs).expect("create window"));
        let size = window.inner_size();
        let gpu = Gpu::new(window.clone(), size.width, size.height);
        self.shape_pass = Some(ShapePass::new(&gpu.device, gpu.surface_format()));
        self.stage = Some(Stage::new(&gpu.device, gpu.surface_format()));
        self.ui_pass = Some(UiPass::new(
            &gpu.device,
            &gpu.queue,
            gpu.surface_format(),
            APP_ICON,
            64,
        ));
        self.bg_pass = Some(UiPass::new(
            &gpu.device,
            &gpu.queue,
            gpu.surface_format(),
            APP_ICON,
            64,
        ));
        self.text = Some(Text::new(&gpu.device, &gpu.queue, gpu.surface_format()));
        self.gpu = Some(gpu);
        self.make_cursors(event_loop, &window);
        self.window = Some(window);
        self.apply_cursor();
        // The startup comp may reference a track — bring it back too.
        self.sync_audio();
        self.request_redraw();
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::ModifiersChanged(m) => self.modifiers = m.state(),
            WindowEvent::CursorMoved { position, .. } => self.cursor_moved(position.x, position.y),
            WindowEvent::MouseInput {
                state,
                button: MouseButton::Left,
                ..
            } => match state {
                ElementState::Pressed => self.press(event_loop),
                ElementState::Released => self.release(event_loop),
            },
            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                button: MouseButton::Right,
                ..
            } => self.right_press(),
            WindowEvent::MouseInput {
                state,
                button: MouseButton::Middle,
                ..
            } => {
                // Middle-drag pans the canvas; anywhere else it's inert.
                let (cx, cy) = (self.cursor_px.0 as f32, self.cursor_px.1 as f32);
                self.canvas_pan = match state {
                    ElementState::Pressed
                        if self.layout().is_some_and(|l| l.viewport.contains(cx, cy)) =>
                    {
                        Some(self.cursor_px)
                    }
                    _ => None,
                };
            }
            WindowEvent::MouseWheel { delta, .. } => self.wheel(delta),
            WindowEvent::KeyboardInput { event, .. } if event.state.is_pressed() => {
                self.key_input(event_loop, &event.logical_key)
            }
            WindowEvent::ScaleFactorChanged { .. } => self.request_redraw(),
            WindowEvent::Resized(size) => {
                if let Some(gpu) = &mut self.gpu {
                    gpu.resize(size.width, size.height);
                }
                self.request_redraw();
            }
            WindowEvent::RedrawRequested => {
                self.redraw();
                // Playback drives continuous redraw only while playing —
                // on either clock, the audio stream's or the silent one.
                if self.playing() {
                    self.request_redraw();
                }
            }
            _ => {}
        }
    }

    /// Tear down GPU state while the event loop (and thus the display
    /// connection) is still alive — dropping the surface after the loop dies
    /// segfaults in the driver. Order matters: passes and text hold device
    /// handles, the surface holds the window.
    fn exiting(&mut self, _event_loop: &ActiveEventLoop) {
        self.shape_pass = None;
        self.stage = None;
        self.ui_pass = None;
        self.bg_pass = None;
        self.text = None;
        self.gpu = None;
        self.window = None;
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
