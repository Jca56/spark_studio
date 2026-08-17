//! Wheel routing. The wheel acts on whatever it is over, so this is one
//! place that has to know about every scrollable region at once — which
//! is exactly why it does not belong in the event loop.

use winit::event::MouseScrollDelta;

use crate::{Studio, colorhome, timeline};

impl Studio {
    pub(crate) fn wheel(&mut self, delta: MouseScrollDelta) {
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
        } else if self.materials_open && layout.left.contains(cx, cy) {
            // The playground is taller than the panel; the wheel
            // scrolls it. Clamping happens at layout time.
            self.materials_scroll = (self.materials_scroll - dy * 60.0 * self.scale()).max(0.0);
            self.request_redraw();
        } else if layout.right.contains(cx, cy) {
            // Only the cards list scrolls; the color home is pinned.
            let (_, cards_vp) =
                colorhome::split(layout.right, self.scale(), self.picker_hsv.is_some());
            if cards_vp.contains(cx, cy) {
                self.layers_scroll = (self.layers_scroll - dy * 60.0 * self.scale()).max(0.0);
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
}
