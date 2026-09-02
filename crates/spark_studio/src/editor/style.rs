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
            size: crate::props::extent(s),
            w: s.box_size().map(|b| b[0]),
            h: s.box_size().map(|b| b[1]),
            d: s.depth(),
            z: s.z(),
            tilt: s.tilt(),
            turn: s.turn(),
            ends: s.is_line().then(|| s.line_ends()),
            rgb,
            // Read off the Gradient *effect*: the shape's own end colour is
            // written by `fx::resolve` on the display copy each frame, so
            // the document's copy of it says nothing about what is on
            // screen.
            rgb2: match self.colour_effect(i) {
                Some((id, c)) => {
                    let e = self.fx[i].find(id);
                    match e {
                        Some(e) => [
                            e.get(c as usize),
                            e.get(c as usize + 1),
                            e.get(c as usize + 2),
                        ],
                        None => [0.0; 3],
                    }
                }
                None => [0.0; 3],
            },
        })
    }

    /// Absolute-value sliders write to the primary shape.
    pub fn set_prop(&mut self, prop: Prop, value: f32) -> bool {
        let Some(i) = self.primary() else {
            return false;
        };
        self.record(Tag::Prop(prop));
        if prop == Prop::Scale {
            // The card speaks full sizes (see `props::extent`); a light's
            // is its range as it is.
            let cur = crate::props::extent(&self.shapes[i]);
            if cur > 0.001 {
                self.scale_index(i, value / cur);
            }
            self.mark_posed(&[i]);
            return true;
        }
        // Glow is an effect, not a shape field. Asking for glow is how you
        // add the effect: the slider and `A` both reach for it, and neither
        // should require visiting a browser first.
        if prop == Prop::Glow {
            return self.set_glow_selection(value);
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
            // A line's ends, one coordinate at a time — the other end
            // holds, which is the whole point of them.
            Prop::X1 | Prop::Y1 | Prop::X2 | Prop::Y2 => crate::anim::apply_prop(s, prop, value),
            Prop::Rotation => s.set_rotation(value),
            Prop::Z => s.set_z(value),
            Prop::Tilt => s.set_tilt(value),
            Prop::Turn => s.set_turn(value),
            Prop::Scale => unreachable!("handled above"),
            Prop::Width => s.set_box_width(value),
            Prop::Height => s.set_box_height(value),
            Prop::Brightness => s.set_brightness(value),
            Prop::Opacity => s.set_opacity(value),
            // Handled above — glow is an effect, not a shape field.
            Prop::Glow => {}
            Prop::Sides => s.set_sides(value.round() as u32),
            Prop::Thickness => s.set_thickness(value),
            Prop::Cone => s.set_cone(value),
            Prop::Rim => s.set_rim(value),
            Prop::Depth => s.set_depth(value),
            Prop::Density => s.set_density(value),
            Prop::Twinkle => s.set_twinkle(value),
            Prop::TwinkleRate => s.set_twinkle_rate(value),
            Prop::Seed => s.set_seed(value),
            Prop::Jag => s.set_jag(value),
            Prop::Branches => s.set_branches(value),
            Prop::Strike => s.set_strike_rate(value),
            Prop::Hole => s.set_hole(value),
            Prop::Twist => s.set_twist(value),
            Prop::Spin => s.set_spin(value),
            Prop::Grain => s.set_grain(value),
            Prop::Shake => s.set_shake(value),
            Prop::ShakeRate => s.set_shake_rate(value),
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
    #[allow(dead_code)] // kept for the redesign; the old panels were the only caller
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
        // The gradient's far end belongs to the Gradient *effect* now, so
        // the B chip paints its colour parameters. The shape's own `rgb2`
        // is written by `fx::resolve` on the display copy every frame, so
        // anything set here would have been overwritten before it drew —
        // which is exactly what made the old endpoint chips dead controls.
        let mut painted = false;
        for i in self.selection.clone() {
            match to_b.then(|| self.colour_effect(i)).flatten() {
                Some((id, c)) => {
                    if let Some(e) = self.fx[i].find_mut(id) {
                        for (k, channel) in rgb.iter().enumerate() {
                            e.set(c as usize + k, *channel);
                        }
                        painted = true;
                    }
                }
                // No armed effect end to paint: this is the shape's own
                // colour, the way it is whenever B isn't armed.
                None => {
                    self.shapes[i].set_rgb(rgb);
                    painted = true;
                }
            }
        }
        if painted {
            self.mark_posed_selection();
        }
        painted
    }

    /// Set the background colour, and paint it onto the gradient end of
    /// every selected shape that carries a Gradient effect — a shape
    /// without one has no far end for it, and its own colour is the
    /// foreground's. True when something on screen changed.
    pub fn set_color_b(&mut self, rgb: [f32; 3]) -> bool {
        self.color_b = rgb;
        let targets: Vec<(usize, u32, u8)> = self
            .selection
            .iter()
            .filter_map(|&i| self.colour_effect(i).map(|(id, c)| (i, id, c)))
            .collect();
        if targets.is_empty() {
            return false;
        }
        self.record(Tag::Color);
        for (i, id, c) in targets {
            if let Some(e) = self.fx[i].find_mut(id) {
                for (k, channel) in rgb.iter().enumerate() {
                    e.set(c as usize + k, *channel);
                }
            }
        }
        self.mark_posed_selection();
        true
    }

    /// The colour-owning effect on layer `i`, as `(effect id, first
    /// channel)`. Turned-off effects count: tuning a colour you can't
    /// currently see is a perfectly ordinary thing to want.
    pub fn colour_effect(&self, i: usize) -> Option<(u32, u8)> {
        let e = self.fx.get(i)?.find_kind(crate::fx::EffectKind::Gradient)?;
        Some((e.id, e.kind.colour_param()?))
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

    /// Which star a field scatters. Applies to every selected field, so a
    /// multi-selection restyles in one click; shapes that aren't fields
    /// ignore it.
    #[allow(dead_code)] // kept for the redesign; the old panels were the only caller
    pub fn set_star_form(&mut self, form: usize) -> bool {
        let Some(i) = self.primary() else {
            return false;
        };
        if self.shapes[i].star_form() == Some(form) {
            return false;
        }
        let s = self.snap();
        self.history.push(s);
        for &j in &self.selection {
            self.shapes[j].set_star_form(form);
        }
        true
    }

    #[allow(dead_code)] // kept for the redesign; the old panels were the only caller
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

    /// Set the glow radius across the selection, adding the Glow effect to
    /// any layer that hasn't got one.
    ///
    /// **Zero does not remove it.** An effect parked at zero is a real thing
    /// to want — glow held at nothing through a verse so it can be keyed up
    /// into the drop — and removing it there would take its keyframes with
    /// it the moment a slider drag passed through the bottom of its range.
    /// Effects leave the stack only when you say so.
    ///
    /// Setting zero on a layer that *hasn't* got the effect is still a
    /// no-op, so `Z` on a shape with no glow doesn't conjure one at zero.
    pub fn set_glow_selection(&mut self, v: f32) -> bool {
        if self.selection.is_empty() {
            return false;
        }
        let v = crate::props::fit(Prop::Glow, v, self.canvas);
        let kind = crate::fx::EffectKind::Glow;
        let touched: Vec<usize> = self
            .selection
            .iter()
            .copied()
            .filter(|&i| v > 0.0 || self.fx[i].find_kind(kind).is_some())
            .collect();
        if touched.is_empty() {
            return false;
        }
        self.record(Tag::KeyGlow);
        for i in touched {
            let stack = &mut self.fx[i];
            let id = stack.add(kind, stack.next_id());
            if let Some(e) = stack.find_mut(id) {
                e.set(0, v);
            }
        }
        self.mark_posed_selection();
        true
    }

    /// `A` / `Z`: step the selection's glow, from whatever it reads now.
    pub(super) fn nudge_glow(&mut self, delta: f32) -> bool {
        let Some(i) = self.primary() else {
            return false;
        };
        let cur = self.fx[i]
            .active(crate::fx::EffectKind::Glow)
            .map(|e| e.get(0))
            .unwrap_or(0.0);
        self.set_glow_selection(cur + delta)
    }

    /// `[` / `]`: the polygon side count, for the tool's defaults and the
    /// selection.
    pub(super) fn adjust_sides(&mut self, delta: i32) -> bool {
        let d = self.defaults.get_mut(crate::props::Tool::Polygon);
        d.sides = (d.sides as i32 + delta).clamp(3, 24) as u32;
        let sides = d.sides;
        println!("polygon sides: {sides}");
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
        e.push_shape(s);
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
