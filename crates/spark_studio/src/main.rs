mod editor;

use std::sync::Arc;

use editor::Editor;
use spark_render::{Gpu, ShapePass, wgpu};
use spark_ui::{Layout, UiPass};
use winit::application::ApplicationHandler;
use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{Key, ModifiersState, NamedKey};
use winit::window::{Window, WindowId};

struct Studio {
    window: Option<Arc<Window>>,
    gpu: Option<Gpu>,
    shape_pass: Option<ShapePass>,
    ui_pass: Option<UiPass>,
    editor: Editor,
    modifiers: ModifiersState,
    cursor_px: (f64, f64),
}

impl Studio {
    fn new() -> Self {
        Self {
            window: None,
            gpu: None,
            shape_pass: None,
            ui_pass: None,
            editor: Editor::new(),
            modifiers: ModifiersState::empty(),
            cursor_px: (0.0, 0.0),
        }
    }

    fn layout(&self) -> Option<Layout> {
        let gpu = self.gpu.as_ref()?;
        let scale = self.window.as_ref()?.scale_factor() as f32;
        let (w, h) = gpu.size();
        Some(Layout::compute(w, h, scale))
    }

    fn request_redraw(&self) {
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }

    fn redraw(&mut self) {
        let Some(layout) = self.layout() else { return };
        let scale = self
            .window
            .as_ref()
            .map(|w| w.scale_factor() as f32)
            .unwrap_or(1.0);
        let (Some(gpu), Some(shape_pass), Some(ui_pass)) =
            (&mut self.gpu, &mut self.shape_pass, &mut self.ui_pass)
        else {
            return;
        };
        let Some(frame) = gpu.begin_frame() else { return };
        let mut encoder = gpu
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        let clear = wgpu::Color {
            r: 0.008,
            g: 0.004,
            b: 0.022,
            a: 1.0,
        };
        shape_pass.draw(
            &gpu.device,
            &gpu.queue,
            &mut encoder,
            &frame.view,
            &self.editor.display_shapes(),
            gpu.size(),
            layout.viewport,
            clear,
        );
        ui_pass.draw(
            &gpu.device,
            &gpu.queue,
            &mut encoder,
            &frame.view,
            &layout.panel_rects(scale),
            gpu.size(),
        );
        gpu.queue.submit([encoder.finish()]);
        frame.present();
    }
}

impl ApplicationHandler for Studio {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let attrs = Window::default_attributes().with_title("Spark Studio");
        let window = Arc::new(event_loop.create_window(attrs).expect("create window"));
        let size = window.inner_size();
        let gpu = Gpu::new(window.clone(), size.width, size.height);
        self.shape_pass = Some(ShapePass::new(&gpu.device, gpu.surface_format()));
        self.ui_pass = Some(UiPass::new(&gpu.device, gpu.surface_format()));
        self.gpu = Some(gpu);
        self.window = Some(window);
        self.request_redraw();
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::ModifiersChanged(m) => self.modifiers = m.state(),
            WindowEvent::CursorMoved { position, .. } => {
                self.cursor_px = (position.x, position.y);
                if let Some(layout) = self.layout() {
                    if self
                        .editor
                        .set_cursor(position.x, position.y, layout.viewport)
                    {
                        self.request_redraw();
                    }
                }
            }
            WindowEvent::MouseInput {
                state,
                button: MouseButton::Left,
                ..
            } => {
                let dirty = match state {
                    ElementState::Pressed => {
                        let in_viewport = self.layout().is_some_and(|l| {
                            l.viewport
                                .contains(self.cursor_px.0 as f32, self.cursor_px.1 as f32)
                        });
                        in_viewport && self.editor.mouse_down()
                    }
                    ElementState::Released => self.editor.mouse_up(),
                };
                if dirty {
                    self.request_redraw();
                }
            }
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
                    Key::Named(NamedKey::Escape) => self.editor.deselect(),
                    Key::Named(NamedKey::Delete) | Key::Named(NamedKey::Backspace) => {
                        self.editor.delete_selected()
                    }
                    Key::Character(c) => {
                        let ctrl = self.modifiers.control_key();
                        let key = c.to_lowercase();
                        if ctrl && key == "q" {
                            event_loop.exit();
                            false
                        } else {
                            self.editor.char_key(&key, ctrl)
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
            WindowEvent::RedrawRequested => self.redraw(),
            _ => {}
        }
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
         Comp:   Ctrl+S save comp.spark | Ctrl+O reload | Esc deselect | Ctrl+Q quit\n"
    );
    let event_loop = EventLoop::new().expect("create event loop");
    event_loop.set_control_flow(ControlFlow::Wait);
    let mut studio = Studio::new();
    event_loop.run_app(&mut studio).expect("run event loop");
}
