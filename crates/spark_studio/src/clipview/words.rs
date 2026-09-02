//! The clip view's words and numbers: what a setting is called (the
//! inspector's own word for it), which settings an object can key and
//! in what order, how a value prints, and the value range a curve
//! stands on. Split from `page` so the layout stays inside the file
//! budget; nothing here knows about pixels.

use spark_render::Shape;

use crate::anim::{Key, Target, keyable, prop_value};
use crate::fx::Stack;
use crate::inspector::{ROWS, fmt_number, fmt_param, is_angle, style_specs};
use crate::props::Prop;

/// A local time as a musician reads it: `Bar 2.3`, one-based.
pub fn beat_label(t: f32, bpm: f32) -> String {
    let beat_s = 60.0 / bpm.max(1.0);
    let beats = (t.max(0.0) / beat_s + 1e-4).floor() as i64;
    format!("Bar {}.{}", beats / 4 + 1, beats % 4 + 1)
}

/// A setting's value as the object stands — what a row shows before it
/// has keys, and what a first key is planted at.
pub fn current_value(shape: &Shape, fx: &Stack, target: Target) -> Option<f32> {
    match target {
        Target::Shape(p) => prop_value(shape, p),
        Target::Effect { id, param } => fx.find(id).map(|e| e.get(param as usize)),
    }
}

/// Every setting the object can key, in the order the inspector shows
/// them and wearing the inspector's words: the transform strip's rows
/// (a line's `X1 Y1`, `X2 Y2`; `X Y Z`, `Tilt Turn Rot`, `S W H`, `D`),
/// the Style sliders, then each effect's parameters (a one-parameter
/// effect is just its name — `Glow`; otherwise `React · Scale`). A
/// setting the object lacks is left out, the way the inspector leaves
/// it out — and so is one it has but can't key: a line's centre, a
/// light's spin (`anim::keyable`, the stamp's own rule).
pub fn keyable_targets(shape: &Shape, fx: &Stack) -> Vec<(Target, String)> {
    let mut out = Vec::new();
    for row in ROWS {
        for &(p, cap) in row.iter() {
            if keyable(shape, p) {
                out.push((Target::Shape(p), cap.to_string()));
            }
        }
    }
    for (p, name) in style_specs(shape) {
        // Glow is the Glow effect's parameter; it lists with the effects.
        if p != Prop::Glow && keyable(shape, p) {
            out.push((Target::Shape(p), name.to_string()));
        }
    }
    for e in &fx.effects {
        let specs = e.kind.params();
        for (k, spec) in specs.iter().enumerate() {
            let label = if specs.len() == 1 {
                e.kind.label().to_string()
            } else {
                format!("{} · {}", e.kind.label(), spec.name)
            };
            out.push((
                Target::Effect {
                    id: e.id,
                    param: k as u8,
                },
                label,
            ));
        }
    }
    out
}

/// What a target is called: the inspector's word for it on this object.
pub fn target_label(target: Target, shape: &Shape, fx: &Stack) -> String {
    keyable_targets(shape, fx)
        .into_iter()
        .find(|(t, _)| *t == target)
        .map(|(_, l)| l)
        .unwrap_or_else(|| match target {
            Target::Shape(p) => prop_name(p).to_string(),
            Target::Effect { id, param } => format!("effect {id}·{param}"),
        })
}

/// The inspector's word for a property, for a target its object no
/// longer carries.
pub fn prop_name(p: Prop) -> &'static str {
    match p {
        Prop::X => "X",
        Prop::Y => "Y",
        Prop::X1 => "X1",
        Prop::Y1 => "Y1",
        Prop::X2 => "X2",
        Prop::Y2 => "Y2",
        Prop::Z => "Z",
        Prop::Rotation => "Rot",
        Prop::Tilt => "Tilt",
        Prop::Turn => "Turn",
        Prop::Scale => "S",
        Prop::Width => "W",
        Prop::Height => "H",
        Prop::Glow => "Glow",
        Prop::Brightness => "Brightness",
        Prop::Opacity => "Opacity",
        Prop::Sides => "Sides",
        Prop::Thickness => "Thickness",
        Prop::Cone => "Cone",
        Prop::Rim => "Rim",
        Prop::Depth => "D",
        Prop::Density => "Density",
        Prop::Twinkle => "Twinkle",
        Prop::TwinkleRate => "Rate",
        Prop::Seed => "Seed",
        Prop::Jag => "Jag",
        Prop::Branches => "Forks",
        Prop::Strike => "Strike",
        Prop::Hole => "Hole",
        Prop::Twist => "Twist",
        Prop::Spin => "Spin",
        Prop::Grain => "Grain",
    }
}

/// A target's value the way the inspector would print it: angles in
/// degrees, a size as the full extent the S field speaks, an effect
/// parameter to its own precision.
pub fn fmt_target(target: Target, v: f32, fx: &Stack, canvas: [f32; 2], is_light: bool) -> String {
    match target {
        Target::Shape(p) if is_angle(p) => format!("{}°", fmt_number(v.to_degrees())),
        Target::Shape(Prop::Scale) => fmt_number(if is_light { v } else { v * 2.0 }),
        Target::Shape(p) => {
            let (lo, hi) = crate::props::range(p, canvas);
            if hi - lo <= 5.0 {
                format!("{v:.2}")
            } else {
                fmt_number(v)
            }
        }
        Target::Effect { id, param } => fx
            .find(id)
            .and_then(|e| e.kind.params().get(param as usize))
            .map(|spec| fmt_param(v, spec))
            .unwrap_or_else(|| fmt_number(v)),
    }
}

/// A number typed in the inspector's units, as the curve stores it: an
/// angle comes in degrees, a size as the full extent the S field speaks
/// (a light's range as it is), everything else as itself.
pub fn typed_value(target: Target, typed: f32, is_light: bool) -> f32 {
    match target {
        Target::Shape(p) if is_angle(p) => typed.to_radians(),
        Target::Shape(Prop::Scale) if !is_light => typed * 0.5,
        _ => typed,
    }
}

/// Whether a property's range is a real ceiling and floor (the graph can
/// stand on it) rather than a slider's reach.
pub(super) fn bounded(p: Prop) -> bool {
    !matches!(
        p,
        Prop::Rotation
            | Prop::Tilt
            | Prop::Turn
            | Prop::Scale
            | Prop::Width
            | Prop::Height
            | Prop::Depth
            | Prop::Z
    )
}

/// A flat curve's window: how far either side of its one value the
/// graph opens, in the target's own units.
fn unit(target: Target, v: f32) -> f32 {
    match target {
        Target::Shape(p) if is_angle(p) => std::f32::consts::FRAC_PI_2,
        Target::Shape(Prop::Z) => 500.0,
        Target::Shape(_) => (v.abs() * 0.5).max(100.0),
        Target::Effect { .. } => 1.0,
    }
}

/// The value range the graph maps: a bounded property stands on its own
/// range (widened if a key sits outside it — X off the canvas), a free
/// one on its keys' reach with a quarter of air either side, and a flat
/// curve on a window around its value so it draws mid-graph.
pub fn value_span(target: Target, keys: &[Key], fx: &Stack, canvas: [f32; 2]) -> (f32, f32) {
    let (kmin, kmax) = keys
        .iter()
        .fold((f32::MAX, f32::MIN), |(a, b), k| (a.min(k.v), b.max(k.v)));
    let (kmin, kmax) = if keys.is_empty() {
        (0.0, 0.0)
    } else {
        (kmin, kmax)
    };
    let base = match target {
        Target::Shape(p) if bounded(p) => Some(crate::props::range(p, canvas)),
        Target::Shape(_) => None,
        Target::Effect { id, param } => fx
            .find(id)
            .and_then(|e| e.kind.params().get(param as usize))
            .map(|s| (s.min, s.max)),
    };
    let (lo, hi) = match base {
        Some((lo, hi)) => (lo.min(kmin), hi.max(kmax)),
        None => {
            let reach = kmax - kmin;
            if reach < 1e-4 {
                let d = unit(target, kmin);
                (kmin - d, kmax + d)
            } else {
                (kmin - reach * 0.25, kmax + reach * 0.25)
            }
        }
    };
    if hi - lo < 1e-6 {
        (lo - 1.0, hi + 1.0)
    } else {
        (lo, hi)
    }
}
