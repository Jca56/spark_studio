//! SDF-rendered glowing shape primitives — the atoms of shape layers.
//!
//! Shapes live in canvas units (a 1920x1080 stage, aspect-fit to the window)
//! and render as instanced quads whose fragment shader evaluates a signed
//! distance field: crisp core + exponential neon halo. Composited back to
//! front — cores occlude (list order is z-order), halos add like light.

mod format;
mod light;
mod mesh;
mod pick;
mod space;
mod stars;

pub use light::LIGHT_PICK;
pub use stars::STAR_FORMS;

pub const CANVAS_W: f32 = 1920.0;
pub const CANVAS_H: f32 = 1080.0;

const KIND_CIRCLE: f32 = 0.0;
const KIND_BOX: f32 = 1.0;
const KIND_NGON: f32 = 2.0;
const KIND_LINE: f32 = 3.0;
const KIND_PATH: f32 = 4.0;
const KIND_STARS: f32 = 5.0;
const KIND_MESH: f32 = 6.0;
const KIND_LIGHT: f32 = 7.0;

/// Floats in a serialized shape — see [`Shape::to_array`].
pub const FIELDS: usize = 30;

/// Where opacity sits in a serialized shape — see
/// [`Shape::from_short_array`], the only thing that should need to know.
const OPACITY_FIELD: usize = 22;

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
    /// A scattered star field filling a box region. One instance, any number
    /// of stars: the fragment shader hashes a grid of cells, each holding one
    /// star, so density costs nothing.
    Stars,
    /// An imported model, drawn by the mesh pass; the shape holds its
    /// fitted footprint and the asset it draws (see `mesh.rs`).
    Mesh,
    /// A light — sun, point or spot — that the meshes are lit by (see
    /// `light.rs`). Draws nothing itself; the editor draws it a gizmo.
    Light,
}

#[repr(C)]
#[derive(Clone, Copy, PartialEq, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Shape {
    kind_rot: [f32; 2],
    a: [f32; 2],
    b: [f32; 2],
    color: [f32; 4], // rgb + intensity
    style: [f32; 4], // glow radius, stroke half-width / line half-thickness / star radius, ngon sides / path bound / star density, additive (1 = pure light, never occludes)
    /// Gradient end color; alpha > 0 turns the two-color fill on (radial
    /// for circles, along-length for lines, local-Y linear otherwise).
    color2: [f32; 4],
    /// Kind-specific extras. Zero means off everywhere, so kinds that don't
    /// use it pay nothing. Stars: `[seed, twinkle amount, twinkle rate,
    /// star form]`.
    extra: [f32; 4],
    /// How the shape is composited *over* what is behind it: `[opacity,
    /// unused ×3]`.
    ///
    /// The one field here where zero is not "off" — off, for opacity, is
    /// invisible. A shape you can see carries 1.0, so [`Shape::base`] sets
    /// it and every reader of a short saved line has to fill it in (see
    /// [`Shape::from_array`]). The pass blends premultiplied, so fading is
    /// one multiply on the whole result: the body stops occluding at exactly
    /// the rate its halo stops emitting.
    over: [f32; 4],
    /// Where the shape's plane sits in the scene: `[z, tilt, turn,
    /// unused]`, all about the shape's own centre — see `space.rs`. Zero
    /// is the canvas plane, which is where every shape lived before the
    /// comp became a scene. The shape pass never reads it; the per-object
    /// model matrix built from it does the placing.
    space: [f32; 4],
}

impl Shape {
    fn base(kind: f32, a: [f32; 2], b: [f32; 2]) -> Self {
        Self {
            kind_rot: [kind, 0.0],
            a,
            b,
            color: [1.0, 1.0, 1.0, 1.0],
            // No glow. Zero means off here like everywhere else, and a
            // primitive that quietly emits light unless told not to is how
            // "everything is neon" became structural rather than chosen.
            style: [0.0, 0.0, 0.0, 0.0],
            color2: [0.0; 4],
            extra: [0.0; 4],
            over: [1.0, 0.0, 0.0, 0.0],
            space: [0.0; 4],
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
        } else if self.kind_rot[0] == KIND_STARS {
            ShapeKind::Stars
        } else if self.kind_rot[0] == KIND_MESH {
            ShapeKind::Mesh
        } else if self.kind_rot[0] == KIND_LIGHT {
            ShapeKind::Light
        } else {
            ShapeKind::Line
        }
    }

    pub fn is_stars(&self) -> bool {
        self.kind_rot[0] == KIND_STARS
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

    /// Whether the shape draws as an outline; `None` for lines, paths and
    /// star fields, where the distinction doesn't exist (paths are always
    /// strokes, and a field's `style[1]` is its star radius — flipping it to
    /// zero would erase the stars, not hollow them).
    pub fn outline(&self) -> Option<bool> {
        (!self.is_line() && !self.is_path() && !self.is_stars() && !self.is_mesh() && !self.is_light())
            .then(|| self.style[1] > 0.0)
    }

    pub fn is_ngon(&self) -> bool {
        self.kind_rot[0] == KIND_NGON
    }

    /// A line's endpoints (only meaningful for lines).
    pub fn line_ends(&self) -> ([f32; 2], [f32; 2]) {
        (self.a, self.b)
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
    /// boxes, circles (which are really ellipses) and star fields, whose
    /// `b` is the region they scatter into. `None` otherwise.
    pub fn box_size(&self) -> Option<[f32; 2]> {
        matches!(
            self.kind(),
            ShapeKind::Box | ShapeKind::Circle | ShapeKind::Stars
        )
        .then(|| [self.b[0] * 2.0, self.b[1] * 2.0])
    }

    fn boxy(&self) -> bool {
        matches!(
            self.kind(),
            ShapeKind::Box | ShapeKind::Circle | ShapeKind::Stars
        )
    }

    pub fn set_box_width(&mut self, w: f32) {
        if self.boxy() {
            self.b[0] = (w * 0.5).clamp(1.5, 2000.0);
        }
    }

    pub fn set_box_height(&mut self, h: f32) {
        if self.boxy() {
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

    /// How much of the shape reaches the frame: 1 is solid, 0 is gone.
    pub fn opacity(&self) -> f32 {
        self.over[0]
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
        self.style[0] = g.clamp(0.0, 600.0);
    }

    pub fn set_brightness(&mut self, b: f32) {
        self.color[3] = b.clamp(0.05, 8.0);
    }

    /// Fading all the way out is the point, so unlike brightness this one
    /// has no floor: 0 means the shape emits nothing *and* occludes nothing.
    pub fn set_opacity(&mut self, o: f32) {
        self.over[0] = o.clamp(0.0, 1.0);
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

    /// Two-color gradient fill on/off (the mode follows the shape's kind).
    pub fn gradient(&self) -> bool {
        self.color2[3] > 0.5
    }

    pub fn set_gradient(&mut self, on: bool) {
        self.color2[3] = if on { 1.0 } else { 0.0 };
    }

    pub fn rgb2(&self) -> [f32; 3] {
        [self.color2[0], self.color2[1], self.color2[2]]
    }

    pub fn set_rgb2(&mut self, rgb: [f32; 3]) {
        self.color2[..3].copy_from_slice(&rgb);
    }

    pub fn set_rgb(&mut self, rgb: [f32; 3]) {
        self.color[0..3].copy_from_slice(&rgb);
    }

    pub fn add_glow(&mut self, delta: f32) {
        self.style[0] = (self.style[0] + delta).clamp(0.0, 600.0);
    }

    pub fn add_intensity(&mut self, delta: f32) {
        self.color[3] = (self.color[3] + delta).clamp(0.05, 8.0);
    }

    pub fn toggle_outline(&mut self) {
        if self.outline().is_some() {
            self.style[1] = if self.style[1] > 0.0 { 0.0 } else { 4.0 };
        }
    }

    pub fn set_outline(&mut self, on: bool) {
        if self.outline().is_some() {
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
        } else if k == KIND_LIGHT {
            // Around the gizmo, not the range: the range is a reach, not
            // a thing you can see the edge of.
            Self::circle(self.a, light::LIGHT_PICK + 8.0)
        } else if k == KIND_BOX || k == KIND_STARS || k == KIND_MESH {
            // A field's ants ride its region — the box you dragged is the
            // object, so that's what has to read as selected. A mesh's ride
            // its fitted footprint.
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
        // The ants ride the shape's plane, so a turned shape is outlined
        // where it is drawn.
        h.space = self.space;
        h.style[1] = 2.2;
        h.color = [1.0, 1.0, 1.0, 0.9];
        h.style[0] = 2.0;
        // Dashed light overlay ("marching ants") — never occludes, never
        // glows, so the selected shape's own look stays readable.
        h.style[3] = 2.0;
        h
    }
}
