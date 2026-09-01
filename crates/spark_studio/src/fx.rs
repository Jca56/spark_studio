//! Effects: the modular half of what a layer looks like.
//!
//! A shape carries only what it *is* — where it sits, how big, what colour.
//! Everything you might optionally want it to do is an effect you add. Glow
//! is the case that proved this: it used to be a permanent field floored
//! above zero, so no shape could stop emitting light and "everything is
//! neon" became structural rather than chosen. A setting that is always
//! present is a decision already made for you.
//!
//! An effect is a **kind** plus a flat list of parameter values. Kinds
//! declare their parameters in a static table, so adding one is a table
//! entry rather than a new struct field on every layer — the same shape as
//! `materials::SLOTS`, which names the editor's colours the same way.
//!
//! Effects carry **stable ids**, not stack positions. Keyframes and drivers
//! address `Effect { id, param }`, so reordering the stack can't silently
//! repoint a curve at a different effect — the same lesson shape ids
//! already taught (see `editor::Editor::ids`).

/// One tunable number on an effect.
pub struct ParamSpec {
    /// What it's called on screen. Read by the effect browser and the card's
    /// stack rows, which land next — the table is the single place these
    /// names live, so it declares them before anything renders them.
    #[allow(dead_code)]
    pub name: &'static str,
    pub min: f32,
    pub max: f32,
    /// What it reads when the effect is first added.
    pub default: f32,
    /// What [`resolve`] behaves as when the effect *isn't* there.
    ///
    /// Not the same as `default`, and not always `min`: adding a Glow reads
    /// 30, but a layer without one glows by 0. This is the value a stamp
    /// treats as the parameter's history — add glow at bar 5 and press `K`,
    /// and the backfilled holding key at bar 1 carries *this*, so the glow
    /// ramps up from nothing instead of appearing flat.
    pub absent: f32,
}

const fn p(name: &'static str, min: f32, max: f32, default: f32, absent: f32) -> ParamSpec {
    ParamSpec {
        name,
        min,
        max,
        default,
        absent,
    }
}

/// What an effect does. Every kind here resolves into the shapes handed to
/// the renderer each frame — the document is never mutated by drawing.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum EffectKind {
    /// A halo outside the silhouette. Was a permanent shape field.
    Glow,
    /// A second colour across the shape, mode chosen by kind.
    Gradient,
    /// Audio reaction: how hard the shape rides the track — bass into
    /// size and glow, mids and onsets into brightness. Was three amounts
    /// on every object, at 1.0 from birth, so everything wobbled whether
    /// asked to or not (Alva, 2026-08-31: "remove the React effect from
    /// being on every single object ever by default. Please."). An
    /// effect now: absent until added, keyable like any other.
    React,
}

/// Every effect the editor can add, in the order the browser lists them.
///
/// Two kinds left this list on 2026-08-18, for opposite reasons. **Brightness**
/// was never wired into [`resolve`] at all: it listed, added, and did
/// nothing, while the shape's own brightness slider did the work. A control
/// that changes nothing is worse than a missing one, because you spend the
/// session wondering what you did wrong. **Additive** had the mirror
/// problem — the effect worked and the shape's own toggle was overwritten by
/// the resolver every frame, so the setting on the card was dead. It is a
/// checkbox on the card now: one on/off switch, in the place you were
/// already looking for it, and nothing about "pure light instead of
/// occluding" needs a parameter, a stack position, or a curve.
pub const KINDS: [EffectKind; 3] = [EffectKind::Glow, EffectKind::Gradient, EffectKind::React];

const GLOW: [ParamSpec; 1] = [p("Radius", 0.0, 200.0, 30.0, 0.0)];
// Added, it is the classic wobble (every amount at 1); absent, nothing
// moves.
const REACT: [ParamSpec; 3] = [
    p("Scale", 0.0, 2.0, 1.0, 0.0),
    p("Glow", 0.0, 2.0, 1.0, 0.0),
    p("Brightness", 0.0, 2.0, 1.0, 0.0),
];
// Colour as three linear channels: a parameter list is flat floats, so the
// colour home writes all three at once and one keyframe track type covers
// every parameter there is.
const GRADIENT: [ParamSpec; 3] = [
    p("End red", 0.0, 1.0, 0.0, 0.0),
    p("End green", 0.0, 1.0, 0.0, 0.0),
    p("End blue", 0.0, 1.0, 0.0, 0.0),
];
impl EffectKind {
    /// What it's called on screen. See [`ParamSpec::name`] on why this is
    /// declared ahead of the UI that reads it.
    #[allow(dead_code)]
    pub fn label(self) -> &'static str {
        match self {
            EffectKind::Glow => "Glow",
            EffectKind::Gradient => "Gradient",
            EffectKind::React => "React",
        }
    }

    /// One short line on what it does, for the effects browser's rows.
    pub fn blurb(self) -> &'static str {
        match self {
            EffectKind::Glow => "A halo outside the shape.",
            EffectKind::Gradient => "Fade to the background colour.",
            EffectKind::React => "Ride the track's bass and mids.",
        }
    }

    /// The tag it serializes under. Short, stable, and never reused for a
    /// different kind — a comp written today has to open in a year.
    pub fn tag(self) -> &'static str {
        match self {
            EffectKind::Glow => "glow",
            EffectKind::Gradient => "grad",
            EffectKind::React => "react",
        }
    }

    pub fn from_tag(tag: &str) -> Option<Self> {
        KINDS.into_iter().find(|k| k.tag() == tag)
    }

    /// The first of three consecutive parameters that are one linear RGB
    /// colour, where the kind has such a triple.
    ///
    /// A colour is three numbers only because a parameter list is flat
    /// floats. The card draws a swatch you click — which routes the colour
    /// home at it, the same picker everything else in the editor uses —
    /// rather than three sliders: nobody chooses a colour by dragging its
    /// channels apart from each other.
    pub fn colour_param(self) -> Option<u8> {
        match self {
            EffectKind::Gradient => Some(0),
            _ => None,
        }
    }

    /// Its parameters, in the order `Effect::params` stores them.
    pub fn params(self) -> &'static [ParamSpec] {
        match self {
            EffectKind::Glow => &GLOW,
            EffectKind::Gradient => &GRADIENT,
            EffectKind::React => &REACT,
        }
    }

    /// What a parameter reads when the layer hasn't got this effect.
    pub fn absent(self, param: usize) -> f32 {
        self.params().get(param).map(|s| s.absent).unwrap_or(0.0)
    }

    /// Only one of these makes sense per layer — adding a second Glow would
    /// silently override the first rather than compounding.
    pub fn unique(self) -> bool {
        true
    }
}

/// One effect on one layer.
#[derive(Clone, PartialEq, Debug)]
pub struct Effect {
    /// Stable within the layer; what keyframes and drivers address.
    pub id: u32,
    pub kind: EffectKind,
    /// Turned off keeps its settings — an effect you're auditioning should
    /// be one click away from back on, not gone.
    pub on: bool,
    /// Values, positionally matching `kind.params()`.
    pub params: Vec<f32>,
}

impl Effect {
    /// A fresh effect at its defaults.
    pub fn new(id: u32, kind: EffectKind) -> Self {
        Self {
            id,
            kind,
            on: true,
            params: kind.params().iter().map(|s| s.default).collect(),
        }
    }

    pub fn get(&self, i: usize) -> f32 {
        let specs = self.kind.params();
        self.params
            .get(i)
            .copied()
            .unwrap_or_else(|| specs.get(i).map(|s| s.default).unwrap_or(0.0))
    }

    pub fn set(&mut self, i: usize, v: f32) {
        let specs = self.kind.params();
        let Some(spec) = specs.get(i) else { return };
        // Params written from disk or by a driver still have to land in
        // range: an effect is a promise about what its numbers mean.
        if self.params.len() < specs.len() {
            self.params.resize(specs.len(), 0.0);
        }
        self.params[i] = v.clamp(spec.min, spec.max);
    }
}

/// A layer's effect stack, innermost first.
#[derive(Clone, PartialEq, Debug, Default)]
pub struct Stack {
    pub effects: Vec<Effect>,
}

impl Stack {
    pub fn find(&self, id: u32) -> Option<&Effect> {
        self.effects.iter().find(|e| e.id == id)
    }

    pub fn find_mut(&mut self, id: u32) -> Option<&mut Effect> {
        self.effects.iter_mut().find(|e| e.id == id)
    }

    /// The effect of a kind on this layer, on or off. `active` is the one
    /// that also has to be switched on; this is "does the layer carry it".
    pub fn find_kind(&self, kind: EffectKind) -> Option<&Effect> {
        self.effects.iter().find(|e| e.kind == kind)
    }

    /// The live effect of a kind, if the layer has one turned on.
    pub fn active(&self, kind: EffectKind) -> Option<&Effect> {
        self.effects.iter().find(|e| e.kind == kind && e.on)
    }

    /// Add an effect, returning its id. Kinds marked unique replace rather
    /// than stack — a second Glow would override the first, which reads as
    /// the effect not working.
    pub fn add(&mut self, kind: EffectKind, id: u32) -> u32 {
        if kind.unique()
            && let Some(e) = self.effects.iter_mut().find(|e| e.kind == kind)
        {
            // Already there: turn it back on rather than adding a twin.
            e.on = true;
            return e.id;
        }
        self.effects.push(Effect::new(id, kind));
        id
    }

    /// Take an effect off the layer. The only way one leaves the stack —
    /// setting a parameter to zero holds it at zero, it doesn't delete it.
    /// Wired to the stack row's remove button, which lands next.
    #[allow(dead_code)]
    pub fn remove(&mut self, id: u32) -> bool {
        let n = self.effects.len();
        self.effects.retain(|e| e.id != id);
        self.effects.len() != n
    }

    /// The next id free in this stack. Ids are per-layer, so a duplicated
    /// layer keeps its curves pointing at its own copies.
    pub fn next_id(&self) -> u32 {
        self.effects.iter().map(|e| e.id).max().unwrap_or(0) + 1
    }
}

/// How hard a layer rides the track — its React effect's amounts, or
/// nothing at all without one. Read at scene time off the display stack,
/// so a keyed React breathes with the curves.
pub fn react_of(stack: &Stack) -> [f32; 3] {
    stack
        .active(EffectKind::React)
        .map(|e| [e.get(0), e.get(1), e.get(2)])
        .unwrap_or([0.0; 3])
}

/// Paint a layer's effects onto the copy of its shape being drawn.
///
/// The stack is the source of truth for everything it controls: an absent
/// Glow means glow zero, not "whatever the shape happened to store". That
/// is the whole point — a look you didn't ask for cannot leak in from a
/// field nobody can see. The document is never mutated; this runs on the
/// display copy, so `frame = render(project, t)` still holds.
pub fn resolve(shape: &mut spark_render::Shape, stack: &Stack) {
    shape.set_glow(
        stack
            .active(EffectKind::Glow)
            .map(|e| e.get(0))
            .unwrap_or(0.0),
    );
    match stack.active(EffectKind::Gradient) {
        Some(e) => {
            shape.set_gradient(true);
            shape.set_rgb2([e.get(0), e.get(1), e.get(2)]);
        }
        None => shape.set_gradient(false),
    }
    // Additive is deliberately absent: it is the shape's own field, set by
    // the checkbox on the card. The resolver used to write it every frame
    // from an effect, which is precisely what made that checkbox dead.
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every kind has to round-trip through its serialization tag, or a
    /// saved comp silently loses effects on load.
    #[test]
    fn every_kind_round_trips_through_its_tag() {
        for k in KINDS {
            assert_eq!(EffectKind::from_tag(k.tag()), Some(k), "{k:?}");
        }
        assert_eq!(EffectKind::from_tag("nonsense"), None);
    }

    /// Tags are the on-disk contract: two kinds sharing one would make a
    /// comp load as the wrong effect.
    #[test]
    fn tags_and_labels_are_unique() {
        for (i, k) in KINDS.iter().enumerate() {
            for other in &KINDS[..i] {
                assert_ne!(k.tag(), other.tag(), "{k:?} and {other:?} share a tag");
                assert_ne!(k.label(), other.label(), "{k:?} and {other:?} share a name");
            }
        }
    }

    /// A fresh effect reads its declared defaults, and every default sits
    /// inside its own range — a default outside its range would clamp on
    /// the first edit and jump under the user's hand.
    #[test]
    fn defaults_are_in_range() {
        for k in KINDS {
            let e = Effect::new(1, k);
            assert_eq!(e.params.len(), k.params().len(), "{k:?}");
            for (i, spec) in k.params().iter().enumerate() {
                assert!(spec.min <= spec.max, "{k:?} {}: inverted range", spec.name);
                assert!(
                    (spec.min..=spec.max).contains(&e.get(i)),
                    "{k:?} {}: default {} outside {}..{}",
                    spec.name,
                    spec.default,
                    spec.min,
                    spec.max
                );
            }
        }
    }

    #[test]
    fn params_clamp_to_their_range() {
        let mut e = Effect::new(1, EffectKind::Glow);
        e.set(0, 5000.0);
        assert_eq!(e.get(0), 200.0);
        e.set(0, -5.0);
        assert_eq!(e.get(0), 0.0);
    }

    /// Adding a kind that's already there turns it back on instead of
    /// stacking a twin that would silently override the first.
    #[test]
    fn a_unique_kind_is_never_added_twice() {
        let mut s = Stack::default();
        let a = s.add(EffectKind::Glow, s.next_id());
        s.find_mut(a).unwrap().on = false;
        s.find_mut(a).unwrap().set(0, 90.0);
        let b = s.add(EffectKind::Glow, s.next_id());
        assert_eq!(a, b, "a second Glow made a new effect");
        assert_eq!(s.effects.len(), 1);
        assert!(s.find(a).unwrap().on, "re-adding turns it back on");
        assert_eq!(s.find(a).unwrap().get(0), 90.0, "and keeps its settings");
    }

    /// Turning an effect off keeps it in the stack with its settings — it's
    /// an audition toggle, not a delete.
    #[test]
    fn off_is_not_gone() {
        let mut s = Stack::default();
        let id = s.add(EffectKind::Glow, s.next_id());
        s.find_mut(id).unwrap().set(0, 120.0);
        s.find_mut(id).unwrap().on = false;
        assert!(s.find(id).is_some(), "still listed");
        assert!(s.active(EffectKind::Glow).is_none(), "but not drawing");
        assert_eq!(s.find(id).unwrap().get(0), 120.0, "settings kept");
    }

    /// The stack is the source of truth for what it controls. An absent
    /// effect has to actively clear its field, or a look nobody asked for
    /// leaks in from storage the layer card doesn't show.
    #[test]
    fn an_absent_effect_clears_what_it_controls() {
        let mut sh = spark_render::Shape::circle([0.0, 0.0], 10.0);
        sh.set_glow(80.0);
        sh.set_gradient(true);
        resolve(&mut sh, &Stack::default());
        assert_eq!(sh.glow_radius(), 0.0, "glow leaked in");
        assert!(!sh.gradient(), "gradient leaked in");
    }

    /// ...and the mirror of it: what the stack does *not* control, it must
    /// leave alone. Additive is the shape's own field, set by the checkbox
    /// on its card. The resolver used to overwrite it from an effect every
    /// frame, which is precisely what made that checkbox a dead control.
    #[test]
    fn resolve_leaves_the_shapes_own_fields_alone() {
        let mut sh = spark_render::Shape::circle([0.0, 0.0], 10.0);
        sh.set_additive(true);
        sh.set_opacity(0.4);
        sh.set_brightness(2.0);
        resolve(&mut sh, &Stack::default());
        assert!(sh.additive(), "the resolver turned the blend back off");
        assert_eq!(sh.opacity(), 0.4, "the resolver un-faded the shape");
        assert_eq!(sh.brightness(), 2.0, "the resolver reset the brightness");
    }

    /// An effect turned off is the same as no effect, as far as drawing
    /// goes — it just keeps its settings for when you turn it back on.
    #[test]
    fn resolve_honours_the_on_switch() {
        let mut s = Stack::default();
        let id = s.add(EffectKind::Glow, s.next_id());
        s.find_mut(id).unwrap().set(0, 75.0);
        let mut sh = spark_render::Shape::circle([0.0, 0.0], 10.0);
        resolve(&mut sh, &s);
        assert_eq!(sh.glow_radius(), 75.0);
        s.find_mut(id).unwrap().on = false;
        resolve(&mut sh, &s);
        assert_eq!(sh.glow_radius(), 0.0, "an off effect still drew");
    }

    /// An effect held at zero is a real thing to want — glow parked at
    /// nothing through a verse, waiting to be keyed up into the drop. It
    /// must survive being set there, or a slider drag through the bottom of
    /// the range would take the effect and its keyframes with it.
    #[test]
    fn zero_is_a_value_not_a_removal() {
        let mut s = Stack::default();
        let id = s.add(EffectKind::Glow, s.next_id());
        s.find_mut(id).unwrap().set(0, 0.0);
        assert!(s.find(id).is_some(), "zero removed the effect");
        assert_eq!(s.find(id).unwrap().get(0), 0.0);
        // ...and it still draws as nothing, which is the point.
        let mut sh = spark_render::Shape::circle([0.0, 0.0], 10.0);
        resolve(&mut sh, &s);
        assert_eq!(sh.glow_radius(), 0.0);
    }

    /// Ids are what curves address, so a fresh effect must never collide
    /// with a live one. Ids come from `max + 1`, so removing the *top*
    /// effect does free its number for the next add — which is safe only
    /// because `Editor::remove_effect` takes that effect's curves with it,
    /// and would otherwise resurrect them on the next add.
    #[test]
    fn a_new_effect_never_collides_with_a_live_one() {
        let mut s = Stack::default();
        let first = s.add(EffectKind::Glow, s.next_id());
        let second = s.add(EffectKind::Gradient, s.next_id());
        assert_ne!(first, second);
        assert!(s.remove(second));
        // Re-adding the same kind must not inherit the dead id's curves.
        let third = s.add(EffectKind::Gradient, s.next_id());
        assert_ne!(third, first, "collided with the effect still on the stack");
    }
}
