//! Mouse release. Split from `input` so the press path stays readable.

use spark_ui::TitleAction;
use winit::event_loop::ActiveEventLoop;

use crate::Studio;

impl Studio {
    pub(crate) fn release(&mut self, event_loop: &ActiveEventLoop) {
        let (cx, cy) = (self.cursor_px.0 as f32, self.cursor_px.1 as f32);
        if self.context_release() {
            // A slider let go; the menu stays up.
            self.request_redraw();
            return;
        }
        // An effect row let go: dropped on an object, it's added there.
        if self.left_release(cx, cy) {
            self.request_redraw();
            return;
        }
        if self.look_release() {
            // A look in the fly view, or the click on empty space that
            // dropped the selection: either way the release is spent.
            self.request_redraw();
            return;
        }
        if self.gizmo_drag.take().is_some() {
            self.request_redraw();
        }
        // An inspector drag ends here too — a scrub that never travelled
        // opens its field for typing — and falls through to end the
        // gesture like any other.
        if let Some(l) = self.layout()
            && self.inspector_release(l.right)
        {
            self.request_redraw();
        }
        self.editor.end_gesture();
        self.handle_drag = None;
        self.timeline_scrub = false;
        self.clip_drag = None;
        self.loop_drag = None;
        self.panel_resize = false;
        if let Some(pressed) = self.title_pressed.take() {
            let hit = self.title_bar().and_then(|tb| tb.hit(cx, cy));
            if hit == Some(pressed)
                && let Some(window) = &self.window
            {
                match pressed {
                    TitleAction::Minimize => window.set_minimized(true),
                    TitleAction::Maximize => window.set_maximized(!window.is_maximized()),
                    TitleAction::Close => {
                        if self.confirm_discard(crate::project::Discard::Quit) {
                            event_loop.exit();
                        }
                    }
                }
            }
            self.request_redraw();
        } else if self.editor.mouse_up() {
            self.request_redraw();
        }
    }
}
