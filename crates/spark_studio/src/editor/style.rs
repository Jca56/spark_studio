//! Writing values onto shapes: the absolute-value property setters behind
//! the cards' sliders and scrub fields, plus the look toggles (palette,
//! fill/outline, blend, side count). Split from `editor` so the interaction
//! state machine stays readable.

use crate::history::Tag;
use crate::props::{PALETTE, PALETTE_NAMES, Prop, Props};

use super::Editor;

impl Editor {
    pub fn selected_props(&self) -> Option<Props> {
        let i = self.primary()?;
        let s = &self.shapes[i];
        let c = s.center();
        let rgb = s.rgb();
        Some(Props {
            x: c[0],
            y: c[1],
            rotation: s.rotation(),
            size: s.size(),
            rgb,
            grad: s.gradient(),
            rgb2: s.rgb2(),
        })
    }

    /// Absolute-value sliders write to the primary shape.
    pub fn set_prop(&mut self, prop: Prop, value: f32) -> bool {
        let Some(i) = self.primary() else {
            return false;
        };
        self.record(Tag::Prop(prop));
        if prop == Prop::Scale {
            let cur = self.shapes[i].size();
            if cur > 0.001 {
                self.scale_index(i, value / cur);
            }
            self.mark_posed(&[i]);
            return true;
        }
        // React amounts live editor-side, per shape — set and done.
        if let Some(slot) = match prop {
            Prop::ReactScale => Some(0),
            Prop::ReactGlow => Some(1),
            Prop::ReactBright => Some(2),
            _ => None,
        } {
            for &j in &self.selection.clone() {
                self.react[j][slot] = value.clamp(0.0, 2.0);
            }
            return true;
        }
        let s = &mut self.shapes[i];
        match prop {
            Prop::X => {
                let c = s.center();
                s.set_center([value, c[1]]);
            }
            Prop::Y => {
                let c = s.center();
                s.set_center([c[0], value]);
            }
            Prop::Rotation => s.set_rotation(value),
            Prop::Scale => unreachable!("handled above"),
            Prop::Width => s.set_box_width(value),
            Prop::Height => s.set_box_height(value),
            Prop::Glow => s.set_glow(value),
            Prop::Brightness => s.set_brightness(value),
            Prop::Sides => s.set_sides(value.round() as u32),
            Prop::Thickness => s.set_thickness(value),
            Prop::ReactScale | Prop::ReactGlow | Prop::ReactBright => {
                unreachable!("handled above")
            }
        }
        self.mark_posed(&[i]);
        true
    }

    /// Pick a palette swatch: it becomes the current color, and paints the
    /// selection if there is one. With `to_b`, gradient-enabled shapes take
    /// it as the gradient's end color instead.
    ///
    /// Always reports `true`: the current color moved even when no shape did,
    /// and the swatch ring has to follow it.
    pub fn set_color_index(&mut self, i: usize, to_b: bool) -> bool {
        self.set_current_color(PALETTE[i % PALETTE.len()], to_b);
        true
    }

    /// The one road every color edit takes: set the current color, then paint
    /// it onto whatever is selected. Nothing else may write `self.color`, so
    /// the color home and the picker can never drift apart.
    pub fn set_current_color(&mut self, rgb: [f32; 3], to_b: bool) -> bool {
        self.color = rgb;
        if self.selection.is_empty() {
            return false;
        }
        self.record(Tag::Color);
        self.with_selected(|s| {
            if to_b && s.gradient() {
                s.set_rgb2(rgb);
            } else {
                s.set_rgb(rgb);
            }
        })
    }

    pub fn set_outline(&mut self, on: bool) -> bool {
        let Some(i) = self.primary() else {
            return false;
        };
        // `None` (a line) and already-matching both mean nothing to do.
        if self.shapes[i].outline() != Some(!on) {
            return false;
        }
        let s = self.snap();
        self.history.push(s);
        for &j in &self.selection {
            self.shapes[j].set_outline(on);
        }
        true
    }

    pub fn set_additive(&mut self, on: bool) -> bool {
        let Some(i) = self.primary() else {
            return false;
        };
        if self.shapes[i].additive() == on {
            return false;
        }
        let s = self.snap();
        self.history.push(s);
        for &j in &self.selection {
            self.shapes[j].set_additive(on);
        }
        true
    }

    /// `[` / `]`: the polygon side count, for the tool and the selection.
    pub(super) fn adjust_sides(&mut self, delta: i32) -> bool {
        self.sides = (self.sides as i32 + delta).clamp(3, 24) as u32;
        let sides = self.sides;
        println!("polygon sides: {}", self.sides);
        if self.selection.iter().any(|&i| self.shapes[i].is_ngon()) {
            self.record(Tag::Sides);
        }
        let changed = self.with_selected(|s| s.set_sides(sides));
        if changed {
            self.mark_posed_selection();
        }
        changed
    }

    /// `C`: step to the next palette entry. Off-palette current colors start
    /// the cycle over rather than jumping somewhere arbitrary.
    pub(super) fn cycle_color(&mut self) -> bool {
        let next = self.palette_match().map_or(0, |i| (i + 1) % PALETTE.len());
        println!("color: {}", PALETTE_NAMES[next]);
        self.set_current_color(PALETTE[next], false);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use spark_render::Shape;

    /// One red shape, nothing selected.
    fn one_red() -> Editor {
        let mut e = Editor::empty();
        let mut s = Shape::circle([0.0, 0.0], 10.0);
        s.set_rgb([1.0, 0.0, 0.0]);
        e.shapes.push(s);
        e.names.push(String::new());
        e.anim.push(crate::anim::ShapeAnim::default());
        e.react.push([1.0; 3]);
        e.group.push(0);
        e.hidden.push(false);
        e
    }

    #[test]
    fn color_survives_with_nothing_selected() {
        // The whole point of the rework: choose a color before there's
        // anything to draw on, and have it stick.
        let mut e = one_red();
        e.set_current_color([0.0, 0.2, 1.0], false);
        assert_eq!(e.color(), [0.0, 0.2, 1.0]);
        // ...and the next shape draws with it, not with a palette entry.
        assert_eq!(e.shapes[0].rgb(), [1.0, 0.0, 0.0], "no selection, no paint");
    }

    #[test]
    fn selecting_a_layer_leaves_the_color_alone() {
        // Selection is not allowed to move the current color — that's the
        // eyedropper's job, and only the eyedropper's.
        let mut e = one_red();
        e.set_current_color([0.0, 0.2, 1.0], false);
        e.select(Some(0));
        assert_eq!(e.color(), [0.0, 0.2, 1.0]);
    }

    #[test]
    fn eyedropper_takes_the_shapes_color() {
        let mut e = one_red();
        e.set_current_color([0.0, 0.2, 1.0], false);
        assert!(e.eyedrop(0));
        assert_eq!(e.color(), [1.0, 0.0, 0.0]);
        // And it left the shape exactly as it found it.
        assert_eq!(e.shapes[0].rgb(), [1.0, 0.0, 0.0]);
    }

    #[test]
    fn editing_color_with_a_selection_paints_it() {
        let mut e = one_red();
        e.select(Some(0));
        e.set_current_color([0.0, 0.2, 1.0], false);
        assert_eq!(e.color(), [0.0, 0.2, 1.0]);
        assert_eq!(e.shapes[0].rgb(), [0.0, 0.2, 1.0]);
    }

    #[test]
    fn swatch_always_reports_a_change() {
        // The ring has to follow the current color even when no shape moved,
        // or the swatch looks dead with nothing selected.
        let mut e = one_red();
        assert!(e.set_color_index(3, false));
        assert_eq!(e.color(), PALETTE[3]);
        assert_eq!(e.palette_match(), Some(3));
    }

    #[test]
    fn cycle_from_an_off_palette_color_restarts() {
        let mut e = one_red();
        e.set_current_color([0.123, 0.456, 0.789], false);
        assert_eq!(e.palette_match(), None);
        e.cycle_color();
        assert_eq!(e.color(), PALETTE[0]);
    }
}
