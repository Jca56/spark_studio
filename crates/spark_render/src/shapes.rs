//! SDF-rendered glowing shape primitives — the atoms of shape layers.
//!
//! Shapes live in canvas units (a 1920x1080 stage, aspect-fit to the window)
//! and render as instanced quads whose fragment shader evaluates a signed
//! distance field: crisp core + exponential neon halo. Composited back to
//! front — cores occlude (list order is z-order), halos add like light.

use crate::sdf;

pub const CANVAS_W: f32 = 1920.0;
pub const CANVAS_H: f32 = 1080.0;

const KIND_CIRCLE: f32 = 0.0;
const KIND_BOX: f32 = 1.0;
const KIND_NGON: f32 = 2.0;
const KIND_LINE: f32 = 3.0;
const KIND_PATH: f32 = 4.0;

/// What a shape is, for UI that lists or describes shapes.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ShapeKind {
    Circle,
    Box,
    Ngon,
    Line,
    /// A polyline through center-relative vertices held outside the shape
    /// (the document owns the vertex list; `b` = [list id, ±count], count
    /// negative when the path closes back on itself).
    Path,
}

#[repr(C)]
#[derive(Clone, Copy, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Shape {
    kind_rot: [f32; 2],
    a: [f32; 2],
    b: [f32; 2],
    color: [f32; 4], // rgb + intensity
    style: [f32; 4], // glow radius, stroke half-width / line half-thickness, ngon sides, additive (1 = pure light, never occludes)
}

impl Shape {
    fn base(kind: f32, a: [f32; 2], b: [f32; 2]) -> Self {
        Self {
            kind_rot: [kind, 0.0],
            a,
            b,
            color: [1.0, 1.0, 1.0, 1.0],
            style: [20.0, 0.0, 0.0, 0.0],
        }
    }

    pub fn circle(center: [f32; 2], radius: f32) -> Self {
        Self::base(KIND_CIRCLE, center, [radius, radius])
    }

    pub fn rect(center: [f32; 2], half: [f32; 2]) -> Self {
        Self::base(KIND_BOX, center, half)
    }

    pub fn ngon(center: [f32; 2], radius: f32, sides: u32) -> Self {
        let mut s = Self::base(KIND_NGON, center, [radius, radius]);
        s.style[2] = sides as f32;
        s
    }

    pub fn line(from: [f32; 2], to: [f32; 2], half_thickness: f32) -> Self {
        let mut s = Self::base(KIND_LINE, from, to);
        s.style[1] = half_thickness;
        s
    }

    /// A polyline shape. `bound` is the farthest vertex distance from the
    /// center (drives the instance quad and Scale).
    pub fn path(
        center: [f32; 2],
        id: usize,
        count: usize,
        closed: bool,
        bound: f32,
        half_thickness: f32,
    ) -> Self {
        let mut s = Self::base(KIND_PATH, center, [0.0, 0.0]);
        s.b = [
            id as f32,
            if closed {
                -(count as f32)
            } else {
                count as f32
            },
        ];
        s.style[1] = half_thickness.max(0.5);
        s.style[2] = bound.max(1.0);
        s
    }

    // --- builder-style styling ---

    pub fn color(mut self, r: f32, g: f32, b: f32) -> Self {
        self.color[0] = r;
        self.color[1] = g;
        self.color[2] = b;
        self
    }

    pub fn intensity(mut self, i: f32) -> Self {
        self.color[3] = i;
        self
    }

    pub fn glow(mut self, radius: f32) -> Self {
        self.style[0] = radius;
        self
    }

    /// Outline instead of fill (not meaningful for lines).
    pub fn stroke(mut self, half_width: f32) -> Self {
        self.style[1] = half_width;
        self
    }

    pub fn rot(mut self, radians: f32) -> Self {
        self.kind_rot[1] = radians;
        self
    }

    // --- queries ---

    pub fn is_line(&self) -> bool {
        self.kind_rot[0] == KIND_LINE
    }

    pub fn kind(&self) -> ShapeKind {
        if self.kind_rot[0] == KIND_CIRCLE {
            ShapeKind::Circle
        } else if self.kind_rot[0] == KIND_BOX {
            ShapeKind::Box
        } else if self.kind_rot[0] == KIND_NGON {
            ShapeKind::Ngon
        } else if self.kind_rot[0] == KIND_PATH {
            ShapeKind::Path
        } else {
            ShapeKind::Line
        }
    }

    pub fn is_path(&self) -> bool {
        self.kind_rot[0] == KIND_PATH
    }

    /// (vertex list id, count, closed) for paths.
    pub fn path_meta(&self) -> Option<(usize, usize, bool)> {
        self.is_path().then(|| {
            (
                self.b[0] as usize,
                self.b[1].abs() as usize,
                self.b[1] < 0.0,
            )
        })
    }

    /// Repoint a display copy's vertex range at the flattened frame buffer.
    pub fn set_path_start(&mut self, start: usize) {
        if self.is_path() {
            self.b[0] = start as f32;
        }
    }

    /// Refresh count/closed/bound after the vertex list changed.
    pub fn set_path_shape(&mut self, count: usize, closed: bool, bound: f32) {
        if self.is_path() {
            self.b[1] = if closed {
                -(count as f32)
            } else {
                count as f32
            };
            self.style[2] = bound.max(1.0);
        }
    }

    pub fn rgb(&self) -> [f32; 3] {
        [self.color[0], self.color[1], self.color[2]]
    }

    /// Whether the shape draws as an outline; `None` for lines and paths,
    /// where the distinction doesn't exist (paths are always strokes).
    pub fn outline(&self) -> Option<bool> {
        (!self.is_line() && !self.is_path()).then(|| self.style[1] > 0.0)
    }

    pub fn is_ngon(&self) -> bool {
        self.kind_rot[0] == KIND_NGON
    }

    /// A line's endpoints (only meaningful for lines).
    pub fn line_ends(&self) -> ([f32; 2], [f32; 2]) {
        (self.a, self.b)
    }

    /// Signed distance from a canvas point to the *filled* silhouette
    /// (outline carving ignored, so a click inside an outlined shape hits it).
    pub fn distance(&self, p: [f32; 2]) -> f32 {
        if self.is_line() {
            return sdf::sd_segment(p, self.a, self.b) - self.style[1];
        }
        if self.is_path() {
            // Needs the vertex list the document owns — the editor computes
            // path picking itself.
            return f32::MAX;
        }
        let d = [p[0] - self.a[0], p[1] - self.a[1]];
        let (sn, cs) = (-self.kind_rot[1]).sin_cos();
        let q = [d[0] * cs - d[1] * sn, d[0] * sn + d[1] * cs];
        if self.kind_rot[0] == KIND_CIRCLE {
            // Ellipse approximation, matching the shader.
            let rx = self.b[0].max(0.001);
            let ry = self.b[1].max(0.001);
            let n = ((q[0] / rx).powi(2) + (q[1] / ry).powi(2)).sqrt();
            (n - 1.0) * rx.min(ry)
        } else if self.kind_rot[0] == KIND_BOX {
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
        if !self.is_line() && self.style[1] > 0.0 {
            d.abs() - self.style[1]
        } else {
            d
        }
    }

    /// Uniform size: radius for circles/ngons, the larger half-extent for
    /// boxes, half the length for lines, the vertex bound for paths. Pairs
    /// with [`Shape::scale_by`].
    pub fn size(&self) -> f32 {
        if self.is_line() {
            let d = [self.b[0] - self.a[0], self.b[1] - self.a[1]];
            (d[0] * d[0] + d[1] * d[1]).sqrt() * 0.5
        } else if self.is_path() {
            self.style[2]
        } else {
            self.b[0].max(self.b[1])
        }
    }

    /// Stroke half-width for lines and outlined shapes; `None` for fills.
    pub fn thickness(&self) -> Option<f32> {
        (self.is_line() || self.style[1] > 0.0).then_some(self.style[1])
    }

    /// No-op on filled shapes — thickness there would turn them into
    /// outlines, which is the Style toggle's job.
    pub fn set_thickness(&mut self, v: f32) {
        if self.is_line() || self.style[1] > 0.0 {
            self.style[1] = v.clamp(0.5, 60.0);
        }
    }

    /// Full dimensions (width, height) for the per-axis-sizable kinds:
    /// boxes and circles (which are really ellipses). `None` otherwise.
    pub fn box_size(&self) -> Option<[f32; 2]> {
        matches!(self.kind(), ShapeKind::Box | ShapeKind::Circle)
            .then(|| [self.b[0] * 2.0, self.b[1] * 2.0])
    }

    pub fn set_box_width(&mut self, w: f32) {
        if matches!(self.kind(), ShapeKind::Box | ShapeKind::Circle) {
            self.b[0] = (w * 0.5).clamp(1.5, 2000.0);
        }
    }

    pub fn set_box_height(&mut self, h: f32) {
        if matches!(self.kind(), ShapeKind::Box | ShapeKind::Circle) {
            self.b[1] = (h * 0.5).clamp(1.5, 2000.0);
        }
    }

    pub fn center(&self) -> [f32; 2] {
        if self.is_line() {
            [(self.a[0] + self.b[0]) * 0.5, (self.a[1] + self.b[1]) * 0.5]
        } else {
            self.a
        }
    }

    /// Absolute rotation in radians. For lines this is the segment's angle.
    pub fn rotation(&self) -> f32 {
        if self.is_line() {
            (self.b[1] - self.a[1]).atan2(self.b[0] - self.a[0])
        } else {
            self.kind_rot[1]
        }
    }

    pub fn glow_radius(&self) -> f32 {
        self.style[0]
    }

    pub fn brightness(&self) -> f32 {
        self.color[3]
    }

    pub fn sides(&self) -> Option<u32> {
        self.is_ngon().then(|| self.style[2] as u32)
    }

    // --- edits ---

    pub fn set_center(&mut self, c: [f32; 2]) {
        let cur = self.center();
        self.translate([c[0] - cur[0], c[1] - cur[1]]);
    }

    pub fn set_rotation(&mut self, r: f32) {
        let cur = self.rotation();
        self.rotate_by(r - cur);
    }

    pub fn set_glow(&mut self, g: f32) {
        self.style[0] = g.clamp(2.0, 600.0);
    }

    pub fn set_brightness(&mut self, b: f32) {
        self.color[3] = b.clamp(0.05, 8.0);
    }

    pub fn translate(&mut self, d: [f32; 2]) {
        self.a[0] += d[0];
        self.a[1] += d[1];
        if self.is_line() {
            self.b[0] += d[0];
            self.b[1] += d[1];
        }
    }

    pub fn scale_by(&mut self, s: f32) {
        if self.is_line() {
            let mid = [(self.a[0] + self.b[0]) * 0.5, (self.a[1] + self.b[1]) * 0.5];
            for p in [&mut self.a, &mut self.b] {
                p[0] = mid[0] + (p[0] - mid[0]) * s;
                p[1] = mid[1] + (p[1] - mid[1]) * s;
            }
        } else if self.is_path() {
            // The vertex list scales document-side; only the bound lives here.
            self.style[2] = (self.style[2] * s).clamp(1.0, 4000.0);
        } else {
            self.b[0] = (self.b[0] * s).clamp(1.0, 4000.0);
            self.b[1] = (self.b[1] * s).clamp(1.0, 4000.0);
        }
    }

    pub fn rotate_by(&mut self, r: f32) {
        if self.is_line() {
            let mid = [(self.a[0] + self.b[0]) * 0.5, (self.a[1] + self.b[1]) * 0.5];
            let (sn, cs) = r.sin_cos();
            for p in [&mut self.a, &mut self.b] {
                let d = [p[0] - mid[0], p[1] - mid[1]];
                p[0] = mid[0] + d[0] * cs - d[1] * sn;
                p[1] = mid[1] + d[0] * sn + d[1] * cs;
            }
        } else {
            self.kind_rot[1] += r;
        }
    }

    pub fn set_rgb(&mut self, rgb: [f32; 3]) {
        self.color[0..3].copy_from_slice(&rgb);
    }

    pub fn add_glow(&mut self, delta: f32) {
        self.style[0] = (self.style[0] + delta).clamp(2.0, 600.0);
    }

    pub fn add_intensity(&mut self, delta: f32) {
        self.color[3] = (self.color[3] + delta).clamp(0.05, 8.0);
    }

    pub fn toggle_outline(&mut self) {
        if !self.is_line() && !self.is_path() {
            self.style[1] = if self.style[1] > 0.0 { 0.0 } else { 4.0 };
        }
    }

    pub fn set_outline(&mut self, on: bool) {
        if !self.is_line() && !self.is_path() {
            self.style[1] = if on { 4.0 } else { 0.0 };
        }
    }

    /// Additive shapes composite as pure light: identical overlaps merge
    /// instead of occluding.
    pub fn additive(&self) -> bool {
        self.style[3] > 0.5
    }

    pub fn set_additive(&mut self, on: bool) {
        self.style[3] = if on { 1.0 } else { 0.0 };
    }

    pub fn set_sides(&mut self, n: u32) {
        if self.is_ngon() {
            self.style[2] = n.clamp(3, 24) as f32;
        }
    }

    /// A dashed light outline ("marching ants") hugging this shape, for
    /// selection display. Lines get a rotated bounding-rect outline —
    /// striping the segment itself just reads as a candy cane.
    pub fn selection_halo(&self) -> Shape {
        let k = self.kind_rot[0];
        let mut h = if k == KIND_CIRCLE {
            Self::circle(self.a, self.b[0].max(self.b[1]) + 10.0)
        } else if k == KIND_BOX {
            Self::rect(self.a, [self.b[0] + 10.0, self.b[1] + 10.0])
        } else if k == KIND_NGON {
            Self::ngon(self.a, self.b[0] + 12.0, self.style[2].max(3.0) as u32)
        } else if k == KIND_PATH {
            Self::rect(self.a, [self.style[2] + 10.0, self.style[2] + 10.0])
        } else {
            let d = [self.b[0] - self.a[0], self.b[1] - self.a[1]];
            let len = (d[0] * d[0] + d[1] * d[1]).sqrt();
            Self::rect(self.center(), [len * 0.5 + 8.0, self.style[1] + 8.0]).rot(d[1].atan2(d[0]))
        };
        if k != KIND_LINE {
            h.kind_rot[1] = self.kind_rot[1];
        }
        h.style[1] = 2.2;
        h.color = [1.0, 1.0, 1.0, 0.9];
        h.style[0] = 2.0;
        // Dashed light overlay ("marching ants") — never occludes, never
        // glows, so the selected shape's own look stays readable.
        h.style[3] = 2.0;
        h
    }

    // --- serialization (seed of the project text format) ---

    pub fn to_array(&self) -> [f32; 14] {
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
        ]
    }

    pub fn from_array(v: [f32; 14]) -> Self {
        Self {
            kind_rot: [v[0], v[1]],
            a: [v[2], v[3]],
            b: [v[4], v[5]],
            color: [v[6], v[7], v[8], v[9]],
            style: [v[10], v[11], v[12], v[13]],
        }
    }
}
