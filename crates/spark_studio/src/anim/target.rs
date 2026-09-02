//! What a keyframe track drives.
//!
//! A curve used to name a [`Prop`] and nothing else, which quietly meant
//! "everything animatable is a field on `Shape`". Effects broke that: a
//! glow radius now lives on an effect in the layer's stack, not on the
//! shape, and it still has to be keyable.
//!
//! Effects are addressed by their **stable id**, never their position in
//! the stack — reordering effects must not repoint a curve at a different
//! one. Shape ids taught that lesson once already (`editor::Editor::ids`).

use crate::props::Prop;

/// The thing a [`super::Track`] writes to.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Target {
    /// A property of the shape itself, or of a folder's transform.
    Shape(Prop),
    /// One parameter of one effect on this layer.
    Effect { id: u32, param: u8 },
}

impl Target {
    pub fn prop(self) -> Option<Prop> {
        match self {
            Target::Shape(p) => Some(p),
            Target::Effect { .. } => None,
        }
    }

    /// How it serializes: a bare property tag, or `fx:<id>:<param>`.
    pub fn tag(self) -> String {
        match self {
            Target::Shape(p) => super::prop_tag(p).to_string(),
            Target::Effect { id, param } => format!("fx:{id}:{param}"),
        }
    }

    pub fn parse(tag: &str) -> Option<Self> {
        if let Some(rest) = tag.strip_prefix("fx:") {
            let (id, param) = rest.split_once(':')?;
            return Some(Target::Effect {
                id: id.parse().ok()?,
                param: param.parse().ok()?,
            });
        }
        super::parse_prop(tag).map(Target::Shape)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The on-disk contract. A target that doesn't survive a round trip is
    /// a curve that vanishes when the comp is reopened.
    #[test]
    fn targets_round_trip_through_their_tag() {
        for t in [
            Target::Shape(Prop::X),
            Target::Shape(Prop::X1),
            Target::Shape(Prop::Y2),
            Target::Shape(Prop::Rotation),
            Target::Shape(Prop::Twinkle),
            Target::Effect { id: 1, param: 0 },
            Target::Effect { id: 42, param: 7 },
        ] {
            assert_eq!(Target::parse(&t.tag()), Some(t), "{t:?}");
        }
    }

    /// An effect tag can't be mistaken for a property tag, and nonsense is
    /// rejected rather than silently landing on some default property.
    #[test]
    fn malformed_tags_are_rejected() {
        for bad in ["fx:", "fx:1", "fx:a:0", "fx:1:b", "fx::", "nonsense", ""] {
            assert_eq!(Target::parse(bad), None, "{bad:?} parsed");
        }
    }
}
