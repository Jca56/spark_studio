//! Hit testing: how far a canvas point is from a shape.
//!
//! Separate from the shape data because it answers a different question —
//! not "what is this" but "did the click land on it" — and because the two
//! kinds that can't answer from the instance alone (paths, which need the
//! document's vertex list) are easier to see when they're side by side.

use crate::sdf;

use super::light::LIGHT_PICK;
use super::{KIND_BOX, KIND_CIRCLE, KIND_LIGHT, KIND_MESH, KIND_STARS, KIND_VORTEX, Shape};

impl Shape {
    /// Signed distance from a canvas point to the *filled* silhouette
    /// (outline carving ignored, so a click inside an outlined shape hits it).
    ///
    /// A star field answers for its whole region, not for the individual
    /// stars: clicking anywhere inside the box you drew picks the field,
    /// which is the only object there is to pick.
    pub fn distance(&self, p: [f32; 2]) -> f32 {
        if self.is_line() {
            // A bolt wanders as far as its jag from the straight line,
            // so that whole band is it.
            let reach = self.style[1] + self.jag().unwrap_or(0.0);
            return sdf::sd_segment(p, self.a, self.b) - reach;
        }
        if self.is_path() {
            // Needs the vertex list the document owns — the editor computes
            // path picking itself.
            return f32::MAX;
        }
        let d = [p[0] - self.a[0], p[1] - self.a[1]];
        if self.kind_rot[0] == KIND_LIGHT {
            // A light is picked by its gizmo, not by how far it shines.
            return (d[0] * d[0] + d[1] * d[1]).sqrt() - LIGHT_PICK;
        }
        if self.is_camera() {
            // No place on the canvas yet: the outliner picks it.
            return f32::MAX;
        }
        let (sn, cs) = (-self.kind_rot[1]).sin_cos();
        let q = [d[0] * cs - d[1] * sn, d[0] * sn + d[1] * cs];
        if self.kind_rot[0] == KIND_CIRCLE {
            // Ellipse approximation, matching the shader.
            let rx = self.b[0].max(0.001);
            let ry = self.b[1].max(0.001);
            let n = ((q[0] / rx).powi(2) + (q[1] / ry).powi(2)).sqrt();
            (n - 1.0) * rx.min(ry)
        } else if [KIND_BOX, KIND_STARS, KIND_MESH, KIND_VORTEX].contains(&self.kind_rot[0]) {
            sdf::sd_box(q, self.b)
        } else {
            // Negated to match the shader: canvas y-down flips ngons.
            sdf::sd_ngon([-q[0], -q[1]], self.b[0], self.style[2].max(3.0))
        }
    }

    /// Distance to what's actually *drawn*: outlined shapes carve to their
    /// ring, so a hollow center doesn't swallow clicks meant for shapes
    /// beneath it.
    pub fn pick_distance(&self, p: [f32; 2]) -> f32 {
        let d = self.distance(p);
        if !self.is_line() && !self.is_stars() && !self.is_vortex() && self.style[1] > 0.0 {
            d.abs() - self.style[1]
        } else {
            d
        }
    }
}
