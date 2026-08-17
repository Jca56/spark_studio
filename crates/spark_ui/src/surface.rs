//! Surfaces: the chrome's material vocabulary.
//!
//! A [`Surface`] is a complete recipe for how a *kind* of thing is painted —
//! a card, a sunken well, a floating panel — expressed once instead of
//! re-assembled at every call site. Before this existed, twenty-odd places
//! each spelled out their own fill, radius and border by hand, which is why
//! restyling the editor meant editing twenty-odd places and why they had all
//! quietly drifted apart from each other.
//!
//! Sizes here are **logical px**; [`Surface::rect`] multiplies by the output
//! scale on the way to a [`UiRect`], so a recipe is resolution-independent.
//!
//! Every knob is a plain number or a color, deliberately: a live material
//! editor is a slider per field and nothing else.

use spark_render::Viewport;

use crate::rect::{TURN, UiRect};
use crate::theme::Theme;

/// How one kind of chrome is painted. Zero means off, as in the renderer.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Surface {
    /// Fill, and the color a vertical gradient runs to. `fill_to` alpha 0
    /// keeps the fill flat.
    pub fill: [f32; 4],
    pub fill_to: [f32; 4],
    /// Corner radius, logical px.
    pub radius: f32,
    /// Border width in logical px (0 = none), inset, and its color.
    pub border: f32,
    pub border_color: [f32; 4],
    /// Rim light: `[top highlight, bottom shade, thickness logical px]`.
    pub bevel: [f32; 3],
    /// Drop shadow: `[drop, blur, alpha]`, logical px. Chrome light comes
    /// from straight above, so there is no sideways offset.
    pub shadow: [f32; 3],
    /// Inner shadow: `[drop, blur, alpha]`, logical px.
    pub inner: [f32; 3],
    /// Surface tooth, so a big panel isn't dead plastic.
    pub grain: f32,
}

impl Surface {
    /// A flat fill with a corner radius and nothing else — where every
    /// surface in Spark starts today.
    pub const fn flat(fill: [f32; 4], radius: f32) -> Self {
        Self {
            fill,
            fill_to: [0.0; 4],
            radius,
            border: 0.0,
            border_color: [0.0; 4],
            bevel: [0.0; 3],
            shadow: [0.0; 3],
            inner: [0.0; 3],
            grain: 0.0,
        }
    }

    /// A real border, `width` logical px, inset.
    pub const fn edge(mut self, width: f32, color: [f32; 4]) -> Self {
        self.border = width;
        self.border_color = color;
        self
    }

    /// A top-to-bottom gradient, the direction that reads as a lit surface.
    pub const fn shade(mut self, to: [f32; 4]) -> Self {
        self.fill_to = to;
        self
    }

    /// Rim light along the top edge, shade along the bottom.
    pub const fn lit(mut self, top: f32, bottom: f32, thickness: f32) -> Self {
        self.bevel = [top, bottom, thickness];
        self
    }

    /// Cast a shadow — the surface reads as sitting above its container.
    pub const fn raised(mut self, drop: f32, blur: f32, alpha: f32) -> Self {
        self.shadow = [drop, blur, alpha];
        self
    }

    /// Catch a shadow — the surface reads as a hole cut into its container.
    pub const fn recessed(mut self, drop: f32, blur: f32, alpha: f32) -> Self {
        self.inner = [drop, blur, alpha];
        self
    }

    pub const fn textured(mut self, amount: f32) -> Self {
        self.grain = amount;
        self
    }

    /// The same material at a different corner — for the few places whose
    /// geometry differs but whose material should not.
    pub const fn at_radius(mut self, radius: f32) -> Self {
        self.radius = radius;
        self
    }

    /// The same material in a different color — hover and pressed states.
    pub const fn filled(mut self, fill: [f32; 4]) -> Self {
        self.fill = fill;
        self
    }

    /// Paint it. This is the only place logical px become physical.
    pub fn rect(&self, v: Viewport, scale: f32) -> UiRect {
        let mut r = UiRect::region_rounded(v, self.fill, self.radius * scale);
        if self.fill_to[3] > 0.0 {
            r = r.gradient(self.fill_to, TURN);
        }
        if self.border > 0.0 {
            r = r.stroke(self.border * scale, self.border_color);
        }
        if self.bevel[0] > 0.0 || self.bevel[1] > 0.0 {
            r = r.bevel(self.bevel[0], self.bevel[1], self.bevel[2] * scale);
        }
        if self.shadow[2] > 0.0 {
            r = r.shadow(
                [0.0, self.shadow[0] * scale],
                self.shadow[1] * scale,
                0.0,
                [0.0, 0.0, 0.0, self.shadow[2]],
            );
        }
        if self.inner[2] > 0.0 {
            r = r.inner_shadow(
                [0.0, self.inner[0] * scale],
                self.inner[1] * scale,
                0.0,
                [0.0, 0.0, 0.0, self.inner[2]],
            );
        }
        if self.grain > 0.0 {
            r = r.grain(self.grain, 2.0 * scale);
        }
        r
    }

    /// Painted with its border recolored — selection, focus, armed state.
    /// Falls back to a hairline if the material carries no border, so a
    /// borderless surface can still be marked.
    pub fn edged(&self, v: Viewport, scale: f32, color: [f32; 4]) -> UiRect {
        let width = if self.border > 0.0 { self.border } else { 2.0 };
        self.edge(width, color).rect(v, scale)
    }

    /// Painted with a ring *outside* it — for marking something whose own
    /// color has to stay readable, like a swatch or a gradient endpoint.
    pub fn ringed(&self, v: Viewport, scale: f32, width: f32, color: [f32; 4]) -> UiRect {
        self.rect(v, scale).stroke_outer(width * scale, color)
    }
}

/// The materials the editor is built out of.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Surfaces {
    /// A list row that must read as its own object: layer cards, lanes.
    pub card: Surface,
    /// A folder header — flatter than a card, so members read as beneath it.
    pub header: Surface,
    /// A raised button: toolbar squares, the keyframe stamp.
    pub plate: Surface,
    /// A sunken box: scrub fields, name boxes, anything typed into.
    pub well: Surface,
    /// A panel floating over the chrome: menus, popups.
    pub float: Surface,
    /// A text input at rest.
    pub field: Surface,
    /// The wash under a hovered control.
    pub hover: Surface,
}

impl Surfaces {
    /// Spark's materials, derived from a palette.
    ///
    /// Flat on purpose *for now*: this reproduces exactly what the editor
    /// looked like before surfaces existed, so adopting them changed no
    /// pixels. The depth knobs — `lit`, `raised`, `recessed`, `shade`,
    /// `textured` — are all wired and all at zero, waiting to be dialled in
    /// rather than guessed at.
    pub fn from_theme(t: &Theme) -> Self {
        Self {
            card: Surface::flat(t.card, 12.0).edge(2.5, t.card_border),
            header: Surface::flat(t.header, 12.0).edge(2.5, t.card_border),
            plate: Surface::flat(t.card, 12.0).edge(2.0, t.plate_edge),
            well: Surface::flat(t.well, 6.0),
            float: Surface::flat(t.card, 10.0).edge(3.0, t.seam),
            field: Surface::flat(t.slider_track, 8.0).edge(3.0, t.seam),
            hover: Surface::flat(t.button_hover, 8.0),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::default_theme;

    fn vp() -> Viewport {
        Viewport {
            x: 10.0,
            y: 20.0,
            w: 100.0,
            h: 40.0,
        }
    }

    /// Logical px become physical exactly once, in `rect`.
    #[test]
    fn scale_is_applied_to_every_length() {
        let s = Surface::flat([1.0; 4], 6.0)
            .edge(2.0, [0.5; 4])
            .lit(0.2, 0.1, 3.0)
            .raised(4.0, 10.0, 0.5)
            .recessed(1.0, 5.0, 0.4);
        let r = s.rect(vp(), 2.0);
        assert_eq!(r.radii, [12.0; 4], "radius");
        assert_eq!(r.edge[0], 4.0, "border width");
        assert_eq!(r.bevel[2], 6.0, "bevel thickness");
        assert_eq!([r.outer[1], r.outer[2]], [8.0, 20.0], "shadow drop, blur");
        assert_eq!([r.inner[1], r.inner[2]], [2.0, 10.0], "inner drop, blur");
        // Geometry is already physical and must not be scaled again.
        assert_eq!(r.pos, [10.0, 20.0]);
        assert_eq!(r.size, [100.0, 40.0]);
    }

    /// Every knob at zero has to stay off, or a flat surface starts paying
    /// for effects it never asked for.
    #[test]
    fn a_flat_surface_switches_nothing_on() {
        let r = Surface::flat([1.0; 4], 6.0).rect(vp(), 1.0);
        assert_eq!(r.grad[0], 0.0, "no gradient");
        assert_eq!(r.edge[0], 0.0, "no border");
        assert_eq!(r.outer_color[3], 0.0, "no shadow");
        assert_eq!(r.inner_color[3], 0.0, "no inner shadow");
        assert_eq!(r.bevel, [0.0; 4], "no bevel");
        assert_eq!(r.grain[0], 0.0, "no grain");
    }

    #[test]
    fn edged_recolors_the_border_and_nothing_else() {
        let s = Surfaces::from_theme(&default_theme()).card;
        let plain = s.rect(vp(), 1.0);
        let marked = s.edged(vp(), 1.0, [1.0, 0.0, 0.0, 1.0]);
        assert_eq!(marked.edge[0], plain.edge[0], "same width");
        assert_eq!(marked.radii, plain.radii, "same corners");
        assert_eq!(marked.color, plain.color, "same fill");
        assert_eq!(marked.edge_color, [1.0, 0.0, 0.0, 1.0]);
    }

    /// A borderless material still has to be markable.
    #[test]
    fn edged_gives_a_borderless_surface_something_to_show() {
        let well = Surfaces::from_theme(&default_theme()).well;
        assert_eq!(well.border, 0.0, "well is borderless at rest");
        assert!(
            well.edged(vp(), 1.0, [1.0; 4]).edge[0] > 0.0,
            "but markable"
        );
    }

    /// A ring marks without covering: it must sit outside the edge.
    #[test]
    fn ringed_puts_the_ring_outside() {
        let r = Surfaces::from_theme(&default_theme())
            .well
            .ringed(vp(), 2.0, 3.0, [1.0; 4]);
        assert_eq!(r.edge[0], 6.0, "width scaled");
        assert_eq!(r.edge[1], 1.0, "aligned outward");
    }

    /// Adopting surfaces was meant to change **zero pixels**: every recipe
    /// here has to still produce exactly the `UiRect` the call site used to
    /// spell out by hand. Written out longhand on purpose — this is the
    /// receipt for "the refactor is invisible", so it must not be expressed
    /// in terms of the thing it is checking.
    #[test]
    fn every_material_matches_the_hand_written_original() {
        let t = default_theme();
        let c = Surfaces::from_theme(&t);
        let s = 2.0;
        let v = vp();
        let round = |fill, r: f32| UiRect::region_rounded(v, fill, r * s);
        let cases: [(&str, UiRect, UiRect); 7] = [
            (
                "card",
                c.card.rect(v, s),
                round(t.card, 12.0).stroke(2.5 * s, t.card_border),
            ),
            (
                "header",
                c.header.rect(v, s),
                round(t.header, 12.0).stroke(2.5 * s, t.card_border),
            ),
            (
                "plate",
                c.plate.rect(v, s),
                round(t.card, 12.0).stroke(2.0 * s, t.plate_edge),
            ),
            ("well", c.well.rect(v, s), round(t.well, 6.0)),
            (
                "float",
                c.float.rect(v, s),
                round(t.card, 10.0).stroke(3.0 * s, t.seam),
            ),
            (
                "field",
                c.field.rect(v, s),
                round(t.slider_track, 8.0).stroke(3.0 * s, t.seam),
            ),
            ("hover", c.hover.rect(v, s), round(t.button_hover, 8.0)),
        ];
        for (name, got, want) in cases {
            assert_eq!(got, want, "{name} drifted from what it replaced");
        }
    }

    /// Two call sites take the same material at a different size. Those
    /// derivations must change only what they name.
    #[test]
    fn derivations_change_only_what_they_name() {
        let t = default_theme();
        let well = Surfaces::from_theme(&t).well;
        let wide = well.at_radius(10.0).rect(vp(), 2.0);
        assert_eq!(wide, UiRect::region_rounded(vp(), t.well, 20.0));
        let deep = well.filled(t.well_deep).at_radius(10.0).rect(vp(), 2.0);
        assert_eq!(deep, UiRect::region_rounded(vp(), t.well_deep, 20.0));
    }
}
