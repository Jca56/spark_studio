//! Lights in the editor: adding one, switching its kind.

use spark_render::{LightKind, Shape};

use super::Editor;

impl Editor {
    /// Add > Sun / Point Light / Spot Light: a new light, named and
    /// selected. A sun lands in the upper left, aimed exactly as the
    /// default sun every comp starts under; a point or a spot lands at
    /// the centre, a little in front of the canvas, so it lights what is
    /// already there. Undoable.
    pub fn add_light(&mut self, kind: LightKind) -> usize {
        let s = self.snap();
        self.history.push(s);
        let [cw, ch] = self.canvas;
        let shape = match kind {
            LightKind::Sun => Shape::sun([cw * 0.25, ch * 0.25]),
            // Everywhere at once has no place; its card is what matters,
            // so its mark sits out of the way, upper right.
            LightKind::Ambient => Shape::light([cw * 0.75, ch * 0.25], kind),
            _ => {
                let mut l = Shape::light([cw * 0.5, ch * 0.5], kind);
                l.set_z(400.0);
                l
            }
        };
        let i = self.push_shape(shape);
        self.names[i] = match kind {
            LightKind::Sun => "sun",
            LightKind::Point => "point light",
            LightKind::Spot => "spot light",
            LightKind::Ambient => "ambient",
        }
        .to_string();
        self.select(Some(i));
        self.clear_posed();
        i
    }

    /// The card's kind picker: make every selected light this kind.
    #[allow(dead_code)] // kept for the redesign; the old panels were the only caller
    pub fn set_light_kind(&mut self, index: usize) -> bool {
        let Some(i) = self.primary() else {
            return false;
        };
        let kind = LightKind::from_index(index);
        if self.shapes[i].light_kind().is_none_or(|k| k == kind) {
            return false;
        }
        let s = self.snap();
        self.history.push(s);
        for &j in &self.selection {
            self.shapes[j].set_light_kind(kind);
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adding_a_light_names_and_selects_it() {
        let mut e = Editor::empty();
        let i = e.add_light(LightKind::Spot);
        assert_eq!(e.shapes()[i].light_kind(), Some(LightKind::Spot));
        assert_eq!(e.name(i), "spot light");
        assert_eq!(e.selection(), &[i]);
        // In front of the canvas, so it lights what's on it.
        assert!(e.shapes()[i].z() > 0.0);
        let s = e.add_light(LightKind::Sun);
        assert_eq!(e.name(s), "sun");
        assert!(e.shapes()[s].as_light().unwrap().direction.z < 0.0);
    }

    #[test]
    fn the_kind_picker_switches_every_selected_light() {
        let mut e = Editor::empty();
        let a = e.add_light(LightKind::Point);
        let b = e.add_light(LightKind::Point);
        e.select(Some(a));
        e.toggle_select(b);
        assert!(e.set_light_kind(2));
        assert_eq!(e.shapes()[a].light_kind(), Some(LightKind::Spot));
        assert_eq!(e.shapes()[b].light_kind(), Some(LightKind::Spot));
        // Already that kind: nothing to do, nothing to undo.
        assert!(!e.set_light_kind(2));
        // Not a light: nothing.
        e.push_shape(Shape::circle([0.0; 2], 5.0));
        let c = e.shapes().len() - 1;
        e.select(Some(c));
        assert!(!e.set_light_kind(0));
    }
}
