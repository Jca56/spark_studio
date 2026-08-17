mod anim;
mod chrome;
mod colorhome;
mod cursor;
mod doc;
mod drag;
mod editor;
mod handles;
mod history;
mod hotkeys;
mod input;
mod lanes;
mod layers;
mod menu;
mod picker;
mod project;
mod props;
mod render;
mod timeline;
mod transport;
mod view;

use std::path::PathBuf;
use std::sync::Arc;

use editor::{Editor, Prop, Tool};
use spark_render::{Gpu, ShapePass};
use spark_text::Text;
use spark_ui::{
    ICON_ARROW, ICON_CIRCLE, ICON_LINE, ICON_PENTAGON, ICON_SQUARE, IconBar, Layout, Menu,
    TitleAction, TitleBar, UiPass,
};
use winit::application::ApplicationHandler;
use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
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
enum PickerDrag {
    Sv,
    Hue,
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

/// Toolbar buttons: tool + icon glyph, in display order.
const TOOLS: [(Tool, f32); 5] = [
    (Tool::Select, ICON_ARROW),
    (Tool::Circle, ICON_CIRCLE),
    (Tool::Box, ICON_SQUARE),
    (Tool::Polygon, ICON_PENTAGON),
    (Tool::Line, ICON_LINE),
];

struct Studio {
    window: Option<Arc<Window>>,
    gpu: Option<Gpu>,
    shape_pass: Option<ShapePass>,
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
    slider_drag: Option<Prop>,
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
    /// A scrub-field drag: property, last cursor y, and whether the drag
    /// actually moved (a clean click opens the field for typing instead).
    scrub_drag: Option<(ScrubTarget, Prop, f64, bool)>,
    /// A scrub field being text-edited: the target, property, typed buffer.
    field_edit: Option<(ScrubTarget, Prop, String)>,
    /// Hovered card cogwheel (shape index).
    cog_hover: Option<usize>,
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
    transport_hover: bool,
    /// Hovering the keyframe-stamp button.
    key_hover: bool,
    /// The bottom-panel tab currently showing.
    timeline_tab: timeline::Tab,
    /// Which of Arrange/Keys the rectangular toggle button holds — what a
    /// click on it from the Wave tab returns to.
    edit_tab: timeline::Tab,
    /// Visible slice of song time; reset when a track loads.
    time_view: timeline::TimeView,
    /// Scroll offset (physical px) for the timeline's keyframe lanes.
    lanes_scroll: f32,
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
            ui_pass: None,
            bg_pass: None,
            text: None,
            editor: Editor::new(),
            modifiers: ModifiersState::empty(),
            cursor_px: (0.0, 0.0),
            title_hover: None,
            title_pressed: None,
            tool_hover: None,
            slider_drag: None,
            layer_drag: None,
            folder_drag: None,
            menu_open: None,
            menu_hover: None,
            menu_anchor_hover: None,
            wordmark_w: 0.0,
            anchor_ws: [0.0; 2],
            menu_item_w: 0.0,
            view_black: false,
            cursor_choice: Some(0),
            custom_cursors: [None, None],
            rename: None,
            last_layer_click: None,
            last_folder_click: None,
            rename_folder: None,
            layers_scroll: 0.0,
            card_open: None,
            scrub_drag: None,
            field_edit: None,
            cog_hover: None,
            handle_drag: None,
            picker_hsv: None,
            grad_edit_b: false,
            picker_drag: None,
            current_file: editor::COMP_PATH.to_string(),
            proxy,
            picker_busy: false,
            audio: None,
            audio_file: None,
            audio_loading: None,
            player: None,
            transport_hover: false,
            key_hover: false,
            timeline_tab: timeline::Tab::Wave,
            edit_tab: timeline::Tab::Arrange,
            time_view: timeline::TimeView::new(0.0, 1.0),
            lanes_scroll: 0.0,
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
        if let Some([h, s, v]) = self.picker_hsv {
            let srgb = spark_ui::picker::hsv_to_rgb(h, s, v);
            let lin = [
                spark_ui::picker::srgb_to_linear(srgb[0]),
                spark_ui::picker::srgb_to_linear(srgb[1]),
                spark_ui::picker::srgb_to_linear(srgb[2]),
            ];
            self.editor.set_rgb_selection(lin, self.grad_edit_b);
        }
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
            self.picker_hsv = Some(input::hsv_of_linear(self.editor.color()));
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
            WindowEvent::MouseWheel { delta, .. } => {
                let dy = match delta {
                    MouseScrollDelta::LineDelta(_, y) => y,
                    MouseScrollDelta::PixelDelta(p) => p.y as f32 / 40.0,
                };
                let (cx, cy) = (self.cursor_px.0 as f32, self.cursor_px.1 as f32);
                let Some(layout) = self.layout() else { return };
                // The wheel acts on whatever it's over: shapes in the
                // viewport, scrolling in the side panels.
                if layout.viewport.contains(cx, cy) {
                    if self.modifiers.control_key() {
                        // Ctrl+wheel zooms the canvas at the cursor — the
                        // timeline recipe, applied to the stage.
                        let factor = 1.18f32.powf(dy);
                        self.canvas_view
                            .zoom_at(factor, cx, cy, layout.viewport, self.scale());
                        self.request_redraw();
                    } else if self.editor.wheel(dy, self.modifiers.shift_key()) {
                        self.request_redraw();
                    }
                } else if layout.right.contains(cx, cy) {
                    // Only the cards list scrolls; the color home is pinned.
                    let (_, cards_vp) =
                        colorhome::split(layout.right, self.scale(), self.picker_hsv.is_some());
                    if cards_vp.contains(cx, cy) {
                        self.layers_scroll =
                            (self.layers_scroll - dy * 60.0 * self.scale()).max(0.0);
                        self.request_redraw();
                    }
                } else if layout.timeline.contains(cx, cy)
                    && let Some(track) = &self.audio
                {
                    let panel = timeline::panel(layout.timeline, self.scale());
                    if self.modifiers.control_key() {
                        // Zoom around the time under the cursor.
                        let pivot = self.time_view.t_at(cx, panel.axis);
                        let factor = (1.0f32 / 1.18).powf(dy);
                        self.time_view.zoom(factor, pivot, track.duration);
                    } else if self.modifiers.shift_key() {
                        let dt = -dy * self.time_view.span() * 0.10;
                        self.time_view.pan(dt, track.duration);
                    } else {
                        self.lanes_scroll = (self.lanes_scroll - dy * 60.0 * self.scale()).max(0.0);
                    }
                    self.request_redraw();
                }
            }
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
                // Playback drives continuous redraw only while playing.
                if self.player.as_ref().is_some_and(|p| p.is_playing()) {
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
        self.ui_pass = None;
        self.bg_pass = None;
        self.text = None;
        self.gpu = None;
        self.window = None;
    }
}

fn main() {
    println!(
        "\nSpark Studio — comp editor v0 (status prints here until in-app UI lands)\n\
         \n\
         Tools:  1 select/move   2 circle   3 box   4 polygon   5 line\n\
         Draw:   click-drag in the viewport\n\
         Edit:   drag move | scroll scale | Shift+scroll or Q/E rotate\n\
                 [ ] polygon sides | C color | T outline/fill\n\
                 A/Z glow +/- | W/S brightness +/- | X or Del delete\n\
         Paths:  P make editable | drag points | = add point | - remove | O open/close\n\
         Layers: click a row to select | Shift+click a range | Ctrl+click toggles one\n\
                 drag rows to reorder the stack | Ctrl+D duplicate\n\
         Folder: Ctrl+Shift+N puts the selected layers in a folder | +/- collapses\n\
                 X/Y/R/S on the header moves everything inside, about its own center\n\
                 drag the header to reorder the whole run | K keys the folder too\n\
                 click the header to select its contents | double-click renames\n\
                 the folder eye hides everything inside | right-click dissolves it\n\
                 drag a card onto a header to file it; onto a loose card to pull it out\n\
         Merge:  Ctrl+G merges the selection into one layer (colors + keys kept)\n\
                 Ctrl+Shift+G unmerges | File > Save/Import Shape... reuses selections\n\
         Anim:   K or the diamond button stamps the selection's pose as a keyframe\n\
                 posing without stamping is a preview — it reverts when the playhead moves\n\
                 folders key too — their lane sits above its members in Keys\n\
                 drag keys to retime (16th grid) | Alt+drag copies | right-click deletes\n\
                 Ctrl+drag empty lane space box-selects keys | Shift+click adds/removes\n\
                 Ctrl+C copies selected keys | Ctrl+V pastes at playhead\n\
                 Ctrl+Shift+V repeat-pastes bar-aligned (to loop end, else x4)\n\
                 arrows jump playhead between keys | , . nudge selected keys a 16th\n\
                 Ctrl+click a key: smooth (diamond) <-> linear (square)\n\
         Loop:   Shift+drag the ruler brackets bars | L toggles | right-click clears\n\
         View:   Ctrl+wheel zoom at cursor | Shift+wheel pan | wheel scrolls lanes\n\
         Canvas: Ctrl+wheel zoom at cursor | middle-drag pan | Ctrl+0 back to 100%\n\
                 zoom bar bottom-right: - + steppers, 100% refit, live readout\n\
         Cards:  each layer card owns its shape: drag X/Y/R/S up/down to scrub,\n\
                 click one to type the value (Enter commits, Esc cancels)\n\
                 eye toggles visibility | cogwheel expands full settings\n\
         Color:  the color home is the *current color* — swatches, picker, hex\n\
                 selecting a layer never changes it; Alt+click a shape or I eyedrops\n\
                 with a selection, editing the color paints it too | C cycles palette\n\
         React:  Keys-tab sidebar sliders set how hard each shape rides the track\n\
         Undo:   Ctrl+Z undo | Ctrl+Shift+Z redo\n\
         Comp:   File > New for a blank project | Ctrl+S save | Ctrl+O open\n\
         Layout: drag the toolbar's top edge to resize the bottom panel; double-click resets\n\
                 the square wave button opens the Wave tab; the wide button flips Arrange/Keys\n\
         Misc:   Esc deselect | Ctrl+Q quit\n"
    );
    let event_loop = EventLoop::<AppEvent>::with_user_event()
        .build()
        .expect("create event loop");
    event_loop.set_control_flow(ControlFlow::Wait);
    let mut studio = Studio::new(event_loop.create_proxy());
    event_loop.run_app(&mut studio).expect("run event loop");
}
