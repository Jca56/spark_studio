mod chrome;
mod editor;
mod history;
mod input;
mod inspector;
mod layers;
mod menu;
mod picker;
mod render;
mod timeline;

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
use winit::keyboard::{Key, ModifiersState, NamedKey};
use winit::window::{Window, WindowId};

/// Results posted back to the event loop from worker threads.
enum AppEvent {
    /// The file picker closed: the chosen path, or `None` on cancel.
    Picked(picker::Purpose, Option<PathBuf>),
    /// Off-thread decode + analysis of the given path finished.
    AudioLoaded(String, Result<spark_audio::Track, String>),
}

/// App icon baked to raw RGBA (64x64) from spark_studio.svg — no image
/// decoding at runtime.
const APP_ICON: &[u8] = include_bytes!("../assets/spark_icon_64.rgba");

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
    menu_open: bool,
    menu_hover: Option<usize>,
    menu_anchor_hover: bool,
    wordmark_w: f32,
    /// Measured label widths for the File menu, cached between frames.
    file_w: f32,
    menu_item_w: f32,
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
}

impl Studio {
    fn new(proxy: EventLoopProxy<AppEvent>) -> Self {
        Self {
            window: None,
            gpu: None,
            shape_pass: None,
            ui_pass: None,
            text: None,
            editor: Editor::new(),
            modifiers: ModifiersState::empty(),
            cursor_px: (0.0, 0.0),
            title_hover: None,
            title_pressed: None,
            tool_hover: None,
            slider_drag: None,
            layer_drag: None,
            menu_open: false,
            menu_hover: None,
            menu_anchor_hover: false,
            wordmark_w: 0.0,
            file_w: 0.0,
            menu_item_w: 0.0,
            current_file: editor::COMP_PATH.to_string(),
            proxy,
            picker_busy: false,
            audio: None,
            audio_file: None,
            audio_loading: None,
            player: None,
            transport_hover: false,
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

    fn toggle_play(&mut self) -> bool {
        match &self.player {
            Some(p) => {
                p.toggle();
                true
            }
            None => false,
        }
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
        Some(Layout::compute(w, h, self.scale()))
    }

    fn title_bar(&self) -> Option<TitleBar> {
        Some(TitleBar::new(
            self.layout()?.title,
            self.scale(),
            self.wordmark_w,
        ))
    }

    fn toolbar(&self) -> Option<IconBar<Tool>> {
        Some(IconBar::new(self.layout()?.top, self.scale(), &TOOLS))
    }

    fn file_menu(&self) -> Option<Menu> {
        Some(menu::build(
            &self.layout()?,
            self.scale(),
            self.file_w,
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
        self.text = Some(Text::new(&gpu.device, &gpu.queue, gpu.surface_format()));
        self.gpu = Some(gpu);
        self.window = Some(window);
        // The startup comp may reference a track — bring it back too.
        self.sync_audio();
        self.request_redraw();
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::ModifiersChanged(m) => self.modifiers = m.state(),
            WindowEvent::CursorMoved { position, .. } => {
                self.cursor_px = (position.x, position.y);
                let mut dirty = false;
                if let Some(layout) = self.layout() {
                    dirty |= self
                        .editor
                        .set_cursor(position.x, position.y, layout.viewport);
                    if let Some(prop) = self.slider_drag {
                        if let Some(props) = self.editor.selected_props() {
                            let insp = inspector::build(layout.left, self.scale(), &props);
                            if let Some(row) = insp.rows.iter().find(|r| r.prop == prop) {
                                let t = (position.x as f32 - row.track.x) / row.track.w;
                                self.editor.set_prop(prop, inspector::value_for(prop, t));
                                dirty = true;
                            }
                        } else {
                            self.slider_drag = None;
                        }
                    }
                    if let Some(from) = self.layer_drag {
                        let rows = layers::rows(
                            layout.right,
                            self.scale(),
                            self.editor.shapes(),
                            self.editor.selection(),
                        );
                        if let Some(to) = layers::hit(&rows, position.x as f32, position.y as f32)
                            && self.editor.move_layer(from, to)
                        {
                            self.layer_drag = Some(to);
                            dirty = true;
                        }
                    }
                }
                let hover = self
                    .title_bar()
                    .and_then(|tb| tb.hit(position.x as f32, position.y as f32));
                if hover != self.title_hover {
                    self.title_hover = hover;
                    dirty = true;
                }
                let tool_hover = self
                    .toolbar()
                    .and_then(|bar| bar.hit(position.x as f32, position.y as f32));
                if tool_hover != self.tool_hover {
                    self.tool_hover = tool_hover;
                    dirty = true;
                }
                if self.audio.is_some()
                    && let Some(layout) = self.layout()
                {
                    let strip = timeline::strip(layout.timeline, self.scale());
                    let hover = strip.button.contains(position.x as f32, position.y as f32);
                    if hover != self.transport_hover {
                        self.transport_hover = hover;
                        dirty = true;
                    }
                }
                if let Some(m) = self.file_menu() {
                    let anchor_hover = m.hit_anchor(position.x as f32, position.y as f32);
                    if anchor_hover != self.menu_anchor_hover {
                        self.menu_anchor_hover = anchor_hover;
                        dirty = true;
                    }
                    if self.menu_open {
                        let hover = m.hit_item(position.x as f32, position.y as f32);
                        if hover != self.menu_hover {
                            self.menu_hover = hover;
                            dirty = true;
                        }
                    }
                }
                if dirty {
                    self.request_redraw();
                }
            }
            WindowEvent::MouseInput {
                state,
                button: MouseButton::Left,
                ..
            } => match state {
                ElementState::Pressed => self.press(event_loop),
                ElementState::Released => self.release(event_loop),
            },
            WindowEvent::MouseWheel { delta, .. } => {
                let dy = match delta {
                    MouseScrollDelta::LineDelta(_, y) => y,
                    MouseScrollDelta::PixelDelta(p) => p.y as f32 / 40.0,
                };
                if self.editor.wheel(dy, self.modifiers.shift_key()) {
                    self.request_redraw();
                }
            }
            WindowEvent::KeyboardInput { event, .. } if event.state.is_pressed() => {
                let dirty = match &event.logical_key {
                    Key::Named(NamedKey::Escape) => {
                        if self.menu_open {
                            self.menu_open = false;
                            true
                        } else {
                            self.editor.deselect()
                        }
                    }
                    Key::Named(NamedKey::Delete) | Key::Named(NamedKey::Backspace) => {
                        self.editor.delete_selected()
                    }
                    Key::Named(NamedKey::Space) => self.toggle_play(),
                    Key::Character(c) if c == " " => self.toggle_play(),
                    Key::Character(c) => {
                        let ctrl = self.modifiers.control_key();
                        let key = c.to_lowercase();
                        if ctrl && key == "q" {
                            event_loop.exit();
                            false
                        } else if ctrl && key == "s" {
                            self.editor.save(&self.current_file);
                            false
                        } else if ctrl && key == "o" {
                            self.spawn_picker(picker::Purpose::OpenComp);
                            false
                        } else {
                            self.editor.char_key(&key, ctrl, self.modifiers.shift_key())
                        }
                    }
                    _ => false,
                };
                if dirty {
                    self.request_redraw();
                }
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
         Layers: click a row to select | drag rows to reorder the stack\n\
         Undo:   Ctrl+Z undo | Ctrl+Shift+Z redo\n\
         Comp:   File menu or Ctrl+S save | Ctrl+O open | Esc deselect | Ctrl+Q quit\n"
    );
    let event_loop = EventLoop::<AppEvent>::with_user_event()
        .build()
        .expect("create event loop");
    event_loop.set_control_flow(ControlFlow::Wait);
    let mut studio = Studio::new(event_loop.create_proxy());
    event_loop.run_app(&mut studio).expect("run event loop");
}
