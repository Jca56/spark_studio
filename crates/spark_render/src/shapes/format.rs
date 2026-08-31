//! The shape's serialized form: one line of the .spark text format.
//!
//! One shape is [`FIELDS`] floats. The count has grown four times (14 ->
//! 18 with gradients, 18 -> 22 with `extra`, 22 -> 26 with opacity, 26 ->
//! 30 with `space`) and the document parser reads every past length, so
//! old comps keep opening. Every era but the last is zero-filled — except
//! opacity, which is the one field a zero would silently erase the shape
//! with, so the parser fills it with 1.0 — see [`Shape::from_short_array`].

use super::{FIELDS, OPACITY_FIELD, Shape};

impl Shape {
    pub fn to_array(&self) -> [f32; FIELDS] {
        [
            self.kind_rot[0],
            self.kind_rot[1],
            self.a[0],
            self.a[1],
            self.b[0],
            self.b[1],
            self.color[0],
            self.color[1],
            self.color[2],
            self.color[3],
            self.style[0],
            self.style[1],
            self.style[2],
            self.style[3],
            self.color2[0],
            self.color2[1],
            self.color2[2],
            self.color2[3],
            self.extra[0],
            self.extra[1],
            self.extra[2],
            self.extra[3],
            self.over[0],
            self.over[1],
            self.over[2],
            self.over[3],
            self.space[0],
            self.space[1],
            self.space[2],
            self.space[3],
        ]
    }

    pub fn from_array(v: [f32; FIELDS]) -> Self {
        Self {
            kind_rot: [v[0], v[1]],
            a: [v[2], v[3]],
            b: [v[4], v[5]],
            color: [v[6], v[7], v[8], v[9]],
            style: [v[10], v[11], v[12], v[13]],
            color2: [v[14], v[15], v[16], v[17]],
            extra: [v[18], v[19], v[20], v[21]],
            over: [v[22], v[23], v[24], v[25]],
            space: [v[26], v[27], v[28], v[29]],
        }
    }

    /// Read a shape from an older, shorter line — `count` floats of it, the
    /// rest zero.
    ///
    /// The rule lives here rather than in the document parser because it is
    /// a fact about the fields, not about the file: every field added since
    /// `count` reads as zero and zero is off for all of them **but
    /// opacity**, where zero is an erased shape. Nothing had been faded when
    /// nothing could fade, so a short line is opaque.
    pub fn from_short_array(mut v: [f32; FIELDS], count: usize) -> Self {
        if count <= OPACITY_FIELD {
            v[OPACITY_FIELD] = 1.0;
        }
        Self::from_array(v)
    }
}
