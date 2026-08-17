//! Keyframe curves: per-shape, per-property value tracks over song time.
//! Evaluation is pure — `apply(shape, t)` poses a shape without touching
//! the curves — which is what keeps `frame = render(project, t)` honest.

use spark_render::Shape;

use crate::props::Prop;

/// Two keys closer than this (seconds) are the same key.
pub const KEY_EPS: f32 = 0.001;

/// Evaluation order: geometry before uniform scale, look last. Width/Height
/// set absolute extents, then Scale multiplies both axes — so a box keyed on
/// all three lands at Scale's size with W/H's aspect, deterministically.
pub const PROP_ORDER: [Prop; 10] = [
    Prop::X,
    Prop::Y,
    Prop::Rotation,
    Prop::Width,
    Prop::Height,
    Prop::Scale,
    Prop::Glow,
    Prop::Brightness,
    Prop::Sides,
    Prop::Thickness,
];

/// How a key interpolates toward the *next* key.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Ease {
    Smooth,
    Linear,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Key {
    pub t: f32,
    pub v: f32,
    pub ease: Ease,
}

/// One property's keys, kept sorted by time.
#[derive(Clone, PartialEq, Debug)]
pub struct Track {
    pub prop: Prop,
    pub keys: Vec<Key>,
}

impl Track {
    /// Curve value at `t`: clamped to the first/last key outside the range,
    /// eased between the surrounding pair inside it.
    pub fn sample(&self, t: f32) -> Option<f32> {
        let first = self.keys.first()?;
        if t <= first.t {
            return Some(first.v);
        }
        let last = self.keys.last()?;
        if t >= last.t {
            return Some(last.v);
        }
        let i = self.keys.partition_point(|k| k.t <= t) - 1;
        let a = self.keys[i];
        let b = self.keys[i + 1];
        let span = (b.t - a.t).max(KEY_EPS);
        let mut u = ((t - a.t) / span).clamp(0.0, 1.0);
        if a.ease == Ease::Smooth {
            u = u * u * (3.0 - 2.0 * u);
        }
        Some(a.v + (b.v - a.v) * u)
    }

    /// Set the value at `t`: overwrites a key already there, otherwise
    /// inserts a new smooth key in time order.
    pub fn upsert(&mut self, t: f32, v: f32) {
        match self.keys.iter_mut().find(|k| (k.t - t).abs() < KEY_EPS) {
            Some(k) => k.v = v,
            None => {
                let at = self.keys.partition_point(|k| k.t < t);
                self.keys.insert(
                    at,
                    Key {
                        t,
                        v,
                        ease: Ease::Smooth,
                    },
                );
            }
        }
    }
}

/// What a keyframe track belongs to. Folders are addressed by id rather
/// than position because ids survive reordering and shape indices don't.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Owner {
    Shape(usize),
    Folder(u32),
}

impl Owner {
    /// Folder transforms only animate X/Y/Rotation/Scale.
    pub fn animates(&self, prop: Prop) -> bool {
        match self {
            Owner::Shape(_) => true,
            Owner::Folder(_) => matches!(
                prop,
                Prop::X | Prop::Y | Prop::Rotation | Prop::Scale
            ),
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

/// All of one shape's tracks. Empty tracks never persist — "has a track"
/// always means "has keys".
#[derive(Clone, PartialEq, Default, Debug)]
pub struct ShapeAnim {
    pub tracks: Vec<Track>,
}

impl ShapeAnim {
    pub fn track_mut(&mut self, prop: Prop) -> Option<&mut Track> {
        self.tracks.iter_mut().find(|t| t.prop == prop)
    }

    pub fn has_keys(&self) -> bool {
        self.tracks.iter().any(|t| !t.keys.is_empty())
    }

    /// Distinct key times across every track, sorted, with each time's ease
    /// (linear wins if any co-timed key is linear — the lane marker shows it).
    pub fn key_times(&self) -> Vec<(f32, Ease)> {
        let mut out: Vec<(f32, Ease)> = Vec::new();
        for track in &self.tracks {
            for k in &track.keys {
                match out.iter_mut().find(|(t, _)| (*t - k.t).abs() < KEY_EPS) {
                    Some((_, e)) => {
                        if k.ease == Ease::Linear {
                            *e = Ease::Linear;
                        }
                    }
                    None => out.push((k.t, k.ease)),
                }
            }
        }
        out.sort_by(|a, b| a.0.total_cmp(&b.0));
        out
    }

    /// Pose `shape` at time `t` — keyed properties take their curve value,
    /// everything else keeps the shape's own.
    pub fn apply(&self, shape: &mut Shape, t: f32) {
        for prop in PROP_ORDER {
            let Some(track) = self.tracks.iter().find(|tr| tr.prop == prop) else {
                continue;
            };
            if let Some(v) = track.sample(t) {
                apply_prop(shape, prop, v);
            }
        }
    }

    /// Keyed-property bitmask, for gold value readouts (see [`prop_bit`]).
    pub fn keyed_mask(&self) -> u16 {
        self.tracks
            .iter()
            .filter(|t| !t.keys.is_empty())
            .fold(0, |m, t| m | prop_bit(t.prop))
    }

    pub fn prune_empty(&mut self) {
        self.tracks.retain(|t| !t.keys.is_empty());
    }
}

/// Write one property value absolutely (curves never accumulate — every
/// setter here lands on the target regardless of the shape's current state).
pub fn apply_prop(shape: &mut Shape, prop: Prop, v: f32) {
    match prop {
        Prop::X => {
            let c = shape.center();
            shape.set_center([v, c[1]]);
        }
        Prop::Y => {
            let c = shape.center();
            shape.set_center([c[0], v]);
        }
        Prop::Rotation => shape.set_rotation(v),
        Prop::Scale => {
            let cur = shape.size();
            if cur > 0.001 {
                shape.scale_by(v / cur);
            }
        }
        Prop::Width => shape.set_box_width(v),
        Prop::Height => shape.set_box_height(v),
        Prop::Glow => shape.set_glow(v),
        Prop::Brightness => shape.set_brightness(v),
        Prop::Sides => shape.set_sides(v.round().max(3.0) as u32),
        Prop::Thickness => shape.set_thickness(v),
        // React amounts live on the editor, not the shape — never curves.
        Prop::ReactScale | Prop::ReactGlow | Prop::ReactBright => {}
    }
}

/// Read one property off a shape; `None` where it doesn't apply (sides of a
/// circle, thickness of a fill).
pub fn prop_value(shape: &Shape, prop: Prop) -> Option<f32> {
    match prop {
        Prop::X => Some(shape.center()[0]),
        Prop::Y => Some(shape.center()[1]),
        Prop::Rotation => Some(shape.rotation()),
        Prop::Scale => Some(shape.size()),
        Prop::Width => shape.box_size().map(|b| b[0]),
        Prop::Height => shape.box_size().map(|b| b[1]),
        Prop::Glow => Some(shape.glow_radius()),
        Prop::Brightness => Some(shape.brightness()),
        Prop::Sides => shape.sides().map(|n| n as f32),
        Prop::Thickness => shape.thickness(),
        Prop::ReactScale | Prop::ReactGlow | Prop::ReactBright => None,
    }
}

/// Bit for `prop` in a keyed-property mask (inspector gold values).
pub fn prop_bit(prop: Prop) -> u16 {
    1 << PROP_ORDER.iter().position(|p| *p == prop).unwrap_or(15)
}

// --- serialization tags (the `anim` lines of the .spark format) ---

pub fn prop_tag(prop: Prop) -> &'static str {
    match prop {
        Prop::X => "x",
        Prop::Y => "y",
        Prop::Rotation => "rot",
        Prop::Scale => "scale",
        Prop::Width => "w",
        Prop::Height => "h",
        Prop::Glow => "glow",
        Prop::Brightness => "bright",
        Prop::Sides => "sides",
        Prop::Thickness => "thick",
        // Present for exhaustiveness; react amounts serialize on their own
        // `react` line, never as curves.
        Prop::ReactScale => "react-scale",
        Prop::ReactGlow => "react-glow",
        Prop::ReactBright => "react-bright",
    }
}

pub fn parse_prop(tag: &str) -> Option<Prop> {
    PROP_ORDER.into_iter().find(|p| prop_tag(*p) == tag)
}
