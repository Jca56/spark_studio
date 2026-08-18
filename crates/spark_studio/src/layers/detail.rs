//! The cog-expanded half of a layer card: the sliders, the segmented
//! toggles, and the gradient chips.
//!
//! Which controls appear is decided entirely by asking the shape what it
//! has — `density()` is `None` off a star field, `outline()` is `None` on
//! one — so a new kind brings its own controls along and no list here has
//! to be kept in sync with a `match` on kinds.

use spark_render::{Shape, Viewport};
use spark_ui::Segmented;

use crate::anim::prop_bit;
use crate::editor::Prop;
use crate::props::range;

use super::{CHIPS_H, CardDetail, ChoiceRow, SLIDER_H, SliderRow, TOGGLE_H, ToggleRow};

/// The cog-expanded settings block, advancing `cy` as it lays out.
#[allow(clippy::too_many_arguments)]
pub(super) fn detail(
    shape: &Shape,
    fx: &crate::fx::Stack,
    fx_keyed: &dyn Fn(u32, u8) -> bool,
    picking: bool,
    inner_x: f32,
    inner_w: f32,
    scale: f32,
    km: u16,
    cy: &mut f32,
) -> CardDetail {
    let mut sliders = Vec::new();
    let mut push = |prop: Prop, label: &'static str, v: f32, value: String, cy: &mut f32| {
        let (min, max) = range(prop);
        sliders.push(SliderRow {
            prop,
            label,
            label_pos: [inner_x, *cy],
            track: Viewport {
                x: inner_x,
                y: *cy + 30.0 * scale,
                w: inner_w,
                h: 10.0 * scale,
            },
            t: ((v - min) / (max - min)).clamp(0.0, 1.0),
            value,
            keyed: km & prop_bit(prop) != 0,
        });
        *cy += SLIDER_H * scale;
    };
    *cy += 4.0 * scale;
    if let Some([w, h]) = shape.box_size() {
        push(Prop::Width, "Width", w, format!("{w:.0}"), cy);
        push(Prop::Height, "Height", h, format!("{h:.0}"), cy);
    }
    if let Some(n) = shape.density() {
        push(Prop::Density, "Density", n, format!("{n:.0}"), cy);
    }
    // Only what this layer actually has. An effect you never added has no
    // row, which is the entire point of effects being a list rather than a
    // permanent set of fields — a Glow slider parked at 0 on every shape
    // forever is clutter that also quietly makes everything able to glow.
    if let Some(e) = fx.active(crate::fx::EffectKind::Glow) {
        let glow = e.get(0);
        push(Prop::Glow, "Glow", glow, format!("{glow:.0}"), cy);
    }
    let br = shape.brightness();
    push(Prop::Brightness, "Brightness", br, format!("{br:.1}"), cy);
    if let Some(sides) = shape.sides() {
        push(Prop::Sides, "Sides", sides as f32, format!("{sides}"), cy);
    }
    if let Some(th) = shape.thickness() {
        // On a field that number is how big one star is, not how thick a
        // stroke is — the label has to say the thing you're looking at.
        let label = if shape.is_stars() {
            "Star size"
        } else {
            "Thickness"
        };
        push(Prop::Thickness, label, th, format!("{th:.1}"), cy);
    }
    if let Some(tw) = shape.twinkle() {
        push(Prop::Twinkle, "Twinkle", tw, format!("{tw:.2}"), cy);
    }
    if let Some(rate) = shape.twinkle_rate() {
        push(
            Prop::TwinkleRate,
            "Twinkle speed",
            rate,
            format!("{rate:.1}"),
            cy,
        );
    }
    if let Some(seed) = shape.seed() {
        push(Prop::Seed, "Seed", seed, format!("{seed:.0}"), cy);
    }
    let toggle = |cy: &mut f32, on: bool| {
        let t = ToggleRow {
            label_pos: [inner_x, *cy],
            seg: Segmented::new(
                Viewport {
                    x: inner_x,
                    y: *cy + 32.0 * scale,
                    w: inner_w,
                    h: 44.0 * scale,
                },
                2,
                scale,
            ),
            on,
        };
        *cy += TOGGLE_H * scale;
        t
    };
    let form = shape.star_form().map(|active| {
        let row = ChoiceRow {
            label_pos: [inner_x, *cy],
            seg: Segmented::new(
                Viewport {
                    x: inner_x,
                    y: *cy + 32.0 * scale,
                    w: inner_w,
                    h: 44.0 * scale,
                },
                spark_render::STAR_FORMS.len(),
                scale,
            ),
            active,
            options: &spark_render::STAR_FORMS,
        };
        *cy += TOGGLE_H * scale;
        row
    });
    let style = shape.outline().map(|o| toggle(cy, o));
    let blend = toggle(cy, shape.additive());
    let grad = toggle(cy, shape.gradient());
    let chips = shape.gradient().then(|| {
        let side = 40.0 * scale;
        let chips = [
            Viewport {
                x: inner_x,
                y: *cy,
                w: side,
                h: side,
            },
            Viewport {
                x: inner_x + side + 10.0 * scale,
                y: *cy,
                w: side,
                h: side,
            },
        ];
        *cy += CHIPS_H * scale;
        chips
    });
    // The effects last: the shape's own settings say what it is, and
    // everything below the header is what you chose to add to it.
    let fx_block = super::effects::block(fx, fx_keyed, inner_x, inner_w, scale, picking, cy);
    CardDetail {
        sliders,
        form,
        style,
        blend,
        grad,
        chips,
        rgb2: shape.rgb2(),
        fx: fx_block,
    }
}
