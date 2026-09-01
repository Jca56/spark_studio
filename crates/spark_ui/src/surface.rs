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
    /// Fill, and the color the gradient runs to. `fill_to` alpha 0 keeps
    /// the fill flat — zero means off, as everywhere else.
    pub fill: [f32; 4],
    pub fill_to: [f32; 4],
    /// Where the gradient goes: `[direction in turns, radial]`. The
    /// direction was hardcoded to straight down, which is the right default
    /// for a lit surface and the wrong one for everything else — the shader
    /// has always taken any angle, and a radial, and neither was reachable
    /// from a recipe. Radial ignores the direction.
    pub grad: [f32; 2],
    /// Where along the surface the blend happens: `[start, end]`, both 0..1.
    /// `[0, 1]` runs it corner to corner — which is all a gradient could do
    /// until this existed, so "fade across the left quarter" was unaskable.
    pub grad_span: [f32; 2],
    /// Corner radius, logical px.
    pub radius: f32,
    /// Border width in logical px (0 = none), inset, and its color.
    pub border: f32,
    pub border_color: [f32; 4],
    /// Rim light: `[top highlight, bottom shade, thickness logical px,
    /// lit-from-below flip]` — the flip is what a recess's bottom lip uses.
    pub bevel: [f32; 4],
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
            grad: [TURN, 0.0],
            grad_span: [0.0, 1.0],
            radius,
            border: 0.0,
            border_color: [0.0; 4],
            bevel: [0.0; 4],
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

    /// A gradient toward `to`, top-to-bottom — the direction that reads as
    /// a lit surface.
    pub const fn shade(mut self, to: [f32; 4]) -> Self {
        self.fill_to = to;
        self
    }

    /// Which way the gradient runs, in turns: `0.0` left→right, [`TURN`]
    /// top→bottom, `0.5` right→left.
    pub const fn toward(mut self, turns: f32) -> Self {
        self.grad[0] = turns;
        self
    }

    /// Confine the blend to a band: before `start` the surface is its fill,
    /// after `end` it is the far colour.
    pub const fn span(mut self, start: f32, end: f32) -> Self {
        self.grad_span = [start, end];
        self
    }

    /// Run the gradient center→corners instead of along a direction.
    pub const fn radial(mut self, on: bool) -> Self {
        self.grad[1] = if on { 1.0 } else { 0.0 };
        self
    }

    /// Rim light along the top edge, shade along the bottom.
    pub const fn lit(mut self, top: f32, bottom: f32, thickness: f32) -> Self {
        self.bevel = [top, bottom, thickness, 0.0];
        self
    }

    /// Rim light along the *bottom* edge — the sliver of light a recess's
    /// lip catches, Lantern Mix's sunken treatment.
    pub const fn lit_below(mut self, light: f32, thickness: f32) -> Self {
        self.bevel = [light, 0.0, thickness, 1.0];
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
            r = match self.grad[1] > 0.5 {
                true => r.gradient_radial(self.fill_to),
                false => r.gradient(self.fill_to, self.grad[0]),
            };
            if self.grad_span != [0.0, 1.0] {
                r = r.gradient_span(self.grad_span[0], self.grad_span[1]);
            }
        }
        if self.border > 0.0 {
            r = r.stroke(self.border * scale, self.border_color);
        }
        if self.bevel[0] > 0.0 || self.bevel[1] > 0.0 {
            r = match self.bevel[3] > 0.5 {
                true => r.bevel_below(self.bevel[0], self.bevel[2] * scale),
                false => r.bevel(self.bevel[0], self.bevel[1], self.bevel[2] * scale),
            };
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

/// A darker version of `c` — the bottom end of a lit surface's gradient.
/// `amount` is 0..1; the cap keeps a shaded surface from bottoming out to
/// black, which reads as a hole rather than a lit face.
/// A shade keeps the fill's own transparency: darkening a translucent
/// surface must not quietly make it solid.
pub fn darken(c: [f32; 4], amount: f32) -> [f32; 4] {
    let k = 1.0 - amount.clamp(0.0, 1.0) * SHADE_DEPTH;
    [c[0] * k, c[1] * k, c[2] * k, c[3]]
}

/// How dark the far end of a full-strength shade goes.
pub const SHADE_DEPTH: f32 = 0.6;

/// A lighter version of `c` — the top end of a lit surface's gradient or a
/// hover step, pushed toward white. Alpha rides through untouched.
pub fn lighten(c: [f32; 4], amount: f32) -> [f32; 4] {
    let k = amount.clamp(0.0, 1.0);
    [
        c[0] + (1.0 - c[0]) * k,
        c[1] + (1.0 - c[1]) * k,
        c[2] + (1.0 - c[2]) * k,
        c[3],
    ]
}

/// The materials the editor is built out of.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Surfaces {
    // -- window regions ------------------------------------------------
    // The largest surfaces in the editor, and until now the only ones with
    // no material at all: they were painted as bare fills, so the side
    // panels could be recoloured but never shaded, textured or lit.
    /// The left and right side panels.
    pub panel: Surface,
    /// The tool strip, the transport bar and the zoom strip.
    pub bar: Surface,
    /// The bottom panel behind the timeline.
    pub timeline: Surface,
    /// The strip along the very bottom of the window.
    pub status: Surface,

    // -- things on those panels ----------------------------------------
    /// A box that must read as its own object: layer cards, timeline lanes.
    pub card: Surface,
    /// A box nested inside one of those — the cog-expanded settings block.
    pub card_inner: Surface,
    /// An effect's card, sitting on that block.
    pub fx_card: Surface,
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
    /// Spark's materials — the Lantern Mix treatment (2026-08-31), dialled
    /// onto the knobs that had sat wired-at-zero since surfaces landed.
    /// The physics: **everything is lit from above.** Raised faces shade
    /// downward with a thin highlight along the top edge; recesses catch
    /// an inset shadow from above and a sliver of light on the bottom lip;
    /// floating panels sit on a drop shadow. Spark keeps its own palette —
    /// the grey ladder, gold/purple accents — the treatment is what came
    /// over, not Lantern Mix's neutral accent.
    pub fn from_theme(t: &Theme) -> Self {
        Self {
            // Square by definition — a window region meets its neighbours,
            // so a corner radius here would cut a hole in the layout. The
            // face runs from a *lifted* top to a darkened floor — the
            // lighten lead is the whole trick at these dark values: the
            // darken half alone came out to one sRGB count on 0x151515
            // ("Umm... it looks exactly the same?" — Alva, correctly).
            panel: Surface::flat(lighten(t.panel, 0.01), 0.0)
                .shade(darken(t.panel, 0.4))
                .textured(0.03),
            bar: Surface::flat(lighten(t.toolbar, 0.02), 0.0)
                .shade(darken(t.toolbar, 0.4))
                .lit(0.06, 0.0, 1.5),
            // The timeline is ground, not object: no lift — it fades from
            // its base toward near-black, so the clips and rows read as
            // the lit things on it (Alva: "I thought it used to be nearly
            // black... everything got lighter instead of darker").
            timeline: Surface::flat(t.timeline, 0.0)
                .shade(darken(t.timeline, 0.55))
                .textured(0.03),
            // The status strip only recedes — it closes the layout by
            // being darker, and a lift would fight that.
            status: Surface::flat(t.status, 0.0).shade(darken(t.status, 0.35)),
            // A raised object: lit face (Lantern Mix's exact lift), top
            // highlight, floating just off the panel.
            card: Surface::flat(lighten(t.card, 0.06), 12.0)
                .shade(darken(t.card, 0.30))
                .lit(0.10, 0.0, 1.5)
                .edge(2.0, t.card_border)
                .raised(2.0, 6.0, 0.6),
            // Borderless at rest: it is already bounded by the card it sits
            // in. Gently recessed, so settings read as set *into* the card.
            card_inner: Surface::flat(t.card_inner, 10.0)
                .edge(0.0, t.card_border)
                .recessed(1.5, 4.0, 0.45),
            // Sunk below the block it sits on, and edged — a dark box on a
            // dark box needs a line to say where one stops.
            fx_card: Surface::flat(t.fx_card, 10.0)
                .edge(2.0, t.card_border)
                .recessed(1.5, 4.0, 0.5),
            // Flatter than a card, so members read as beneath it.
            header: Surface::flat(lighten(t.header, 0.03), 12.0)
                .shade(darken(t.header, 0.25))
                .edge(2.5, t.card_border),
            // A pressable face: the strongest raise in the set.
            plate: Surface::flat(lighten(t.button, 0.06), 12.0)
                .shade(darken(t.button, 0.30))
                .lit(0.12, 0.0, 1.5)
                .edge(2.0, t.plate_edge)
                .raised(1.5, 3.0, 0.55),
            // A recess you type into: inset shadow from above, lit lip
            // below, and its edge — a box you can click into, not a gap.
            well: Surface::flat(t.well, 6.0)
                .edge(1.5, t.card_border)
                .recessed(2.0, 5.0, 0.55)
                .lit_below(0.08, 1.5),
            // Floats over everything: the deep drop shadow is what says so.
            // Toned like the side panels (menus looked like light-grey
            // cards next to them), with the gold seam border — the lntrn
            // menu look, on Spark's own ground.
            float: Surface::flat(lighten(t.panel, 0.02), 10.0)
                .shade(darken(t.panel, 0.5))
                .lit(0.07, 0.0, 1.5)
                .edge(3.0, t.seam)
                .raised(4.0, 8.0, 0.55),
            field: Surface::flat(t.slider_track, 8.0)
                .edge(3.0, t.seam)
                .recessed(2.0, 5.0, 0.5)
                .lit_below(0.06, 1.5),
            hover: Surface::flat(t.button_hover, 8.0).edge(0.0, t.card_border),
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

    /// The shader has always taken any angle and a radial; until now a
    /// recipe could only ever ask for straight down.
    #[test]
    fn a_gradient_can_run_any_direction() {
        let to = [0.2, 0.3, 0.4, 1.0];
        let down = Surface::flat([1.0; 4], 6.0).shade(to).rect(vp(), 1.0);
        assert_eq!(down.grad[0], 1.0, "armed");
        assert_eq!(down.grad[1], TURN, "top to bottom by default");
        assert_eq!(down.color2, to);

        let across = Surface::flat([1.0; 4], 6.0)
            .shade(to)
            .toward(0.0)
            .rect(vp(), 1.0);
        assert_eq!(across.grad[1], 0.0, "left to right");

        let out = Surface::flat([1.0; 4], 6.0)
            .shade(to)
            .radial(true)
            .rect(vp(), 1.0);
        assert_eq!(out.grad[2], crate::rect::GRAD_RADIAL, "center to corners");
    }

    /// Direction is stored whether or not a gradient is armed, but an
    /// unarmed one still paints flat — zero means off, and a leftover angle
    /// must not switch anything on.
    #[test]
    fn a_direction_alone_does_not_arm_a_gradient() {
        let r = Surface::flat([1.0; 4], 6.0)
            .toward(0.1)
            .radial(true)
            .rect(vp(), 1.0);
        assert_eq!(r.grad[0], 0.0, "no end colour, no gradient");
    }

    /// Darkening a translucent surface must not quietly make it solid.
    #[test]
    fn a_shade_keeps_the_fill_transparency() {
        let c = darken([0.8, 0.8, 0.8, 0.4], 0.5);
        assert_eq!(c[3], 0.4);
        assert!(c[0] < 0.8, "and still darkens");
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
        let hover = Surfaces::from_theme(&default_theme()).hover;
        assert_eq!(hover.border, 0.0, "hover is borderless at rest");
        assert!(
            hover.edged(vp(), 1.0, [1.0; 4]).edge[0] > 0.0,
            "but markable"
        );
    }

    /// A number box has a visible edge, so it reads as a box you can click
    /// into rather than as a gap in the card it sits on.
    #[test]
    fn the_number_box_has_an_edge() {
        let well = Surfaces::from_theme(&default_theme()).well;
        assert!(well.border > 0.0, "the number box lost its border");
        assert_eq!(
            well.rect(vp(), 2.0).edge[1],
            0.0,
            "inside the edge, so it can't crop the number"
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

    /// The Lantern Mix treatment, receipted: everything is lit from
    /// above. Raised things shade downward, carry a top highlight and cast
    /// a shadow; recesses catch an inset shadow and a lit bottom lip;
    /// floats sit on the deepest shadow of the set. Nobody who can run
    /// this can see the chrome, so the physics are asserted, not eyeballed.
    #[test]
    fn the_chrome_is_lit_from_above() {
        let t = default_theme();
        let c = Surfaces::from_theme(&t);
        let raised = [("card", c.card), ("plate", c.plate), ("float", c.float)];
        for (name, s) in raised {
            assert!(s.fill_to[3] > 0.0, "{name}: no face gradient");
            assert!(
                s.fill_to[0] < s.fill[0],
                "{name}: the gradient must darken downward"
            );
            assert_eq!(s.grad, [TURN, 0.0], "{name}: lit from straight above");
            assert!(s.bevel[0] > 0.0, "{name}: no top highlight");
            assert_eq!(s.bevel[3], 0.0, "{name}: lit from above, not below");
            assert!(s.shadow[2] > 0.0, "{name}: a raised face casts a shadow");
            assert_eq!(s.inner[2], 0.0, "{name}: raised, not also recessed");
        }
        let sunken = [("well", c.well), ("field", c.field)];
        for (name, s) in sunken {
            assert!(s.inner[2] > 0.0, "{name}: a recess catches a shadow");
            assert!(s.bevel[0] > 0.0, "{name}: no lip light");
            assert_eq!(s.bevel[3], 1.0, "{name}: the lip is lit from below");
            assert_eq!(s.shadow[2], 0.0, "{name}: a recess casts nothing");
        }
        // The float's shadow is the deepest — it is the only thing that
        // truly leaves the surface.
        assert!(c.float.shadow[1] > c.card.shadow[1]);
        // Window regions stay square and shadowless: they meet their
        // neighbours, and a shadow there would draw on the seam.
        for (name, s) in [
            ("panel", c.panel),
            ("bar", c.bar),
            ("timeline", c.timeline),
            ("status", c.status),
        ] {
            assert_eq!(s.radius, 0.0, "{name}: regions are square");
            assert_eq!(s.shadow[2], 0.0, "{name}: regions cast nothing");
        }
        // Identity holds: the float keeps Spark's gold seam border.
        assert_eq!(c.float.border_color, t.seam, "the float lost its gold");
    }

    /// The flip reaches the instance: a recess's bevel arrives flagged
    /// lit-from-below, a raised face's does not.
    #[test]
    fn the_bevel_flip_reaches_the_rect() {
        let c = Surfaces::from_theme(&default_theme());
        assert_eq!(c.well.rect(vp(), 2.0).bevel[3], 1.0, "well lip flipped");
        assert_eq!(c.plate.rect(vp(), 2.0).bevel[3], 0.0, "plate lit on top");
        assert!(c.plate.rect(vp(), 2.0).bevel[0] > 0.0);
    }

    /// Lighten pushes toward white and keeps alpha, mirroring darken.
    #[test]
    fn lighten_mirrors_darken() {
        let c = lighten([0.2, 0.4, 0.6, 0.5], 0.5);
        assert!(c[0] > 0.2 && c[1] > 0.4 && c[2] > 0.6);
        assert_eq!(c[3], 0.5, "alpha rides through");
        assert_eq!(lighten([0.3; 4], 0.0), [0.3; 4], "zero is a no-op");
    }

    /// Two call sites take the same material at a different size. Those
    /// derivations must change only what they name.
    #[test]
    fn derivations_change_only_what_they_name() {
        let t = default_theme();
        let well = Surfaces::from_theme(&t).well;
        let wide = well.at_radius(10.0).rect(vp(), 2.0);
        let base = well.rect(vp(), 2.0);
        assert_eq!(wide.radii, [20.0; 4], "the named change");
        assert_eq!(wide.inner, base.inner, "the recess came along");
        assert_eq!(wide.bevel, base.bevel, "the lip came along");
        // `filled` changes the fill and leaves everything else alone.
        let deep = well.filled(t.well_deep).rect(vp(), 2.0);
        assert_eq!(deep.color, t.well_deep);
        assert_eq!(deep.inner, base.inner);
        assert_eq!(deep.edge, base.edge);
    }

}
