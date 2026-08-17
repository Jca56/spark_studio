//! How a key is addressed from outside the curve: who owns a track, and the
//! keyframe clipboard that carries copied keys between times.

use crate::props::Prop;

use super::{Ease, KEY_EPS};

/// What a keyframe track belongs to.
///
/// **Both kinds are addressed by id, never by stack position.** A lane, a
/// key selection and the keyframe clipboard all outlive the frame they were
/// made in, and stack indices don't survive a reorder or a delete — holding
/// one meant a selected key silently repointed at whatever shape had slid
/// into that slot. An id that no longer resolves is simply gone, which every
/// key operation already handles by skipping it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Owner {
    Shape(u32),
    Folder(u32),
}

impl Owner {
    /// Folder transforms only animate X/Y/Rotation/Scale.
    pub fn animates(&self, prop: Prop) -> bool {
        match self {
            Owner::Shape(_) => true,
            Owner::Folder(_) => matches!(prop, Prop::X | Prop::Y | Prop::Rotation | Prop::Scale),
        }
    }
}

/// One copied keyframe: its source owner, offset from the earliest copied
/// key, and the property values stamped at that time.
pub type ClipKey = (Owner, f32, Vec<(Prop, f32, Ease)>);

/// Copied keyframes riding the clipboard.
#[derive(Clone)]
pub struct KeyClip {
    pub keys: Vec<ClipKey>,
    /// First-to-last key distance in seconds (0 for a single key).
    pub span: f32,
    /// Absolute time the earliest key was copied from — repeat-paste uses
    /// it to keep the pattern's phase within its bar.
    pub base: f32,
}

/// Whether `(owner, t)` is in a key list, by near-equal time.
pub fn key_list_has(list: &[(Owner, f32)], o: Owner, t: f32) -> bool {
    list.iter()
        .any(|&(j, jt)| j == o && (jt - t).abs() < KEY_EPS)
}
