mod editor;

use std::sync::Arc;

use editor::Editor;
use spark_render::{Gpu, ShapePass, wgpu};
use winit::application::ApplicationHandler;
use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{Key, ModifiersState, NamedKey};
use winit::window::{Window, WindowId};

struct Studio {
    window: Option<Arc<Window>>,
    gpu: Option<Gpu>,
    pass: Option<ShapePass>,
    editor: Editor,
    modifiers: ModifiersState,
}

impl Studio {
    fn new() -> Self {
        Self {
            window: None,
            gpu: None,
            pass: None,
            editor: Editor::new(),
            modifiers: ModifiersState::empty(),
        }
    }

    fn redraw(&mut self) {
        let (Some(gpu), Some(pass)) = (&mut self.gpu, &mut self.pass) else {
            return;
        };
        let Some(frame) = gpu.begin_frame() else {
            return;
        };
        let mut encoder = gpu
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        let clear = wgpu::Color {
            r: 0.008,
            g: 0.004,
            b: 0.022,
            a: 1.0,
        };
        pass.draw(
            &gpu.device,
            &gpu.queue,
            &mut encoder,
            &frame.view,
            &self.editor.display_shapes(),
            gpu.size(),
            clear,
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
        self.pass = Some(ShapePass::new(&gpu.device, gpu.surface_format()));
        self.gpu = Some(gpu);
        self.window = Some(window);
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::ModifiersChanged(m) => self.modifiers = m.state(),
            WindowEvent::CursorMoved { position, .. } => {
                if let Some(gpu) = &self.gpu {
                    self.editor.set_cursor(position.x, position.y, gpu.size());
                }
            }
            WindowEvent::MouseInput {
                state,
                button: MouseButton::Left,
                ..
            } => match state {
                ElementState::Pressed => self.editor.mouse_down(),
                ElementState::Released => self.editor.mouse_up(),
            },
            WindowEvent::MouseWheel { delta, .. } => {
                let dy = match delta {
                    MouseScrollDelta::LineDelta(_, y) => y,
                    MouseScrollDelta::PixelDelta(p) => p.y as f32 / 40.0,
                };
                self.editor.wheel(dy, self.modifiers.shift_key());
            }
            WindowEvent::KeyboardInput { event, .. } if event.state.is_pressed() => {
                match &event.logical_key {
                    Key::Named(NamedKey::Escape) => self.editor.deselect(),
                    Key::Named(NamedKey::Delete) | Key::Named(NamedKey::Backspace) => {
                        self.editor.delete_selected()
                    }
                    Key::Character(c) => {
                        let ctrl = self.modifiers.control_key();
                        let key = c.to_lowercase();
                        if ctrl && key == "q" {
                            event_loop.exit();
                        } else {
                            self.editor.char_key(&key, ctrl);
                        }
                    }
                    _ => {}
                }
            }
            WindowEvent::Resized(size) => {
                if let Some(gpu) = &mut self.gpu {
                    gpu.resize(size.width, size.height);
                }
            }
            WindowEvent::RedrawRequested => {
                self.redraw();
                if let Some(window) = &self.window {
                    window.request_redraw();
                }
            }
            _ => {}
        }
    }
}

fn main() {
    println!(
        "\nSpark Studio — comp editor v0 (status prints here until in-app UI lands)\n\
         \n\
         Tools:  1 select/move   2 circle   3 box   4 polygon   5 line\n\
         Draw:   click-drag on the canvas\n\
         Edit:   drag move | scroll scale | Shift+scroll or Q/E rotate\n\
                 [ ] polygon sides | C color | T outline/fill\n\
                 A/Z glow +/- | W/S brightness +/- | X or Del delete\n\
         Comp:   Ctrl+S save comp.spark | Ctrl+O reload | Esc deselect | Ctrl+Q quit\n"
    );
    let event_loop = EventLoop::new().expect("create event loop");
    event_loop.set_control_flow(ControlFlow::Poll);
    let mut studio = Studio::new();
    event_loop.run_app(&mut studio).expect("run event loop");
}
