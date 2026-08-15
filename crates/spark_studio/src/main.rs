use std::sync::Arc;
use std::time::Instant;

use spark_render::{CANVAS_H, CANVAS_W, Gpu, Shape, ShapePass, wgpu};
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{Key, NamedKey};
use winit::window::{Window, WindowId};

struct Studio {
    window: Option<Arc<Window>>,
    gpu: Option<Gpu>,
    shapes: Option<ShapePass>,
    start: Instant,
}

impl Studio {
    fn new() -> Self {
        Self {
            window: None,
            gpu: None,
            shapes: None,
            start: Instant::now(),
        }
    }

    fn redraw(&mut self) {
        let t = self.start.elapsed().as_secs_f32();
        let (Some(gpu), Some(shapes)) = (&mut self.gpu, &mut self.shapes) else {
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
        shapes.draw(
            &gpu.device,
            &gpu.queue,
            &mut encoder,
            &frame.view,
            &demo_scene(t),
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
        self.shapes = Some(ShapePass::new(&gpu.device, gpu.surface_format()));
        self.gpu = Some(gpu);
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
                self.redraw();
                if let Some(window) = &self.window {
                    window.request_redraw();
                }
            }
            _ => {}
        }
    }
}

/// Placeholder scene, every parameter a function of `t` — soon these shapes
/// come from layers you drew, and `t` comes from the timeline.
fn demo_scene(t: f32) -> Vec<Shape> {
    let cx = CANVAS_W * 0.5;
    let cy = CANVAS_H * 0.5;
    let mut shapes = Vec::new();

    // Ambient core glow.
    shapes.push(
        Shape::circle([cx, cy], 30.0)
            .color(0.45, 0.10, 1.00)
            .intensity(0.5)
            .glow(260.0),
    );

    // Breathing triangle.
    shapes.push(
        Shape::ngon([cx, cy], 170.0 + 26.0 * (t * 2.2).sin(), 3)
            .stroke(6.0)
            .glow(46.0)
            .color(1.00, 0.16, 0.85)
            .intensity(1.7)
            .rot(t * 0.6),
    );

    // Counter-rotating pentagon frame.
    shapes.push(
        Shape::ngon([cx, cy], 340.0, 5)
            .stroke(4.0)
            .glow(34.0)
            .color(0.16, 0.75, 1.00)
            .intensity(1.3)
            .rot(-t * 0.25),
    );

    // Ring of orbiting orbs.
    for i in 0..12 {
        let angle = t * 0.5 + i as f32 * std::f32::consts::TAU / 12.0;
        let radius = 450.0 + 14.0 * (t * 1.7 + i as f32).sin();
        let pos = [cx + angle.cos() * radius, cy + angle.sin() * radius];
        let orb = if i % 2 == 0 {
            Shape::circle(pos, 9.0).color(1.00, 0.20, 0.90).intensity(1.4)
        } else {
            Shape::circle(pos, 9.0).color(0.20, 0.80, 1.00).intensity(1.2)
        };
        shapes.push(orb.glow(26.0));
    }

    // Sweeping beams from the bottom corners.
    let sweep = (t * 0.4).sin() * 320.0;
    shapes.push(
        Shape::line([0.0, CANVAS_H], [cx + sweep, cy], 3.0)
            .color(0.50, 0.20, 1.00)
            .intensity(0.8)
            .glow(22.0),
    );
    shapes.push(
        Shape::line([CANVAS_W, CANVAS_H], [cx - sweep, cy], 3.0)
            .color(0.20, 0.90, 1.00)
            .intensity(0.8)
            .glow(22.0),
    );

    shapes
}

fn main() {
    let event_loop = EventLoop::new().expect("create event loop");
    event_loop.set_control_flow(ControlFlow::Poll);
    let mut studio = Studio::new();
    event_loop.run_app(&mut studio).expect("run event loop");
}
