//! The inspector body's sections, laid out through the cursor: the
//! transform strip, Style (Light for a light) with Glow kept in it, and
//! one section per effect on the object with Enabled, its settings and
//! Remove. Split from `page` so the layout stays inside the file budget.

use spark_render::{LIGHT_KINDS, STAR_FORMS, Shape};

use super::build::Cursor;
use super::field;
use super::page::{
    ButtonKind, CheckKind, EditKey, SectionKey, SliderTarget, SwitchKind, fmt_param,
};
use crate::editor::Editor;
use crate::fx::EffectKind;
use crate::props::{Prop, Props};
use crate::textbox::TextBox;

/// The Style (or Light) section's sliders, in Alva's order — Sides,
/// Opacity, Brightness, Thickness, Glow, a field's sky after — with the
/// words they wear. Shared with the clip view, whose rows must call a
/// setting exactly what the inspector calls it.
pub fn style_specs(shape: &Shape) -> Vec<(Prop, &'static str)> {
    let mut specs: Vec<(Prop, &'static str)> = Vec::new();
    if shape.is_light() {
        specs.push((Prop::Brightness, "Intensity"));
        if shape.cone().is_some() {
            specs.push((Prop::Cone, "Cone"));
        }
        if shape.rim().is_some() {
            specs.push((Prop::Rim, "Rim"));
        }
    } else {
        if shape.sides().is_some() {
            specs.push((Prop::Sides, "Sides"));
        }
        specs.push((Prop::Opacity, "Opacity"));
        specs.push((Prop::Brightness, "Brightness"));
        if shape.thickness().is_some() {
            specs.push((
                Prop::Thickness,
                if shape.is_stars() { "Size" } else { "Thickness" },
            ));
        }
        if !shape.is_mesh() {
            specs.push((Prop::Glow, "Glow"));
        }
        if shape.is_stars() {
            specs.push((Prop::Density, "Density"));
            specs.push((Prop::Twinkle, "Twinkle"));
            specs.push((Prop::TwinkleRate, "Rate"));
        }
    }
    specs
}

/// Lay the body's sections out for object `i` (`shape` its working
/// copy, `props` its numbers), skipping the content of the ones folded.
pub(super) fn body(
    c: &mut Cursor,
    e: &Editor,
    i: usize,
    shape: &Shape,
    props: Option<&Props>,
    edit: Option<&(EditKey, TextBox)>,
    folded: &[SectionKey],
) {
    let canvas = e.canvas();
    let is_open = |key: SectionKey| !folded.contains(&key);

    // Transform: rows of three fields; a prop the shape lacks is left
    // out and the row closes up.
    if c.section(SectionKey::Transform, "Transform", is_open(SectionKey::Transform)) {
        let has = |prop: Prop| -> Option<f32> {
            let p = props?;
            match prop {
                Prop::X => Some(p.x),
                Prop::Y => Some(p.y),
                Prop::Z => Some(p.z),
                // A light is aimed, not spun; a line's angle is its ends'.
                Prop::Rotation => (!shape.is_light()).then_some(p.rotation),
                Prop::Tilt => Some(p.tilt),
                Prop::Turn => Some(p.turn),
                Prop::Scale => Some(p.size),
                Prop::Width => p.w,
                Prop::Height => p.h,
                Prop::Depth => p.d,
                _ => None,
            }
        };
        for row in field::ROWS {
            let present: Vec<(Prop, &'static str, f32)> = row
                .iter()
                .filter_map(|&(prop, cap)| has(prop).map(|v| (prop, cap, v)))
                .collect();
            c.field_row(&present, edit);
        }
    }
    c.end_section();

    // Style — or, for a light, Light: the kind's switch, its sliders in
    // Alva's order (Sides, Opacity, Brightness, Thickness, Glow, a
    // field's sky after), and Additive. Glow stays here by Alva's call:
    // the one effect so fundamental to a shape it is a setting.
    let style_title = if shape.is_light() { "Light" } else { "Style" };
    if c.section(SectionKey::Style, style_title, is_open(SectionKey::Style)) {
        let switch = if shape.is_light() {
            shape
                .light_kind()
                .map(|k| (SwitchKind::LightKind, &LIGHT_KINDS[..], k.index()))
        } else if shape.is_stars() {
            shape
                .star_form()
                .map(|f| (SwitchKind::StarForm, &STAR_FORMS[..], f))
        } else {
            shape.outline().map(|o| {
                (
                    SwitchKind::FillOutline,
                    &["Fill", "Outline"][..],
                    usize::from(o),
                )
            })
        };
        if let Some((kind, labels, active)) = switch {
            c.switch(kind, labels, active);
        }
        let glow = e
            .fx_of(i)
            .active(EffectKind::Glow)
            .map(|g| g.get(0))
            .unwrap_or(0.0);
        for (prop, label) in style_specs(shape) {
            let value = match prop {
                Prop::Glow => glow,
                p => crate::anim::prop_value(shape, p).unwrap_or(0.0),
            };
            c.slider(
                SliderTarget::Prop(prop),
                label,
                value,
                crate::props::range(prop, canvas),
                crate::defaults::readout(prop, value),
            );
        }
        if !shape.is_light() && !shape.is_mesh() {
            c.check(CheckKind::Additive, "Additive", shape.additive());
        }
    }
    c.end_section();

    // One section per effect on the object — Glow excepted, it lives in
    // Style: Enabled, its settings (a colour as a chip, the rest as
    // sliders), and Remove.
    for fx in &e.fx_of(i).effects {
        if fx.kind == EffectKind::Glow {
            continue;
        }
        let key = SectionKey::Effect(fx.kind);
        if c.section(key, fx.kind.label(), is_open(key)) {
            c.check(CheckKind::EffectOn(fx.id), "Enabled", fx.on);
            let colour = fx.kind.colour_param().map(|c| c as usize);
            for (k, spec) in fx.kind.params().iter().enumerate() {
                if let Some(c0) = colour
                    && (c0..c0 + 3).contains(&k)
                {
                    if k == c0 {
                        c.chip(
                            fx.id,
                            c0,
                            [fx.get(c0), fx.get(c0 + 1), fx.get(c0 + 2)],
                            "End colour",
                        );
                    }
                    continue;
                }
                let v = fx.get(k);
                c.slider(
                    SliderTarget::Effect {
                        id: fx.id,
                        param: k,
                    },
                    spec.name,
                    v,
                    (spec.min, spec.max),
                    fmt_param(v, spec),
                );
            }
            c.button(
                ButtonKind::RemoveEffect(fx.id),
                format!("Remove {}", fx.kind.label()),
            );
        }
        c.end_section();
    }
}
