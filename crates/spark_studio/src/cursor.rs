//! The Spark cursors: baked to raw RGBA (straight alpha) from the assets
//! SVGs via rsvg-convert + ffmpeg — no image decoding at runtime. Two
//! sizes each; the output scale picks one at startup. Split from main so
//! the event plumbing stays readable.

use winit::event_loop::ActiveEventLoop;
use winit::window::Window;

use crate::Studio;

const CURSOR_80: &[u8] = include_bytes!("../assets/spark_cursor_80.rgba");
const CURSOR_120: &[u8] = include_bytes!("../assets/spark_cursor_120.rgba");
const CURSOR2_80: &[u8] = include_bytes!("../assets/spark_cursor2_80.rgba");
const CURSOR2_120: &[u8] = include_bytes!("../assets/spark_cursor2_120.rgba");

impl Studio {
    /// Upload both cursors sized by the output scale, hotspot on each
    /// arrow's tip.
    pub(crate) fn make_cursors(&mut self, event_loop: &ActiveEventLoop, window: &Window) {
        let big = window.scale_factor() >= 1.2;
        let sets = [
            (CURSOR_80, CURSOR_120, (6u16, 4u16), (9u16, 6u16)),
            (CURSOR2_80, CURSOR2_120, (1, 1), (2, 2)),
        ];
        for (i, (b80, b120, h80, h120)) in sets.into_iter().enumerate() {
            let (bytes, side, (hx, hy)) = if big {
                (b120, 120u16, h120)
            } else {
                (b80, 80u16, h80)
            };
            if let Ok(src) =
                winit::window::CustomCursor::from_rgba(bytes.to_vec(), side, side, hx, hy)
            {
                self.custom_cursors[i] = Some(event_loop.create_custom_cursor(src));
            }
        }
    }

    /// The cursor the window rests on: the chosen Spark cursor (if the
    /// compositor took it), the system arrow otherwise. Transient cursors
    /// (row-resize) override it in `drag`.
    pub(crate) fn base_cursor(&self) -> winit::window::Cursor {
        match self
            .cursor_choice
            .and_then(|i| self.custom_cursors[i].clone())
        {
            Some(c) => c.into(),
            None => winit::window::CursorIcon::Default.into(),
        }
    }

    pub(crate) fn apply_cursor(&self) {
        if let Some(w) = &self.window {
            w.set_cursor(self.base_cursor());
        }
    }
}
