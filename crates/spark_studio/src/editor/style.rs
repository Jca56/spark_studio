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
            palette: PALETTE.iter().position(|p| *p == rgb),
            grad: s.gradient(),
            rgb2: s.rgb2(),
            palette2: PALETTE.iter().position(|p| *p == s.rgb2()),
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

    /// Pick a palette color: becomes the draw color and recolors the
    /// selection. With `to_b`, gradient-enabled shapes take it as the
    /// gradient's end color instead.
    pub fn set_color_index(&mut self, i: usize, to_b: bool) -> bool {
        self.palette = i % PALETTE.len();
        let rgb = PALETTE[self.palette];
        if let [sel] = self.selection[..]
            && !(to_b && self.shapes[sel].gradient())
            && self.shapes[sel].rgb() == rgb
        {
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

    /// `C`: step to the next palette entry.
    pub(super) fn cycle_color(&mut self) -> bool {
        self.palette = (self.palette + 1) % PALETTE.len();
        let rgb = PALETTE[self.palette];
        println!("color: {}", PALETTE_NAMES[self.palette]);
        self.record(Tag::Color);
        self.with_selected(|s| s.set_rgb(rgb))
    }
}
