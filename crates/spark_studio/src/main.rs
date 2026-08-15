use std::sync::Arc;
use std::time::Instant;

use spark_render::{Gpu, wgpu};
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{Key, NamedKey};
use winit::window::{Window, WindowId};

struct Studio {
    window: Option<Arc<Window>>,
    gpu: Option<Gpu>,
    start: Instant,
}

impl Studio {
    fn new() -> Self {
        Self {
            window: None,
            gpu: None,
            start: Instant::now(),
        }
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
        self.gpu = Some(Gpu::new(window.clone(), size.width, size.height));
        self.window = Some(window);
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::KeyboardInput { event, .. }
                if event.logical_key == Key::Named(NamedKey::Escape) =>
            {
                event_loop.exit()
            }
            WindowEvent::Resized(size) => {
                if let Some(gpu) = &mut self.gpu {
                    gpu.resize(size.width, size.height);
                }
            }
            WindowEvent::RedrawRequested => {
                let t = self.start.elapsed().as_secs_f64();
                if let Some(gpu) = &mut self.gpu {
                    gpu.render_clear(pulse_color(t));
                }
                if let Some(window) = &self.window {
                    window.request_redraw();
                }
            }
            _ => {}
        }
    }
}

/// The first Spark Studio frame function: a deep synthwave glow breathing
/// with time. Everything is a function of `t`, starting now.
fn pulse_color(t: f64) -> wgpu::Color {
    let breathe = 0.5 + 0.5 * (t * 0.8).sin();
    wgpu::Color {
        r: 0.03 + 0.05 * breathe,
        g: 0.0,
        b: 0.08 + 0.10 * (1.0 - breathe),
        a: 1.0,
    }
}

fn main() {
    let event_loop = EventLoop::new().expect("create event loop");
    event_loop.set_control_flow(ControlFlow::Poll);
    let mut studio = Studio::new();
    event_loop.run_app(&mut studio).expect("run event loop");
}
