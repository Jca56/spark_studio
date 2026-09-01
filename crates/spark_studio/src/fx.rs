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

use crate::anim::Target;
use crate::props::Prop;

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
pub const KINDS: [EffectKind; 2] = [EffectKind::Glow, EffectKind::Gradient];

const GLOW: [ParamSpec; 1] = [p("Radius", 0.0, 200.0, 30.0, 0.0)];
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
        }
    }

    /// The tag it serializes under. Short, stable, and never reused for a
    /// different kind — a comp written today has to open in a year.
    pub fn tag(self) -> &'static str {
        match self {
            EffectKind::Glow => "glow",
            EffectKind::Gradient => "grad",
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

/// A layer's effect stack, innermost first — and its **reactions**: the
/// settings that ride the track (see [`Reaction`]). They live here
/// because the stack already rides every road an object's optional
/// behaviour takes: cloned with the object, undone with it, written
/// beside its `fx` lines.
#[derive(Clone, PartialEq, Debug, Default)]
pub struct Stack {
    pub effects: Vec<Effect>,
    pub reactions: Vec<Reaction>,
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

    /// The reaction on a setting, if it rides the track.
    pub fn reaction(&self, target: Target) -> Option<Reaction> {
        self.reactions.iter().copied().find(|r| r.target == target)
    }

    /// Set a setting's reaction, replacing the one it had.
    pub fn set_reaction(&mut self, r: Reaction) {
        match self.reactions.iter_mut().find(|x| x.target == r.target) {
            Some(x) => *x = r,
            None => self.reactions.push(r),
        }
    }

    pub fn remove_reaction(&mut self, target: Target) -> bool {
        let n = self.reactions.len();
        self.reactions.retain(|r| r.target != target);
        self.reactions.len() != n
    }
}

/// What a setting can ride: one of the curves the analysis bakes
/// (`spark_audio::Curves`) — a band's energy, the onsets, the loudness.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Source {
    Bass,
    LowMid,
    Mid,
    High,
    Onset,
    Loud,
}

impl Source {
    /// In the order the picker shows them.
    pub const ALL: [Source; 6] = [
        Source::Bass,
        Source::LowMid,
        Source::Mid,
        Source::High,
        Source::Onset,
        Source::Loud,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Source::Bass => "Bass",
            Source::LowMid => "Low",
            Source::Mid => "Mid",
            Source::High => "High",
            Source::Onset => "Onset",
            Source::Loud => "Loud",
        }
    }

    /// The tag it serializes under — stable, never reused.
    pub fn tag(self) -> &'static str {
        match self {
            Source::Bass => "bass",
            Source::LowMid => "lowmid",
            Source::Mid => "mid",
            Source::High => "high",
            Source::Onset => "onset",
            Source::Loud => "loud",
        }
    }

    pub fn from_tag(tag: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|s| s.tag() == tag)
    }

    pub fn level(self, l: &Levels) -> f32 {
        match self {
            Source::Bass => l.bass,
            Source::LowMid => l.low_mid,
            Source::Mid => l.mid,
            Source::High => l.high,
            Source::Onset => l.onset,
            Source::Loud => l.loud,
        }
    }
}

/// The track's curves at one moment of song time.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub struct Levels {
    pub bass: f32,
    pub low_mid: f32,
    pub mid: f32,
    pub high: f32,
    pub onset: f32,
    pub loud: f32,
}

impl Levels {
    /// Sampled at the playhead, not at a running player's clock: a
    /// paused frame has to read the same as the same frame in motion,
    /// which `frame = render(project, t)` says it must.
    pub fn at(track: &spark_audio::Track, t: f32) -> Self {
        let c = &track.curves;
        let s = |curve: &[f32]| spark_audio::Curves::sample(curve, c.rate, t);
        Self {
            bass: s(&c.bass),
            low_mid: s(&c.low_mid),
            mid: s(&c.mid),
            high: s(&c.high),
            onset: s(&c.onset),
            loud: s(&c.rms),
        }
    }
}

/// One setting riding one curve. Every frame the setting is pushed by
/// `level × amount`: a size by that fraction of itself, anything else
/// by that slice of its own unit (see [`react`]). Per setting, by
/// Alva's spec (2026-09-01) — the old React effect's three fixed
/// pairings became any setting, any trigger, its own intensity.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Reaction {
    pub target: Target,
    pub source: Source,
    pub amount: f32,
}

/// The intensity slider's reach — 1 is a full unit per full hit, already
/// absurd for most settings — and where a fresh reaction starts.
pub const AMOUNT_MAX: f32 = 2.0;
pub const AMOUNT_DEFAULT: f32 = 0.25;

/// Whether a setting rides proportionally — a size grows by a fraction
/// of itself — rather than by a slice of a fixed unit.
fn proportional(p: Prop) -> bool {
    matches!(
        p,
        Prop::Scale | Prop::Width | Prop::Height | Prop::Depth | Prop::Thickness
    )
}

/// How far a full hit at intensity 1 pushes a setting, in its own units:
/// the canvas across for a place, two thousand units of depth, a full
/// turn for an angle, the slider's whole range for a look.
pub fn unit(p: Prop, canvas: [f32; 2]) -> f32 {
    match p {
        Prop::X => canvas[0],
        Prop::Y => canvas[1],
        Prop::Z => 2000.0,
        Prop::Rotation | Prop::Tilt | Prop::Turn => std::f32::consts::TAU,
        _ => {
            let (lo, hi) = crate::props::range(p, canvas);
            hi - lo
        }
    }
}

/// Ride the track: every reaction on the stack pushes its setting by
/// its curve's level — on the display copies, before [`resolve`] paints
/// the effects, so a reaction on an effect's parameter (Glow's radius)
/// lands where the resolver reads it. The document never changes.
pub fn react(shape: &mut spark_render::Shape, stack: &mut Stack, levels: &Levels, canvas: [f32; 2]) {
    for k in 0..stack.reactions.len() {
        let r = stack.reactions[k];
        let push = r.source.level(levels) * r.amount;
        if push.abs() < 1e-6 {
            continue;
        }
        match r.target {
            Target::Shape(p) => {
                if let Some(v) = crate::anim::prop_value(shape, p) {
                    let next = if proportional(p) {
                        v * (1.0 + push)
                    } else {
                        v + push * unit(p, canvas)
                    };
                    crate::anim::apply_prop(shape, p, next);
                }
            }
            Target::Effect { id, param } => {
                if let Some(e) = stack.find_mut(id) {
                    let Some(spec) = e.kind.params().get(param as usize) else {
                        continue;
                    };
                    let span = spec.max - spec.min;
                    let v = e.get(param as usize);
                    e.set(param as usize, v + push * span);
                }
            }
        }
    }
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

    /// A reaction pushes a size by a fraction of itself, a place by a
    /// slice of the canvas, an effect parameter by a slice of its range —
    /// and only while its curve has something to say.
    #[test]
    fn a_reaction_pushes_its_setting_by_its_curve() {
        use crate::anim::Target;
        let canvas = spark_render::CANVAS;
        let mut stack = Stack::default();
        let gid = stack.add(EffectKind::Glow, 1);
        stack.set_reaction(Reaction {
            target: Target::Shape(Prop::Scale),
            source: Source::Bass,
            amount: 0.5,
        });
        stack.set_reaction(Reaction {
            target: Target::Shape(Prop::X),
            source: Source::Onset,
            amount: 0.1,
        });
        stack.set_reaction(Reaction {
            target: Target::Effect { id: gid, param: 0 },
            source: Source::Mid,
            amount: 1.0,
        });
        let mut s = spark_render::Shape::circle([300.0, 300.0], 40.0);
        let mut st = stack.clone();
        react(&mut s, &mut st, &Levels::default(), canvas);
        assert_eq!(s.size(), 40.0, "silence moves nothing");
        assert_eq!(s.center()[0], 300.0);
        let levels = Levels {
            bass: 1.0,
            onset: 0.5,
            mid: 0.5,
            ..Default::default()
        };
        let mut st = stack.clone();
        react(&mut s, &mut st, &levels, canvas);
        assert!((s.size() - 60.0).abs() < 1e-3, "a full bass hit at 0.5: half again");
        assert!((s.center()[0] - (300.0 + 0.05 * canvas[0])).abs() < 1e-2);
        assert!((st.find(gid).unwrap().get(0) - (30.0 + 0.5 * 200.0)).abs() < 1e-3);
        // Replacing and removing.
        stack.set_reaction(Reaction {
            target: Target::Shape(Prop::Scale),
            source: Source::High,
            amount: 0.1,
        });
        assert_eq!(stack.reactions.len(), 3);
        assert_eq!(
            stack.reaction(Target::Shape(Prop::Scale)).unwrap().source,
            Source::High
        );
        assert!(stack.remove_reaction(Target::Shape(Prop::Scale)));
        assert!(!stack.remove_reaction(Target::Shape(Prop::Scale)));
        for src in Source::ALL {
            assert_eq!(Source::from_tag(src.tag()), Some(src));
        }
    }

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
