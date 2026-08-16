//! SDF-rendered glowing shape primitives — the atoms of shape layers.
//!
//! Shapes live in canvas units (a 1920x1080 stage, aspect-fit to the window)
//! and render as instanced quads whose fragment shader evaluates a signed
//! distance field: crisp core + exponential neon halo. Composited back to
//! front — cores occlude (list order is z-order), halos add like light.

use crate::geom::Viewport;
use crate::sdf;

pub const CANVAS_W: f32 = 1920.0;
pub const CANVAS_H: f32 = 1080.0;

const KIND_CIRCLE: f32 = 0.0;
const KIND_BOX: f32 = 1.0;
const KIND_NGON: f32 = 2.0;
const KIND_LINE: f32 = 3.0;

/// What a shape is, for UI that lists or describes shapes.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ShapeKind {
    Circle,
    Box,
    Ngon,
    Line,
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
        } else {
            ShapeKind::Line
        }
    }

    pub fn rgb(&self) -> [f32; 3] {
        [self.color[0], self.color[1], self.color[2]]
    }

    /// Whether the shape draws as an outline; `None` for lines, where the
    /// distinction doesn't exist.
    pub fn outline(&self) -> Option<bool> {
        (!self.is_line()).then(|| self.style[1] > 0.0)
    }

    pub fn is_ngon(&self) -> bool {
        self.kind_rot[0] == KIND_NGON
    }

    /// Signed distance from a canvas point to the *filled* silhouette
    /// (outline carving ignored, so a click inside an outlined shape hits it).
    pub fn distance(&self, p: [f32; 2]) -> f32 {
        if self.is_line() {
            return sdf::sd_segment(p, self.a, self.b) - self.style[1];
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
    /// boxes, half the length for lines. Pairs with [`Shape::scale_by`].
    pub fn size(&self) -> f32 {
        if self.is_line() {
            let d = [self.b[0] - self.a[0], self.b[1] - self.a[1]];
            (d[0] * d[0] + d[1] * d[1]).sqrt() * 0.5
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
        if !self.is_line() {
            self.style[1] = if self.style[1] > 0.0 { 0.0 } else { 4.0 };
        }
    }

    pub fn set_outline(&mut self, on: bool) {
        if !self.is_line() {
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
            Self::circle(self.a, self.b[0] + 10.0)
        } else if k == KIND_BOX {
            Self::rect(self.a, [self.b[0] + 10.0, self.b[1] + 10.0])
        } else if k == KIND_NGON {
            Self::ngon(self.a, self.b[0] + 12.0, self.style[2].max(3.0) as u32)
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

pub struct ShapePass {
    pipeline: wgpu::RenderPipeline,
    globals: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    instances: wgpu::Buffer,
    capacity: usize,
}

impl ShapePass {
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("shape"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/shape.wgsl").into()),
        });
        let globals = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("shape globals"),
            size: 32,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("shape globals"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("shape globals"),
            layout: &bgl,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: globals.as_entire_binding(),
            }],
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("shape"),
            bind_group_layouts: &[&bgl],
            ..Default::default()
        });
        // Premultiplied alpha: the shader emits alpha = core coverage, so
        // shape bodies occlude what's behind them while glow halos (alpha 0)
        // blend additively. Draw order is z-order, back to front.
        let layered = wgpu::BlendComponent {
            src_factor: wgpu::BlendFactor::One,
            dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
            operation: wgpu::BlendOperation::Add,
        };
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("shape"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: size_of::<Shape>() as u64,
                    step_mode: wgpu::VertexStepMode::Instance,
                    attributes: &wgpu::vertex_attr_array![
                        0 => Float32x2,
                        1 => Float32x2,
                        2 => Float32x2,
                        3 => Float32x4,
                        4 => Float32x4,
                    ],
                }],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState {
                        color: layered,
                        alpha: layered,
                    }),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleStrip,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });
        let capacity = 256;
        let instances = Self::make_instance_buffer(device, capacity);
        Self {
            pipeline,
            globals,
            bind_group,
            instances,
            capacity,
        }
    }

    fn make_instance_buffer(device: &wgpu::Device, capacity: usize) -> wgpu::Buffer {
        device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("shape instances"),
            size: (capacity * size_of::<Shape>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        })
    }

    pub fn draw(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        shapes: &[Shape],
        resolution: (u32, u32),
        viewport: Viewport,
        clear: wgpu::Color,
    ) {
        if shapes.len() > self.capacity {
            self.capacity = shapes.len().next_power_of_two();
            self.instances = Self::make_instance_buffer(device, self.capacity);
        }
        let globals = [
            resolution.0 as f32,
            resolution.1 as f32,
            viewport.x,
            viewport.y,
            viewport.w,
            viewport.h,
            CANVAS_W,
            CANVAS_H,
        ];
        queue.write_buffer(&self.globals, 0, bytemuck::cast_slice(&globals));
        queue.write_buffer(&self.instances, 0, bytemuck::cast_slice(shapes));

        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("shapes"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(clear),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            ..Default::default()
        });
        // Clip to the aspect-fit canvas: what you see is exactly the render
        // area — nothing (not even glow) paints outside the stage.
        let fit = (viewport.w / CANVAS_W).min(viewport.h / CANVAS_H);
        let fw = CANVAS_W * fit;
        let fh = CANVAS_H * fit;
        let fx = (viewport.x + (viewport.w - fw) * 0.5).max(0.0);
        let fy = (viewport.y + (viewport.h - fh) * 0.5).max(0.0);
        let x1 = (fx + fw).min(resolution.0 as f32);
        let y1 = (fy + fh).min(resolution.1 as f32);
        if x1 <= fx || y1 <= fy {
            return;
        }
        pass.set_scissor_rect(fx as u32, fy as u32, (x1 - fx) as u32, (y1 - fy) as u32);
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.bind_group, &[]);
        pass.set_vertex_buffer(0, self.instances.slice(..));
        pass.draw(0..4, 0..shapes.len() as u32);
    }
}
