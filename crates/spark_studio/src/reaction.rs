//! Reactions: a setting riding the track. Per setting, by Alva's spec
//! (2026-09-01) — any number the inspector edits can ride any curve the
//! analysis bakes, by an intensity of its own. The model lives on the
//! effect stack (`fx::Stack::reactions`); this is the vocabulary and
//! the push itself. Split from `fx` so each stays inside the file
//! budget; `fx` re-exports everything here.

use crate::anim::Target;
use crate::fx::Stack;
use crate::props::Prop;

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
        Prop::X | Prop::X1 | Prop::X2 => canvas[0],
        Prop::Y | Prop::Y1 | Prop::Y2 => canvas[1],
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
pub fn react(
    shape: &mut spark_render::Shape,
    stack: &mut Stack,
    levels: &Levels,
    canvas: [f32; 2],
) {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fx::EffectKind;

    /// A reaction pushes a size by a fraction of itself, a place by a
    /// slice of the canvas, an effect parameter by a slice of its range —
    /// and only while its curve has something to say.
    #[test]
    fn a_reaction_pushes_its_setting_by_its_curve() {
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
        assert!(
            (s.size() - 60.0).abs() < 1e-3,
            "a full bass hit at 0.5: half again"
        );
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
}
