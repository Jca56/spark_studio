//! The window's event loop: winit's `ApplicationHandler` for the studio —
//! the window and GPU come up on `resumed`, every window event is routed
//! to its handler, and the GPU is torn down on `exiting`. Split from main
//! so the state and its constructor stay readable.

use std::sync::Arc;

use spark_render::{Gpu, ShapePass, Stage};
use spark_text::Text;
use spark_ui::UiPass;
use winit::application::ApplicationHandler;
use winit::event::{ElementState, MouseButton, WindowEvent};
use winit::event_loop::ActiveEventLoop;
use winit::window::{Window, WindowId};

use crate::{APP_ICON, AppEvent, Studio};

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
        self.stage = Some(Stage::new(&gpu.device, &gpu.queue, gpu.surface_format()));
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
            WindowEvent::CloseRequested => {
                // The compositor's close asks the same question Ctrl+Q
                // does: unsaved work gets a say before it goes.
                if self.confirm_discard(crate::project::Discard::Quit) {
                    event_loop.exit();
                }
            }
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
                state,
                button: MouseButton::Right,
                ..
            } => match state {
                // In the fly view a right-drag pans; over the viewport and
                // the side panels it opens the context menu; elsewhere the
                // right button acts on the timeline. A menu already up
                // closes first, so a second right-click moves it.
                ElementState::Pressed => {
                    self.context_close();
                    if !self.pan_press() && !self.context_press() {
                        self.right_press();
                    }
                }
                ElementState::Released => self.canvas_pan = None,
            },
            WindowEvent::MouseInput {
                state,
                button: MouseButton::Middle,
                ..
            } => {
                // Middle-drag pans — the canvas, or the fly view's eye;
                // anywhere else it's inert.
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
            WindowEvent::KeyboardInput { event, .. } => {
                // The fly view's WASD/QE are held keys, so their releases
                // matter; everything else acts on the press.
                let down = event.state.is_pressed();
                if !self.fly_key(&event.physical_key, down) && down {
                    self.key_input(event_loop, &event.logical_key)
                }
            }
            WindowEvent::Focused(false) => self.drop_fly_keys(),
            WindowEvent::ScaleFactorChanged { .. } => self.request_redraw(),
            WindowEvent::Resized(size) => {
                if let Some(gpu) = &mut self.gpu {
                    gpu.resize(size.width, size.height);
                }
                self.request_redraw();
            }
            WindowEvent::RedrawRequested => {
                // An export renders a slice of its frames first, then the
                // editor draws itself around the progress.
                self.export_tick();
                self.redraw();
                // Playback drives continuous redraw only while playing —
                // on either clock, the audio stream's or the silent one —
                // and so do held fly keys and an export with frames left.
                if self.playing() || self.flying() || self.exporting() {
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
