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

use super::{CHIPS_H, CardDetail, CardTab, ChoiceRow, SLIDER_H, SliderRow, TOGGLE_H, ToggleRow};

/// The cog-expanded settings block, advancing `cy` as it lays out.
#[allow(clippy::too_many_arguments)]
pub(super) fn detail(
    shape: &Shape,
    fx: &crate::fx::Stack,
    fx_keyed: &dyn Fn(u32, u8) -> bool,
    tab: CardTab,
    inner_x: f32,
    inner_w: f32,
    scale: f32,
    km: u16,
    cy: &mut f32,
) -> CardDetail {
    // The Effects tab is the other half of the card entirely: what you
    // added to this layer, not what it is.
    if tab == CardTab::Effects {
        return CardDetail {
            tab,
            // Filled in by the caller, which owns the block's extents.
            panel: Viewport {
                x: 0.0,
                y: 0.0,
                w: 0.0,
                h: 0.0,
            },
            sliders: Vec::new(),
            form: None,
            style: None,
            blend: None,
            grad: None,
            chips: None,
            rgb2: shape.rgb2(),
            fx: super::effects::block(fx, fx_keyed, inner_x, inner_w, scale, cy),
        };
    }
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
                w: (inner_w - (super::VALUE_W + super::VALUE_GAP) * scale).max(1.0),
                h: 10.0 * scale,
            },
            t: ((v - min) / (max - min)).clamp(0.0, 1.0),
            value,
            value_right: inner_x + inner_w,
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
    let br = shape.brightness();
    push(Prop::Brightness, "Brightness", br, format!("{br:.1}"), cy);
    // Read as a percentage: nobody thinks in 0.35 of a shape.
    let op = shape.opacity();
    push(
        Prop::Opacity,
        "Opacity",
        op,
        format!("{:.0}%", op * 100.0),
        cy,
    );
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
    let blend = Some(toggle(cy, shape.additive()));
    let grad = Some(toggle(cy, shape.gradient()));
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
    CardDetail {
        // Filled in by the caller, which owns the block's extents.
        panel: Viewport {
            x: 0.0,
            y: 0.0,
            w: 0.0,
            h: 0.0,
        },
        tab,
        sliders,
        form,
        style,
        blend,
        grad,
        chips,
        rgb2: shape.rgb2(),
        fx: Vec::new(),
    }
}
