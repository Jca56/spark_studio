//! The editor's place in space: the camera it picks through, and the
//! selection's moves off the canvas plane.
//!
//! The editor works in canvas units — the cursor is where the mouse's
//! ray meets the canvas plane, whatever camera the viewport looks
//! through — and asks each shape where that ray lands on *its* plane. So
//! the camera has to be known here, and the studio keeps it current.

use spark_render::Camera;

use super::Editor;
use super::mouse::Drag;
use crate::history::Tag;

impl Editor {
    /// The camera the viewport is looking through this frame.
    pub fn set_camera(&mut self, camera: Camera) {
        self.camera = camera;
    }

    /// The cursor, already in canvas units — where the mouse's ray meets
    /// the canvas plane — driving any drag in progress.
    pub fn set_cursor_canvas(&mut self, now: [f32; 2]) -> bool {
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
        match self.drag {
            Some(Drag::Draw { roll }) => {
                if let Some(&i) = self.selection.last() {
                    self.shapes[i] = self.drawn(now, roll);
                }
                true
            }
            _ => false,
        }
    }

    /// Slide the selection along the canvas plane.
    pub fn move_selection_by(&mut self, d: [f32; 2]) -> bool {
        if self.selection.is_empty() || !d[0].is_finite() || !d[1].is_finite() {
            return false;
        }
        self.record(Tag::Handle);
        for i in self.selection.clone() {
            self.shapes[i].translate(d);
        }
        self.mark_posed_selection();
        true
    }

    /// Move the selection toward (positive) or away from the camera.
    pub fn shift_selection_z(&mut self, dz: f32) -> bool {
        self.change_space(dz, |s, v| s.set_z(s.z() + v))
    }

    pub fn tilt_selection(&mut self, d: f32) -> bool {
        self.change_space(d, |s, v| s.set_tilt(s.tilt() + v))
    }

    pub fn turn_selection(&mut self, d: f32) -> bool {
        self.change_space(d, |s, v| s.set_turn(s.turn() + v))
    }

    /// Spin: the in-plane rotation, each shape about its own centre.
    pub fn spin_selection(&mut self, d: f32) -> bool {
        self.rotate_selection(d, None)
    }

    fn change_space(&mut self, v: f32, f: impl Fn(&mut spark_render::Shape, f32)) -> bool {
        if self.selection.is_empty() || !v.is_finite() || v == 0.0 {
            return false;
        }
        self.record(Tag::Handle);
        for i in self.selection.clone() {
            f(&mut self.shapes[i], v);
        }
        self.mark_posed_selection();
        true
    }
}

impl Editor {
    /// Drag one end of the primary line to `cur` (the end handle's drag,
    /// `k` 0 for its start, 1 for its end); the other end holds. The
    /// pivot a laser swings on is an end, not the middle (Alva,
    /// 2026-09-01). A pose edit like a scrub: previewed on a keyed clip
    /// until `K` stamps it.
    pub fn drag_line_end(&mut self, k: usize, cur: [f32; 2]) -> bool {
        let Some(i) = self.primary() else {
            return false;
        };
        if !self.shapes[i].is_line() || k > 1 {
            return false;
        }
        self.record(crate::history::Tag::Handle);
        if k == 0 {
            self.shapes[i].set_line_start(cur);
        } else {
            self.shapes[i].set_line_end(cur);
        }
        self.mark_posed(&[i]);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use spark_render::Shape;

    #[test]
    fn space_moves_apply_to_the_whole_selection_and_undo() {
        let mut e = Editor::empty();
        let a = e.push_shape(Shape::circle([100.0, 100.0], 10.0));
        let b = e.push_shape(Shape::rect([300.0, 300.0], [20.0, 20.0]));
        e.select(Some(a));
        e.toggle_select(b);
        assert!(e.shift_selection_z(150.0));
        assert!(e.tilt_selection(0.5));
        assert!(e.turn_selection(-0.25));
        assert!(e.move_selection_by([10.0, -5.0]));
        for i in [a, b] {
            let s = e.shapes()[i];
            assert_eq!((s.z(), s.tilt(), s.turn()), (150.0, 0.5, -0.25));
        }
        assert_eq!(e.shapes()[a].center(), [110.0, 95.0]);
        // A zero change is nothing to do; a NaN is refused.
        assert!(!e.shift_selection_z(0.0));
        assert!(!e.turn_selection(f32::NAN));
        e.end_gesture();
        assert!(e.undo());
        assert!(e.shapes()[a].on_plane());
    }

    #[test]
    fn the_cursor_in_canvas_units_drives_a_move() {
        let mut e = Editor::empty();
        let i = e.push_shape(Shape::circle([100.0, 100.0], 10.0));
        e.select(Some(i));
        e.set_cursor_canvas([100.0, 100.0]);
        // Already selected: the selection doesn't change, the drag starts.
        e.mouse_down(false);
        assert!(e.set_cursor_canvas([160.0, 130.0]));
        assert_eq!(e.shapes()[i].center(), [160.0, 130.0]);
        e.mouse_up();
    }
}
