//! Wheel routing. The wheel acts on whatever it is over, so this is one
//! place that has to know about every scrollable region at once — which
//! is exactly why it does not belong in the event loop.

use winit::event::MouseScrollDelta;

use crate::{Studio, timeline};

impl Studio {
    pub(crate) fn wheel(&mut self, delta: MouseScrollDelta) {
        if self.export.is_some() {
            return;
        }
        let dy = match delta {
            MouseScrollDelta::LineDelta(_, y) => y,
            MouseScrollDelta::PixelDelta(p) => p.y as f32 / 40.0,
        };
        let (cx, cy) = (self.cursor_px.0 as f32, self.cursor_px.1 as f32);
        let Some(layout) = self.layout() else { return };
        // The wheel acts on whatever it's over: the view in the
        // viewport, scrolling in the side panels.
        if layout.viewport.contains(cx, cy) {
            if self.fly_wheel(dy) {
                // In the fly view the wheel is the throttle: forward and
                // back along the look, whatever modifier is held.
                self.request_redraw();
            } else if self.modifiers.control_key() {
                // Ctrl+wheel zooms the canvas at the cursor — the
                // timeline recipe, applied to the stage. A plain wheel
                // over the canvas does nothing: it used to scale the
                // selection, which was never what a stray notch meant
                // (gone at Alva's request, 2026-08-31).
                let factor = 1.18f32.powf(dy);
                self.canvas_view
                    .zoom_at(factor, cx, cy, layout.viewport, self.editor.canvas());
                self.request_redraw();
            }
        } else if layout.timeline.contains(cx, cy) {
            // Zoom and pan ride the comp's clock, not the track's — a silent
            // comp has a timeline and it has to be navigable.
            let duration = self.duration();
            let panel = timeline::panel(layout.timeline, self.scale());
            if self.modifiers.control_key() {
                // Zoom around the time under the cursor.
                let pivot = self.time_view.t_at(cx, panel.axis);
                let factor = (1.0f32 / 1.18).powf(dy);
                self.time_view.zoom(factor, pivot, duration);
            } else if self.modifiers.shift_key() {
                let dt = -dy * self.time_view.span() * 0.10;
                self.time_view.pan(dt, duration);
            } else {
                self.lanes_scroll = (self.lanes_scroll - dy * 60.0 * self.scale()).max(0.0);
            }
            self.request_redraw();
        }
    }
}
