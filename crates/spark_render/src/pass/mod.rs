//! The instanced shape render pass: buffers, pipeline, scissored draw.
//!
//! Shapes are flat. Each one is drawn on its own plane, in canvas units,
//! and a per-instance model matrix places that plane in the scene — tilted,
//! turned, moved off the canvas, or (for every shape that has never left
//! it) the identity. The fragment stage only ever sees plane-local
//! coordinates, so the distance fields, the glow and the anti-aliasing are
//! the same arithmetic whether the plane faces the camera or not.

use std::cmp::Ordering;

use crate::camera::{Camera, Framing};
use crate::light::Light;
use crate::math::{Mat4, Vec3};
use crate::shapes::Shape;

pub mod depth;
#[cfg(test)]
mod harness;
pub mod mesh;
#[cfg(test)]
mod scene_tests;
mod stage;
#[cfg(test)]
mod stage_tests;
#[cfg(test)]
mod tests;

pub use mesh::{GpuMesh, MeshData, MeshInstance, TextureData};
pub use stage::{Quality, Stage};

/// Everything a pass reads to draw the document: what to draw, where each
/// thing sits in the scene, what it is looked at through, and when.
#[derive(Clone, Copy)]
pub struct Scene<'a> {
    pub shapes: &'a [Shape],
    /// One matrix per shape — the object's plane → the world. May be
    /// shorter than `shapes`: anything without one is on the canvas plane
    /// (identity), which is where overlays and every 2D comp live.
    pub models: &'a [Mat4],
    /// Path vertex pool (canvas units, centre-relative), flat per frame.
    pub paths: &'a [[f32; 2]],
    /// The scene's opaque objects, drawn under every shape and writing
    /// the depth the shapes test against. Any order.
    pub meshes: &'a [MeshInstance<'a>],
    /// What the meshes are lit by. Empty means the default sun.
    pub lights: &'a [Light],
    pub camera: &'a Camera,
    /// Playhead seconds — the frame's clock, and what a shape without a
    /// clock of its own runs on. The frame stays a pure function of
    /// (document, t) instead of of how long the app has been open.
    pub time: f32,
    /// One clock per shape: the time a generator runs on — its clip's
    /// local time, so a looped explosion bursts every pass and a bolt
    /// crackles the same way each time round. May be shorter than
    /// `shapes`: anything without one runs on `time`.
    pub clocks: &'a [f32],
    /// How many shapes at the end of `shapes` are editor marks drawn
    /// **over** everything — the transform gizmo — ignoring the depth the
    /// opaque passes wrote, so a handle inside a mesh is still there to
    /// see and grab. They are sorted among themselves and drawn last.
    pub over: usize,
}

impl Scene<'_> {
    pub fn model(&self, i: usize) -> Mat4 {
        self.models.get(i).copied().unwrap_or(Mat4::IDENTITY)
    }

    pub fn clock(&self, i: usize) -> f32 {
        self.clocks.get(i).copied().unwrap_or(self.time)
    }

    /// Shapes and their models in drawing order: back to front by view
    /// depth. The sort is stable, so shapes at one depth keep their list
    /// order — which is how a 2D comp, all of it on one plane, still
    /// stacks exactly the way it did.
    ///
    /// The marks drawn over everything keep to the end, sorted among
    /// themselves, so `over` still counts them off the tail.
    pub fn sorted(&self) -> (Vec<Shape>, Vec<Mat4>, Vec<f32>) {
        let n = self.shapes.len();
        let split = n.saturating_sub(self.over);
        let mut order: Vec<(f32, usize)> = Vec::with_capacity(n);
        for range in [0..split, split..n] {
            let mut part: Vec<(f32, usize)> = range
                .map(|i| {
                    let c = self.shapes[i].center();
                    let p = self.model(i).transform_point(Vec3::new(c[0], c[1], 0.0));
                    (self.camera.depth(p), i)
                })
                .collect();
            part.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(Ordering::Equal));
            order.extend(part);
        }
        let shapes = order.iter().map(|&(_, i)| self.shapes[i]).collect();
        let models = order.iter().map(|&(_, i)| self.model(i)).collect();
        let clocks = order.iter().map(|&(_, i)| self.clock(i)).collect();
        (shapes, models, clocks)
    }
}

/// Which part of every shape a pass draws. `Full` is the whole thing in
/// one go — what export and the tests want. The stage splits the work:
/// `Bodies` at the frame's resolution in quads that hug the shape,
/// `Halos` at a lower one in the wide quads a halo needs. A halo that is
/// only a few pixels wide on screen is drawn with its body (see `parts`
/// in the shader), so the split is decided per shape, per frame.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Layer {
    Full = 0,
    Bodies = 1,
    Halos = 2,
}

const LAYERS: usize = 3;

/// Floats in the globals uniform: a `mat4x4` and a `vec4`.
const GLOBALS: usize = 20;

pub struct ShapePass {
    pipeline: wgpu::RenderPipeline,
    /// The same pipeline without the depth test, for the marks drawn
    /// over everything (see [`Scene::over`]).
    pipeline_over: wgpu::RenderPipeline,
    /// One globals buffer *per layer*. A `queue.write_buffer` lands before
    /// the whole command buffer runs, so two passes in one encoder that
    /// shared a buffer would both see the second pass's globals — the
    /// stage draws bodies and halos back to back and needs both to hold.
    globals: [wgpu::Buffer; LAYERS],
    bgl: wgpu::BindGroupLayout,
    bind_groups: [wgpu::BindGroup; LAYERS],
    instances: wgpu::Buffer,
    capacity: usize,
    /// Path vertex pool (canvas units, center-relative), flat per frame.
    verts: wgpu::Buffer,
    verts_capacity: usize,
    /// One model matrix per instance, indexed by `instance_index`.
    models: wgpu::Buffer,
    models_capacity: usize,
    /// One clock per instance — see `Scene::clocks`. Sized with `models`.
    clocks: wgpu::Buffer,
    /// A depth attachment for [`ShapePass::draw`], which renders a whole
    /// scene on its own and has no stage to borrow one from.
    scratch_depth: Option<((u32, u32), wgpu::TextureView)>,
}

/// The shape pipeline, with the depth state that decides whether it
/// asks what the opaque passes wrote (`depth::test_only`) or draws over it
/// (`depth::always`).
fn make_pipeline(
    device: &wgpu::Device,
    layout: &wgpu::PipelineLayout,
    shader: &wgpu::ShaderModule,
    format: wgpu::TextureFormat,
    label: &str,
    depth_stencil: wgpu::DepthStencilState,
) -> wgpu::RenderPipeline {
    // Premultiplied alpha: the shader emits alpha = core coverage, so
    // shape bodies occlude what's behind them while glow halos (alpha 0)
    // blend additively. Draw order is back to front.
    let layered = wgpu::BlendComponent {
        src_factor: wgpu::BlendFactor::One,
        dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
        operation: wgpu::BlendOperation::Add,
    };
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(label),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module: shader,
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
                    5 => Float32x4,
                    6 => Float32x4,
                    7 => Float32x4,
                ],
            }],
        },
        fragment: Some(wgpu::FragmentState {
            module: shader,
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
        // Flat, translucent, sorted: shapes test against what the
        // opaque passes wrote and never write depth themselves.
        depth_stencil: Some(depth_stencil),
        multisample: wgpu::MultisampleState::default(),
        multiview_mask: None,
        cache: None,
    })
}

impl ShapePass {
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("shape"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../shaders/shape.wgsl").into()),
        });
        let globals = std::array::from_fn(|_| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("shape globals"),
                size: (GLOBALS * 4) as u64,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            })
        });
        let storage = wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Storage { read_only: true },
            has_dynamic_offset: false,
            min_binding_size: None,
        };
        let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("shape globals"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    // Fragment too: the star field reads the playhead out of
                    // the globals to work out where in a twinkle it is.
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: storage,
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: storage,
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: storage,
                    count: None,
                },
            ],
        });
        let verts_capacity = 256;
        let verts = Self::make_verts_buffer(device, verts_capacity);
        let models_capacity = 256;
        let models = Self::make_models_buffer(device, models_capacity);
        let clocks = Self::make_clocks_buffer(device, models_capacity);
        let bind_groups = Self::make_bind_groups(device, &bgl, &globals, &verts, &models, &clocks);
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("shape"),
            bind_group_layouts: &[&bgl],
            ..Default::default()
        });
        let pipeline = make_pipeline(device, &layout, &shader, format, "shape", depth::test_only());
        let pipeline_over =
            make_pipeline(device, &layout, &shader, format, "shape over", depth::always());
        let capacity = 256;
        let instances = Self::make_instance_buffer(device, capacity);
        Self {
            pipeline,
            pipeline_over,
            globals,
            bgl,
            bind_groups,
            instances,
            capacity,
            verts,
            verts_capacity,
            models,
            models_capacity,
            clocks,
            scratch_depth: None,
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

    fn make_verts_buffer(device: &wgpu::Device, capacity: usize) -> wgpu::Buffer {
        device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("path verts"),
            size: (capacity * size_of::<[f32; 2]>()) as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        })
    }

    fn make_models_buffer(device: &wgpu::Device, capacity: usize) -> wgpu::Buffer {
        device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("shape models"),
            size: (capacity * size_of::<Mat4>()) as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        })
    }

    fn make_clocks_buffer(device: &wgpu::Device, capacity: usize) -> wgpu::Buffer {
        device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("shape clocks"),
            size: (capacity * size_of::<f32>()) as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        })
    }

    fn make_bind_groups(
        device: &wgpu::Device,
        bgl: &wgpu::BindGroupLayout,
        globals: &[wgpu::Buffer; LAYERS],
        verts: &wgpu::Buffer,
        models: &wgpu::Buffer,
        clocks: &wgpu::Buffer,
    ) -> [wgpu::BindGroup; LAYERS] {
        std::array::from_fn(|i| {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("shape globals"),
                layout: bgl,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: globals[i].as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: verts.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: models.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: clocks.as_entire_binding(),
                    },
                ],
            })
        })
    }

    /// A depth attachment sized to `resolution`, made on first use.
    fn scratch_depth(&mut self, device: &wgpu::Device, resolution: (u32, u32)) -> wgpu::TextureView {
        if self.scratch_depth.as_ref().is_none_or(|(s, _)| *s != resolution) {
            self.scratch_depth = Some((resolution, depth::make(device, resolution)));
        }
        self.scratch_depth.as_ref().expect("made above").1.clone()
    }

    /// Draw a whole scene — bodies and halos together, at the target's
    /// resolution, sorted back to front. The stage uses
    /// [`ShapePass::draw_layer`] instead.
    #[allow(clippy::too_many_arguments)]
    pub fn draw(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        scene: &Scene,
        resolution: (u32, u32),
        framing: Framing,
    ) {
        let (shapes, models, clocks) = scene.sorted();
        let sorted = Scene {
            shapes: &shapes,
            models: &models,
            clocks: &clocks,
            ..*scene
        };
        let depth = self.scratch_depth(device, resolution);
        depth::clear(encoder, &depth);
        self.draw_layer(
            device,
            queue,
            encoder,
            view,
            &depth,
            Layer::Full,
            framing.frame_scale(scene.camera),
            &sorted,
            resolution,
            framing,
        );
    }

    /// Draw one layer of every shape, in the order given — the caller
    /// sorts (see [`Scene::sorted`]). `frame_scale` is the *frame's* px
    /// per canvas unit — `cview.0` may be a reduced-resolution target's —
    /// so a halo is judged small by how it will look, not by how it is
    /// being drawn, and the bodies pass and the halos pass agree on it.
    /// `depth` is tested, never written; the caller clears it.
    #[allow(clippy::too_many_arguments)]
    pub fn draw_layer(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        depth: &wgpu::TextureView,
        layer: Layer,
        frame_scale: f32,
        scene: &Scene,
        resolution: (u32, u32),
        framing: Framing,
    ) {
        let shapes = scene.shapes;
        if shapes.len() > self.capacity {
            self.capacity = shapes.len().next_power_of_two();
            self.instances = Self::make_instance_buffer(device, self.capacity);
        }
        let mut rebind = false;
        if scene.paths.len() > self.verts_capacity {
            self.verts_capacity = scene.paths.len().next_power_of_two();
            self.verts = Self::make_verts_buffer(device, self.verts_capacity);
            rebind = true;
        }
        if shapes.len() > self.models_capacity {
            self.models_capacity = shapes.len().next_power_of_two();
            self.models = Self::make_models_buffer(device, self.models_capacity);
            self.clocks = Self::make_clocks_buffer(device, self.models_capacity);
            rebind = true;
        }
        if rebind {
            self.bind_groups = Self::make_bind_groups(
                device,
                &self.bgl,
                &self.globals,
                &self.verts,
                &self.models,
                &self.clocks,
            );
        }
        if !scene.paths.is_empty() {
            queue.write_buffer(&self.verts, 0, bytemuck::cast_slice(scene.paths));
        }
        if !shapes.is_empty() {
            let models: Vec<Mat4> = (0..shapes.len()).map(|i| scene.model(i)).collect();
            queue.write_buffer(&self.models, 0, bytemuck::cast_slice(&models));
            let clocks: Vec<f32> = (0..shapes.len()).map(|i| scene.clock(i)).collect();
            queue.write_buffer(&self.clocks, 0, bytemuck::cast_slice(&clocks));
            queue.write_buffer(&self.instances, 0, bytemuck::cast_slice(shapes));
        }
        let mut globals = [0.0f32; GLOBALS];
        globals[..16].copy_from_slice(&framing.view_proj(scene.camera, resolution).0);
        // The fourth slot is the canvas's width: a star field's density is
        // stars across the canvas, and the shader has to know how wide
        // that is now that comps come in more than one size.
        globals[16..].copy_from_slice(&[
            scene.time,
            layer as u32 as f32,
            frame_scale,
            scene.camera.canvas[0],
        ]);
        let slot = layer as usize;
        queue.write_buffer(&self.globals[slot], 0, bytemuck::cast_slice(&globals));

        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("shapes"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    // The backdrop pass painted the frame's base coat (gutter
                    // + checkerboard); shapes composite over it. The document
                    // itself has no background — transparency is real.
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(depth::attachment(depth, wgpu::LoadOp::Load)),
            ..Default::default()
        });
        // Clip to the stage ∩ the clip region: nothing (not even glow)
        // paints outside the canvas, and a zoomed-in canvas never bleeds
        // over the chrome around its panel.
        let Some((x, y, w, h)) = framing.paint_rect(scene.camera, resolution) else {
            return;
        };
        pass.set_scissor_rect(x, y, w, h);
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.bind_groups[slot], &[]);
        pass.set_vertex_buffer(0, self.instances.slice(..));
        let n = shapes.len() as u32;
        let over = (scene.over as u32).min(n);
        pass.draw(0..4, 0..n - over);
        if over > 0 {
            // The marks over everything: the same instances, drawn last
            // through the pipeline that never asks the depth buffer.
            pass.set_pipeline(&self.pipeline_over);
            pass.draw(0..4, n - over..n);
        }
    }
}

#[cfg(test)]
mod order_tests {
    use super::*;

    /// Back to front by depth — except the marks drawn over everything,
    /// which keep to the tail, sorted among themselves.
    #[test]
    fn the_marks_over_everything_sort_last() {
        let at = |z: f32| Mat4::translation(Vec3::new(0.0, 0.0, z));
        let shapes = [
            Shape::circle([100.0, 100.0], 10.0),
            Shape::circle([200.0, 100.0], 10.0),
            Shape::circle([300.0, 100.0], 10.0),
            Shape::circle([400.0, 100.0], 10.0),
        ];
        // A: on the canvas; B: nearer; C, D: marks, C far behind, D nearest.
        let models = [at(0.0), at(200.0), at(-500.0), at(400.0)];
        let camera = Camera::stage(crate::shapes::CANVAS);
        let scene = Scene {
            shapes: &shapes,
            models: &models,
            paths: &[],
            meshes: &[],
            lights: &[],
            camera: &camera,
            time: 0.0,
            clocks: &[],
            over: 2,
        };
        let xs = |s: &[Shape]| s.iter().map(|s| s.center()[0]).collect::<Vec<_>>();
        let (sorted, sorted_models, _) = scene.sorted();
        assert_eq!(xs(&sorted), vec![100.0, 200.0, 300.0, 400.0]);
        assert_eq!(sorted_models[2], at(-500.0));
        // Without the split, the far mark is drawn first of all.
        let plain = Scene { over: 0, ..scene };
        let (sorted, _, _) = plain.sorted();
        assert_eq!(xs(&sorted), vec![300.0, 100.0, 200.0, 400.0]);
        // More marks than shapes is every shape a mark, not a panic.
        let all = Scene { over: 9, ..scene };
        assert_eq!(all.sorted().0.len(), 4);
    }
}
