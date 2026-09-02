//! The camera in the editor: adding one, and what it does to the frame's
//! camera — the shake.

use spark_render::Shape;

use super::Editor;

impl Editor {
    /// Add > Camera: a camera object, named and selected, born with a
    /// clip like anything else. It has no place on the canvas yet — the
    /// render camera stays where it is — so the outliner is where it is
    /// picked; what it carries is the shake. Undoable.
    pub fn add_camera(&mut self) -> usize {
        let s = self.snap();
        self.history.push(s);
        let [cw, ch] = self.canvas;
        let i = self.push_shape(Shape::camera([cw * 0.5, ch * 0.5]));
        self.names[i] = "camera".to_string();
        self.select(Some(i));
        self.clear_posed();
        i
    }

    /// How far the render camera is jolted at the playhead, canvas units
    /// right and down: every camera object whose clip is playing, posed
    /// — keyed, reacting to `levels` — shaking on its own clip's clock.
    /// None playing, or all hidden, is no shake at all.
    pub fn shake(&self, levels: Option<crate::fx::Levels>) -> [f32; 2] {
        let mut out = [0.0f32; 2];
        for (i, s) in self.shapes.iter().enumerate() {
            if !s.is_camera() || self.shape_hidden(i) || !self.exists_now(i) {
                continue;
            }
            let posed = self.posed_with(i, *s, levels);
            let (Some(amount), Some(rate)) = (posed.shake(), posed.shake_rate()) else {
                continue;
            };
            let [dx, dy] = crate::shake::offset(amount, rate, self.clock_of(i));
            out[0] += dx;
            out[1] += dy;
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adding_a_camera_names_and_selects_it() {
        let mut e = Editor::empty();
        let i = e.add_camera();
        assert!(e.shapes()[i].is_camera());
        assert_eq!(e.name(i), "camera");
        assert_eq!(e.selection(), &[i]);
        assert!(e.exists_now(i), "born with a clip under the playhead");
    }

    /// The shake is there while the camera's clip plays and not before
    /// or after, moves from moment to moment, and a hidden camera is a
    /// still one.
    #[test]
    fn the_camera_shakes_while_its_clip_plays() {
        let mut e = Editor::empty();
        assert_eq!(e.shake(None), [0.0; 2], "no camera, no shake");
        let i = e.add_camera();
        e.set_time(0.25);
        let a = e.shake(None);
        assert_ne!(a, [0.0; 2]);
        e.set_time(0.3);
        let b = e.shake(None);
        assert_ne!(a, b, "a rumble moves");
        e.set_time(0.25);
        assert_eq!(e.shake(None), a, "the same moment shakes the same way");
        // Turned all the way down (the camera is the selection), it is
        // still.
        assert!(e.set_prop(crate::props::Prop::Shake, 0.0));
        assert_eq!(e.shake(None), [0.0; 2]);
        assert!(e.set_prop(crate::props::Prop::Shake, 16.0));
        assert_ne!(e.shake(None), [0.0; 2]);
        // Past its clip: nothing.
        e.set_time(1e4);
        assert_eq!(e.shake(None), [0.0; 2]);
        // Hidden: nothing.
        e.set_time(0.25);
        assert!(e.toggle_hidden(i));
        assert_eq!(e.shake(None), [0.0; 2]);
    }
}
