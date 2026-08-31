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

use super::{
    CHECK_H, CHECK_SIDE, CardDetail, CardTab, CheckRow, ChoiceRow, SCRUB_H, SLIDER_H, ScrubField,
    SliderRow, TOGGLE_H, ToggleRow,
};

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
    km: u32,
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
            scrubs: Vec::new(),
            sliders: Vec::new(),
            form: None,
            light_kind: None,
            style: None,
            blend: None,
            fx: super::effects::block(fx, fx_keyed, inner_x, inner_w, scale, cy),
        };
    }
    *cy += 4.0 * scale;
    // Where the plane sits in the scene. Every shape has these — a comp is
    // a 3D world, and a shape that has never left the canvas is one whose
    // three are zero. Three boxes across: a fourth would squeeze the words
    // out of the labels.
    let mut scrubs = Vec::new();
    {
        let fields: [(Prop, &'static str, String); 3] = [
            (Prop::Z, "Z", format!("{:.0}", shape.z())),
            (Prop::Tilt, "Tilt", format!("{:.0}", shape.tilt().to_degrees())),
            (Prop::Turn, "Turn", format!("{:.0}", shape.turn().to_degrees())),
        ];
        let fgap = 6.0 * scale;
        let fw = (inner_w - fgap * 2.0) / 3.0;
        let lw = super::SPACE_LABEL_W * scale;
        for (k, (prop, label, value)) in fields.into_iter().enumerate() {
            let fx = inner_x + (fw + fgap) * k as f32;
            scrubs.push(ScrubField {
                prop,
                rect: Viewport {
                    x: fx + lw,
                    y: *cy,
                    w: (fw - lw).max(1.0),
                    h: SCRUB_H * scale,
                },
                label,
                label_pos: [fx, *cy],
                label_w: lw,
                value,
                keyed: km & prop_bit(prop) != 0,
            });
        }
        *cy += (SCRUB_H + 6.0) * scale;
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
    // On a light that number is how hard it shines.
    let label = if shape.is_light() { "Intensity" } else { "Brightness" };
    push(Prop::Brightness, label, br, format!("{br:.1}"), cy);
    // Read as a percentage: nobody thinks in 0.35 of a shape. A light has
    // nothing to be see-through: its intensity is its whole presence.
    if !shape.is_light() {
        let op = shape.opacity();
        push(
            Prop::Opacity,
            "Opacity",
            op,
            format!("{:.0}%", op * 100.0),
            cy,
        );
    }
    if let Some(cone) = shape.cone() {
        push(Prop::Cone, "Cone", cone, format!("{cone:.0}"), cy);
    }
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
    let choice = |cy: &mut f32, active: usize, options: &'static [&'static str]| {
        let row = ChoiceRow {
            label_pos: [inner_x, *cy],
            seg: Segmented::new(
                Viewport {
                    x: inner_x,
                    y: *cy + 32.0 * scale,
                    w: inner_w,
                    h: 44.0 * scale,
                },
                options.len(),
                scale,
            ),
            active,
            options,
        };
        *cy += TOGGLE_H * scale;
        row
    };
    let light_kind = shape
        .light_kind()
        .map(|k| choice(cy, k.index(), &spark_render::LIGHT_KINDS));
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
    // Gradient's Off/On pair and its endpoint chips used to live here. They
    // were dead controls: the Gradient *effect* writes the shape's gradient
    // flag and end colour every frame in `fx::resolve`, so whatever these
    // set was overwritten before it reached the screen. The colour now
    // lives on the effect's own card, where the thing that owns it is.
    // A mesh is solid, and a light already is pure light: neither can be
    // made additive.
    let blend = (!shape.is_mesh() && !shape.is_light()).then(|| CheckRow {
        label: "Additive",
        check: spark_ui::Checkbox::new(inner_x, *cy, inner_w, CHECK_SIDE * scale, scale),
        on: shape.additive(),
    });
    if blend.is_some() {
        *cy += CHECK_H * scale;
    }
    CardDetail {
        // Filled in by the caller, which owns the block's extents.
        panel: Viewport {
            x: 0.0,
            y: 0.0,
            w: 0.0,
            h: 0.0,
        },
        tab,
        scrubs,
        sliders,
        form,
        light_kind,
        style,
        blend,
        fx: Vec::new(),
    }
}
