//! Direct manipulation on the canvas: the cursor's journey from window
//! pixels to canvas units, the press/drag/release state machine for drawing
//! and moving, hit testing, and the scroll wheel.
//!
//! Split from `editor` so the document model and the pointer state machine
//! stay separately readable.

use super::Editor;
use crate::history::Tag;
use crate::props::{Tool, dist, draw_shape};

/// An in-progress pointer gesture on the canvas.
pub(super) enum Drag {
    Draw,
    Move {
        last: [f32; 2],
        /// The primary's *unsnapped* center, tracking the cursor's intent.
        /// Snapping quantizes this — never the already-snapped position,
        /// which would gridlock the drag.
        free: [f32; 2],
    },
}

impl Editor {
    /// Window-space cursor (physical px) -> canvas units through the
    /// canvas view's mapping, then drive any active drag.
    pub fn set_cursor(&mut self, px: f64, py: f64, map: crate::view::CanvasMap) -> bool {
        let (scale, ox, oy) = map;
        let now = [(px as f32 - ox) / scale, (py as f32 - oy) / scale];
        self.cursor = now;
        let free_target = if let Some(Drag::Move { last, free }) = &mut self.drag {
            let d = [now[0] - last[0], now[1] - last[1]];
            *last = now;
            free[0] += d[0];
            free[1] += d[1];
            Some(*free)
        } else {
            None
        };
        if let Some(free) = free_target {
            self.move_selection_to(free);
            return true;
        }
        match &mut self.drag {
            Some(Drag::Draw) => {
                if let Some(&i) = self.selection.last() {
                    self.shapes[i] = draw_shape(self.tool, self.press, now, self.sides, self.color);
                }
                true
            }
            _ => false,
        }
    }

    /// Ctrl+click toggles membership in the selection; a plain click on an
    /// already-selected shape keeps the set (so groups drag together).
    pub fn mouse_down(&mut self, ctrl: bool) -> bool {
        if self.tool == Tool::Select {
            let hit = self.pick(self.cursor);
            let old = self.selection.clone();
            match hit {
                Some(i) if ctrl => {
                    self.history.commit();
                    self.toggle_index(i);
                }
                Some(i) => {
                    if !self.selection.contains(&i) {
                        self.selection = vec![i];
                        self.expand_groups();
                    }
                    // Pre-move state; dropped again at mouse_up if nothing
                    // moved.
                    let s = self.snap();
                    self.history.push(s);
                    let free = self.shapes[self.primary().unwrap_or(i)].center();
                    self.drag = Some(Drag::Move {
                        last: self.cursor,
                        free,
                    });
                }
                None if !ctrl => self.selection.clear(),
                None => {}
            }
            old != self.selection
        } else {
            self.press = self.cursor;
            let s = self.snap();
            self.history.push(s);
            let shape = draw_shape(self.tool, self.press, self.cursor, self.sides, self.color);
            let i = self.push_shape(shape);
            self.selection = vec![i];
            self.drag = Some(Drag::Draw);
            true
        }
    }

    pub fn mouse_up(&mut self) -> bool {
        let mut dirty = false;
        if let Some(Drag::Draw) = self.drag {
            // A click with no drag leaves an accidental speck — discard it.
            if dist(self.press, self.cursor) < 3.0
                && let Some(&i) = self.selection.last()
            {
                self.remove_shape(i);
                self.selection.clear();
                dirty = true;
            }
        }
        if self.drag.take().is_some() {
            // Discarded specks and moves that never moved undo to nothing —
            // drop the snapshot the gesture pushed.
            let s = self.snap();
            self.history.drop_noop(&s);
        }
        self.guides.clear();
        self.history.commit();
        dirty
    }

    /// Topmost unhidden shape within grabbing distance of `p`, in canvas
    /// units. Walks the stack from the front, so what looks in front is what
    /// you get.
    pub(super) fn pick(&self, p: [f32; 2]) -> Option<usize> {
        for (i, s) in self.shapes.iter().enumerate().rev() {
            if self.shape_hidden(i) {
                continue;
            }
            let posed = self.posed_shape(i, *s);
            let d = if posed.is_path() {
                self.path_pick(&posed, p)
            } else {
                posed.pick_distance(p)
            };
            if d <= 14.0 {
                return Some(i);
            }
        }
        None
    }

    pub fn wheel(&mut self, dy: f32, rotate: bool) -> bool {
        if self.selection.is_empty() {
            return false;
        }
        self.record(Tag::Wheel);
        let factor = (1.0 + dy * 0.08).clamp(0.5, 2.0);
        let rot = dy * 0.06;
        for i in self.selection.clone() {
            if rotate {
                self.shapes[i].rotate_by(rot);
            } else {
                self.scale_index(i, factor);
            }
        }
        self.mark_posed_selection();
        true
    }
}
