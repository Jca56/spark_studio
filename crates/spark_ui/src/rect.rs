//! The UI material instance.
//!
//! One `UiRect` is one *complete* piece of chrome. The shader composites the
//! whole stack in a single quad, back to front:
//!
//! ```text
//!   drop shadow   outside the shape (blur + spread + offset, any color)
//!   fill          solid, or a gradient at any angle, or radial
//!   inner shadow  carved inward from the edge — sunken wells, pressed keys
//!   bevel         rim light on up-facing edges, shade on down-facing ones
//!   grain         per-pixel noise so a surface isn't dead plastic
//!   stroke        a REAL border, inset, riding the shape's own distance field
//! ```
//!
//! **On borders.** Spark used to fake every edge by drawing a slightly bigger
//! rect behind a smaller one, which is why they never hugged the corners: a
//! 12px-radius plate behind a 10px-radius plate is two mismatched blobs, not
//! a border. `.stroke(w, color)` rides the shape's signed distance field, so
//! it is exactly `w` px thick everywhere, around every corner, on every
//! shape — glyphs included. Nothing in Spark should fake an edge again.
//!
//! Every field defaults to zero, and zero means "off". A `UiRect` built with
//! the old constructors renders exactly as it always did.

use spark_render::Viewport;

pub const ICON_NONE: f32 = 0.0;
pub const ICON_MINUS: f32 = 1.0;
pub const ICON_SQUARE: f32 = 2.0;
pub const ICON_X: f32 = 3.0;
pub const ICON_ARROW: f32 = 4.0;
pub const ICON_CIRCLE: f32 = 5.0;
pub const ICON_PENTAGON: f32 = 6.0;
pub const ICON_LINE: f32 = 7.0;
/// Samples the pass's bound image texture (tinted by `color`).
pub const ICON_IMAGE: f32 = 8.0;
pub const ICON_PLAY: f32 = 9.0;
pub const ICON_PAUSE: f32 = 10.0;
pub const ICON_PATH: f32 = 11.0;
/// Fill modes for the color picker (not glyphs).
pub const ICON_HSV: f32 = 12.0;
pub const ICON_HUE: f32 = 13.0;
/// Filled diamond — the keyframe marker.
pub const ICON_KEY: f32 = 14.0;
/// Cogwheel — settings/expand affordance.
pub const ICON_GEAR: f32 = 15.0;
/// Open eye — layer visible.
pub const ICON_EYE: f32 = 16.0;
/// Slashed eye — layer hidden.
pub const ICON_EYE_OFF: f32 = 17.0;
/// A thick line between two arbitrary points with round caps — the one thing
/// the chrome could never draw. Built by [`UiRect::line`].
pub const ICON_CAPSULE: f32 = 18.0;
/// A ring segment: knobs, radial meters, circular progress. Built by
/// [`UiRect::arc`].
pub const ICON_ARC: f32 = 19.0;
/// A chevron, pointing down at rest — [`UiRect::rotate`] aims it.
pub const ICON_CHEVRON: f32 = 20.0;
/// Three four-point sparkles of different sizes — the star field tool and
/// the kind glyph on its layer card.
pub const ICON_STARS: f32 = 21.0;

/// Gradient geometry, in `grad[2]`.
pub const GRAD_LINEAR: f32 = 0.0;
pub const GRAD_RADIAL: f32 = 1.0;

/// A quarter turn — `gradient(to, TURN)` runs top→bottom.
pub const TURN: f32 = 0.25;

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct UiRect {
    /// Top-left in window px.
    pub pos: [f32; 2],
    /// Size in window px. The drawn quad grows beyond this to fit a shadow.
    pub size: [f32; 2],
    /// Fill color — also the gradient's start and a glyph's ink.
    pub color: [f32; 4],
    /// `[kind, glyph thickness px, ngon sides, glyph radius factor]`
    pub icon: [f32; 4],
    /// Gradient end color.
    pub color2: [f32; 4],
    /// Corner radii px, clockwise from the top-left: `[tl, tr, br, bl]`.
    /// Capsules reuse it for their endpoints: `[ax, ay, bx, by]`, relative
    /// to the quad's center.
    pub radii: [f32; 4],
    /// `[on, angle in turns, kind, unused]` — see [`GRAD_LINEAR`].
    pub grad: [f32; 4],
    /// Where along the surface the blend happens: `[start, end, on, unused]`,
    /// both 0..1. Off runs it corner to corner.
    pub grad_span: [f32; 4],
    /// `[stroke width px, alignment, unused, unused]` — alignment is 0
    /// inside the edge, 1 outside it, 0.5 straddling.
    pub edge: [f32; 4],
    pub edge_color: [f32; 4],
    /// Drop shadow: `[offset x, offset y, blur px, spread px]`.
    pub outer: [f32; 4],
    pub outer_color: [f32; 4],
    /// Inner shadow: `[offset x, offset y, blur px, spread px]`.
    pub inner: [f32; 4],
    pub inner_color: [f32; 4],
    /// `[top highlight 0..1, bottom shade 0..1, thickness px, unused]`.
    pub bevel: [f32; 4],
    /// `[amount 0..1, pixel size, unused, unused]`.
    pub grain: [f32; 4],
    /// `[rotation in turns, unused, unused, unused]`.
    pub xform: [f32; 4],
    /// `[dash px, gap px, phase px, unused]` — a dash of 0 stays solid.
    pub dash: [f32; 4],
}

impl Default for UiRect {
    fn default() -> Self {
        bytemuck::Zeroable::zeroed()
    }
}

/// Constructors — the shapes the chrome starts from.
impl UiRect {
    fn base(v: Viewport, color: [f32; 4]) -> Self {
        Self {
            pos: [v.x, v.y],
            size: [v.w, v.h],
            color,
            ..Default::default()
        }
    }

    /// A hard-edged rect. Sharp fills skip antialiasing entirely so panels
    /// that tile edge to edge never show a half-covered hairline seam.
    pub fn region(v: Viewport, color: [f32; 4]) -> Self {
        Self::base(v, color)
    }

    pub fn region_rounded(v: Viewport, color: [f32; 4], radius: f32) -> Self {
        Self::base(v, color).radius(radius)
    }

    /// Rounded fill with a left→right gradient from `color` to `color2`.
    pub fn region_rounded_gradient(
        v: Viewport,
        color: [f32; 4],
        color2: [f32; 4],
        radius: f32,
    ) -> Self {
        Self::base(v, color).radius(radius).gradient(color2, 0.0)
    }

    pub fn icon(v: Viewport, kind: f32, thickness: f32, color: [f32; 4]) -> Self {
        Self {
            icon: [kind, thickness, 0.0, 0.0],
            ..Self::base(v, color)
        }
    }

    /// Like [`UiRect::icon`] with an explicit glyph radius factor (fraction
    /// of the quad's short side; the default is 0.20).
    pub fn icon_sized(
        v: Viewport,
        kind: f32,
        thickness: f32,
        color: [f32; 4],
        radius_factor: f32,
    ) -> Self {
        Self {
            icon: [kind, thickness, 0.0, radius_factor],
            ..Self::base(v, color)
        }
    }

    /// A round-capped line from `a` to `b` in window px. Diagonals at last —
    /// and it takes strokes, glows and gradients like anything else.
    pub fn line(a: [f32; 2], b: [f32; 2], thickness: f32, color: [f32; 4]) -> Self {
        let cap = thickness * 0.5;
        let v = Viewport {
            x: a[0].min(b[0]) - cap,
            y: a[1].min(b[1]) - cap,
            w: (a[0] - b[0]).abs() + thickness,
            h: (a[1] - b[1]).abs() + thickness,
        };
        let cx = v.x + v.w * 0.5;
        let cy = v.y + v.h * 0.5;
        Self {
            icon: [ICON_CAPSULE, cap, 0.0, 0.0],
            radii: [a[0] - cx, a[1] - cy, b[0] - cx, b[1] - cy],
            ..Self::base(v, color)
        }
    }

    /// A ring segment — what knobs, radial meters and circular progress are
    /// made of. `start` and `sweep` are in **turns**, measured from straight
    /// up and running clockwise, so a knob at 30% is
    /// `arc(v, 0.0, 0.3, ..)`; a `sweep` of 1.0 or more closes the ring.
    /// `radius_factor` is the ring's radius as a fraction of the quad's
    /// short side, and the band is `thickness` px wide with round caps.
    pub fn arc(
        v: Viewport,
        start: f32,
        sweep: f32,
        radius_factor: f32,
        thickness: f32,
        color: [f32; 4],
    ) -> Self {
        Self {
            icon: [ICON_ARC, thickness * 0.5, 0.0, radius_factor],
            radii: [start, sweep, 0.0, 0.0],
            ..Self::base(v, color)
        }
    }

    /// A closed ring — an arc all the way round. The track a knob's value
    /// arc rides on.
    pub fn ring(v: Viewport, radius_factor: f32, thickness: f32, color: [f32; 4]) -> Self {
        Self::arc(v, 0.0, 1.0, radius_factor, thickness, color)
    }

    /// A chevron pointing down. Aim it with [`UiRect::rotate`]: a quarter
    /// turn points it left, a half turn up, three quarters right.
    pub fn chevron(v: Viewport, thickness: f32, color: [f32; 4], radius_factor: f32) -> Self {
        Self::icon_sized(v, ICON_CHEVRON, thickness, color, radius_factor)
    }
}

/// Shape — corners, and the endpoints capsules borrow them for.
impl UiRect {
    /// One radius on all four corners.
    pub fn radius(mut self, r: f32) -> Self {
        self.radii = [r; 4];
        self
    }

    /// Per-corner radii, clockwise from the top-left.
    pub fn corners(mut self, tl: f32, tr: f32, br: f32, bl: f32) -> Self {
        self.radii = [tl, tr, br, bl];
        self
    }

    /// Round the top two corners only — tabs, and plates that sit on a seam.
    pub fn top_round(self, r: f32) -> Self {
        self.corners(r, r, 0.0, 0.0)
    }

    /// Round the bottom two corners only.
    pub fn bottom_round(self, r: f32) -> Self {
        self.corners(0.0, 0.0, r, r)
    }

    /// Round the left two corners only — the head of a segmented row.
    pub fn left_round(self, r: f32) -> Self {
        self.corners(r, 0.0, 0.0, r)
    }

    /// Round the right two corners only — its tail.
    pub fn right_round(self, r: f32) -> Self {
        self.corners(0.0, r, r, 0.0)
    }

    /// A stadium: fully rounded ends, whatever the size.
    pub fn pill(self) -> Self {
        let r = self.size[0].min(self.size[1]) * 0.5;
        self.radius(r)
    }

    /// Spin the silhouette about the quad's center, in turns, clockwise.
    /// Everything derived from the shape — stroke, bevel, shadow — turns
    /// with it. The quad does *not* grow, so leave a rotated element room.
    /// Ignored by capsules and arcs, which already carry their own angles.
    pub fn rotate(mut self, turns: f32) -> Self {
        self.xform[0] = turns;
        self
    }
}

/// Surface — how the face of the shape is painted.
impl UiRect {
    /// Linear gradient from `color` to `to`. `turns` is the direction:
    /// `0.0` left→right, [`TURN`] top→bottom, `0.5` right→left.
    pub fn gradient(mut self, to: [f32; 4], turns: f32) -> Self {
        self.color2 = to;
        self.grad = [1.0, turns, GRAD_LINEAR, 0.0];
        self
    }

    /// Top→bottom, the direction that reads as a lit surface.
    pub fn gradient_v(self, to: [f32; 4]) -> Self {
        self.gradient(to, TURN)
    }

    /// Left→right.
    pub fn gradient_h(self, to: [f32; 4]) -> Self {
        self.gradient(to, 0.0)
    }

    /// Confine the blend to a band of the surface: before `start` is the
    /// fill, after `end` is the far colour. `0.0, 1.0` is the whole surface,
    /// which is what an unset span already does.
    pub fn gradient_span(mut self, start: f32, end: f32) -> Self {
        self.grad_span = [start, end, 1.0, 0.0];
        self
    }

    /// Center → corners.
    pub fn gradient_radial(mut self, to: [f32; 4]) -> Self {
        self.color2 = to;
        self.grad = [1.0, 0.0, GRAD_RADIAL, 0.0];
        self
    }

    /// Per-pixel noise over the fill, screen-locked so it reads as the
    /// surface's own tooth rather than something sliding around on it.
    /// `amount` is a fraction of the fill's brightness; `px` is the noise
    /// cell size (1.0 = per pixel, 2.0 = chunkier).
    pub fn grain(mut self, amount: f32, px: f32) -> Self {
        self.grain = [amount, px.max(1.0), 0.0, 0.0];
        self
    }
}

/// Edges — the border, and the light along it.
impl UiRect {
    /// A real border, `width` px thick, inset so it never grows the shape.
    pub fn stroke(mut self, width: f32, color: [f32; 4]) -> Self {
        self.edge = [width, 0.0, 0.0, 0.0];
        self.edge_color = color;
        self
    }

    /// A ring sitting just *outside* the edge — a selection halo that marks
    /// a thing without eating into it. The quad grows to fit.
    pub fn stroke_outer(mut self, width: f32, color: [f32; 4]) -> Self {
        self.edge = [width, 1.0, 0.0, 0.0];
        self.edge_color = color;
        self
    }

    /// A border straddling the edge, half in and half out.
    pub fn stroke_center(mut self, width: f32, color: [f32; 4]) -> Self {
        self.edge = [width, 0.5, 0.0, 0.0];
        self.edge_color = color;
        self
    }

    /// Break the edge into dashes of `on` px separated by `off` px, walking
    /// the outline from its top-left. Slide `phase` frame over frame and
    /// you have marching ants.
    ///
    /// Lines and arcs have no interior to speak of, so on those the dashes
    /// break the *shape itself* — a dashed leader line, a segmented meter —
    /// which is what a dashed line has always meant.
    pub fn dash(mut self, on: f32, off: f32, phase: f32) -> Self {
        self.dash = [on, off, phase, 0.0];
        self
    }

    /// Rim lighting: `top` brightens up-facing edges, `bottom` darkens
    /// down-facing ones, both fading out `thickness` px into the shape. It
    /// follows the real distance field, so it curves around every corner.
    pub fn bevel(mut self, top: f32, bottom: f32, thickness: f32) -> Self {
        self.bevel = [top, bottom, thickness, 0.0];
        self
    }
}

/// Depth — what the shape casts, and what it holds.
impl UiRect {
    /// A drop shadow. `offset` moves it, `blur` is how far it reaches,
    /// `spread` fattens it before the blur.
    pub fn shadow(mut self, offset: [f32; 2], blur: f32, spread: f32, color: [f32; 4]) -> Self {
        self.outer = [offset[0], offset[1], blur, spread];
        self.outer_color = color;
        self
    }

    /// A shadow with no offset — i.e. a halo. Give it an accent color and
    /// the widget glows.
    pub fn glow(self, blur: f32, color: [f32; 4]) -> Self {
        self.shadow([0.0, 0.0], blur, 0.0, color)
    }

    /// Shadow carved *inward* from the edge: the shape reads as a hole.
    pub fn inner_shadow(
        mut self,
        offset: [f32; 2],
        blur: f32,
        spread: f32,
        color: [f32; 4],
    ) -> Self {
        self.inner = [offset[0], offset[1], blur, spread];
        self.inner_color = color;
        self
    }

    /// An inner halo, hugging the whole rim from inside.
    pub fn inner_glow(self, blur: f32, color: [f32; 4]) -> Self {
        self.inner_shadow([0.0, 0.0], blur, 0.0, color)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The storage-buffer element stride has to stay 16-byte aligned or the
    /// shader reads garbage for every instance after the first.
    ///
    /// The exact size is a receipt rather than a rule: it moves by 16 every
    /// time a `vec4` is added, and it is here so that adding one is a
    /// deliberate act. Seventeen fields as of the gradient span.
    #[test]
    fn instance_stride_is_std430_safe() {
        assert_eq!(size_of::<UiRect>() % 16, 0);
        assert_eq!(size_of::<UiRect>(), 16 * 17);
    }

    #[test]
    fn defaults_are_all_off() {
        let v = Viewport {
            x: 0.0,
            y: 0.0,
            w: 10.0,
            h: 10.0,
        };
        let r = UiRect::region(v, [1.0; 4]);
        assert_eq!(r.grad[0], 0.0, "gradient off");
        assert_eq!(r.edge[0], 0.0, "no stroke");
        assert_eq!(r.outer_color[3], 0.0, "no shadow");
        assert_eq!(r.radii, [0.0; 4], "sharp");
    }

    #[test]
    fn line_brackets_its_endpoints() {
        let r = UiRect::line([10.0, 20.0], [40.0, 60.0], 4.0, [1.0; 4]);
        assert_eq!(r.pos, [8.0, 18.0]);
        assert_eq!(r.size, [34.0, 44.0]);
        // Endpoints are stored relative to the quad's center.
        let cx = r.pos[0] + r.size[0] * 0.5;
        let cy = r.pos[1] + r.size[1] * 0.5;
        assert_eq!([r.radii[0] + cx, r.radii[1] + cy], [10.0, 20.0]);
        assert_eq!([r.radii[2] + cx, r.radii[3] + cy], [40.0, 60.0]);
    }

    #[test]
    fn pill_radius_is_the_short_side() {
        let v = Viewport {
            x: 0.0,
            y: 0.0,
            w: 100.0,
            h: 30.0,
        };
        assert_eq!(UiRect::region(v, [1.0; 4]).pill().radii, [15.0; 4]);
    }
}
