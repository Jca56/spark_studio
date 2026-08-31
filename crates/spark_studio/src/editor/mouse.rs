//! Direct manipulation on the canvas: the press/drag/release state machine
//! for drawing and moving, hit testing, and the scroll wheel. The cursor
//! arrives already in canvas units — see `space.rs`.
//!
//! Split from `editor` so the document model and the pointer state machine
//! stay separately readable.

use spark_render::Shape;

use super::Editor;
use crate::history::Tag;
use crate::props::{Tool, dist, draw_shape};
use crate::random::Roll;

/// An in-progress pointer gesture on the canvas.
pub(super) enum Drag {
    /// Drawing a new shape. With the dice armed the roll is made once, here,
    /// and dressed onto every rebuild of the shape as the drag sizes it.
    Draw { roll: Option<Roll> },
    Move {
        last: [f32; 2],
        /// The primary's *unsnapped* center, tracking the cursor's intent.
        /// Snapping quantizes this — never the already-snapped position,
        /// which would gridlock the drag.
        free: [f32; 2],
    },
}

impl Editor {
    /// The shape the current draw gesture describes from `press` to `to`,
    /// dressed in the gesture's roll when the dice are armed.
    pub(super) fn drawn(&self, to: [f32; 2], roll: Option<Roll>) -> Shape {
        let shape = draw_shape(self.tool, self.press, to, self.sides, self.color);
        match roll {
            Some(r) => r.apply(shape),
            None => shape,
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
            let roll = self.random.then(|| Roll::new(&mut self.rng));
            let shape = self.drawn(self.cursor, roll);
            let i = self.push_shape(shape);
            self.selection = vec![i];
            self.drag = Some(Drag::Draw { roll });
            true
        }
    }

    pub fn mouse_up(&mut self) -> bool {
        let mut dirty = false;
        if let Some(Drag::Draw { .. }) = self.drag {
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

    /// Whether there is anything under the cursor to grab.
    pub fn hit_at_cursor(&self) -> bool {
        self.pick(self.cursor).is_some()
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
            // A shape off the canvas plane is asked where the click lands
            // on *its* plane; one on the canvas gets the click as it is.
            let Some(q) = posed.unproject(&self.camera, p) else {
                continue;
            };
            let d = if posed.is_path() {
                self.path_pick(&posed, q)
            } else {
                posed.pick_distance(q)
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Identity canvas map: window px are canvas units.
    const MAP: crate::view::CanvasMap = (1.0, 0.0, 0.0);

    /// Press, drag, release one shape; what it looked like mid-drag and
    /// what it ended as.
    fn draw(e: &mut Editor, from: [f32; 2], to: [f32; 2]) -> (Shape, Shape) {
        e.set_cursor_canvas([(from[0] - MAP.1) / MAP.0, (from[1] - MAP.2) / MAP.0]);
        e.mouse_down(false);
        let mid = e.shapes[e.primary().unwrap()];
        e.set_cursor_canvas([(to[0] - MAP.1) / MAP.0, (to[1] - MAP.2) / MAP.0]);
        e.mouse_up();
        (mid, e.shapes[e.primary().unwrap()])
    }

    /// With the dice off, a drawn shape is the tool's colour and plain.
    #[test]
    fn unarmed_draws_the_current_color() {
        let mut e = Editor::empty();
        e.choose_tool(Tool::Circle);
        let (_, s) = draw(&mut e, [300.0, 300.0], [400.0, 300.0]);
        assert_eq!(s.rgb(), e.color());
        assert_eq!(s.glow_radius(), 0.0);
        assert!(!s.gradient());
    }

    /// Armed, each shape rolls its own look — and the roll is made at
    /// mouse-down and kept through the drag, so the shape you watched
    /// yourself size is the shape you let go of.
    #[test]
    fn armed_rolls_once_per_shape() {
        let mut e = Editor::empty();
        e.random = true;
        e.choose_tool(Tool::Circle);
        let (mid, a) = draw(&mut e, [300.0, 300.0], [400.0, 300.0]);
        assert_eq!(mid.rgb(), a.rgb(), "the roll changed mid-drag");
        assert_eq!(mid.glow_radius(), a.glow_radius());
        assert_eq!(mid.outline(), a.outline());
        assert_ne!(a.rgb(), e.color(), "rolled colour should not be the tool's");
        assert_eq!(
            e.color(),
            crate::props::PALETTE[0],
            "the tool colour is untouched"
        );

        let (_, b) = draw(&mut e, [600.0, 300.0], [700.0, 300.0]);
        assert_ne!(a.rgb(), b.rgb(), "two shapes rolled the same colour");
        // Geometry is the drag's, never the dice's.
        assert_eq!(b.center(), [600.0, 300.0]);
        assert!((b.size() - 100.0).abs() < 1e-3);
    }

    /// Disarming goes straight back to the tool's colour.
    #[test]
    fn disarming_restores_the_tool_color() {
        let mut e = Editor::empty();
        e.random = true;
        e.choose_tool(Tool::Box);
        draw(&mut e, [300.0, 300.0], [400.0, 360.0]);
        e.random = false;
        let (_, s) = draw(&mut e, [600.0, 300.0], [700.0, 360.0]);
        assert_eq!(s.rgb(), e.color());
    }
}
