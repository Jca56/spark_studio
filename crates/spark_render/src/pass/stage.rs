//! The stage: the document's picture, built in layers and kept between
//! frames.
//!
//! A redraw used to run the shape pass straight into the swapchain, every
//! time. Two things made that expensive. A redraw is what *any* event asks
//! for, including the cursor crossing a layer card, so nothing changing
//! still re-lit the whole canvas. And every shape's quad reaches four glow
//! radii past its body, so a glowing shape is a near-canvas-sized quad and
//! seventy of them are seventy full-viewport fragment passes, each sampling
//! a smooth exponential at 4K — with a song playing, on every frame.
//!
//! The stage answers both. The picture is rendered into its own texture
//! and composited onto the frame; the texture is redrawn only when
//! something the shape pass reads has changed — decided by comparing
//! *every* input it takes, not by a dirty flag some edit path might forget
//! to set. And the picture is drawn in two layers: **bodies** at the frame's
//! resolution in quads that hug them, and **halos** at half resolution in
//! the wide quads a halo needs, brought up bilinearly and added on top. A
//! halo is light spilling off a body, and it is the same smooth falloff at
//! any sampling rate; the split is decided per shape in the shader, and a
//! halo only a few pixels wide stays with its body where it is cheap and
//! would otherwise go soft.
//!
//! What changes in the picture: a halo now lies over *every* body, where it
//! used to be hidden by bodies drawn in front of its own. Bloom behaves this
//! way — light spills over what is in front of it — and a deliberate look
//! is the price of the budget. `stage_tests::a_halo_now_spills_over_what_is_in_front`
//! holds the line on purpose.
//!
//! **Half-resolution playback** (`preview`) renders the whole stage at half
//! size and brings it up for the frame — a preview quality every editor
//! offers, for the person who wants the fans quiet more than the edges
//! crisp while the song runs. Off, the paused picture and export are
//! untouched.
//!
//! **The stage is a scene.** Shapes are sorted back to front by their depth
//! along the camera's view before either layer draws them — stably, so a
//! comp that never left the canvas plane stacks in list order exactly as it
//! did — and every target carries a depth attachment for the opaque passes
//! to write and the shape pass to test against. The camera and each
//! object's matrix are inputs like any other, so a moved camera is a cache
//! miss and a hovered card still is not.

use super::mesh::{GpuMesh, MeshData, MeshInstance, MeshKey, MeshPass, TextureData};
use super::{Layer, Scene, ShapePass, depth};
use crate::camera::{Camera, Framing};
use crate::light::Light;
use crate::math::Mat4;
use crate::shapes::Shape;

/// Halos render at the stage's resolution over this, per axis.
const HALO_DIV: u32 = 2;
/// The stage's own resolution divisor in half-resolution playback.
const PREVIEW_DIV: u32 = 2;

/// Everything that decides the picture. Equal inputs, equal texture.
#[derive(Clone, PartialEq)]
struct Key {
    shapes: Vec<Shape>,
    models: Vec<Mat4>,
    paths: Vec<[f32; 2]>,
    meshes: Vec<MeshKey>,
    lights: Vec<Light>,
    camera: Camera,
    resolution: (u32, u32),
    framing: Framing,
    time: f32,
    div: u32,
}

pub struct Stage {
    format: wgpu::TextureFormat,
    pipeline: wgpu::RenderPipeline,
    bgl: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    /// The finished picture, sized to the frame (or half of it in
    /// preview). Made on first use, remade when the size changes.
    stage: Option<Target>,
    /// The halo layer, sized to the stage over `HALO_DIV`.
    halo: Option<Target>,
    /// The meshes' resolved picture, stage-sized, laid down first.
    opaque: Option<Target>,
    meshes: MeshPass,
    /// What the stage texture currently shows.
    held: Option<Key>,
}

/// A render target that knows what it will be blitted onto.
struct Target {
    view: wgpu::TextureView,
    /// Its depth attachment, the same size. Cleared with the colour.
    depth: wgpu::TextureView,
    bind_group: wgpu::BindGroup,
    size: (u32, u32),
    /// The size of the texture this one is composited onto — the frame
    /// for the stage, the stage for the halo layer. Baked into the blit's
    /// uniform, so the bind group is made once per size.
    onto: (u32, u32),
}

fn over(size: (u32, u32), div: u32) -> (u32, u32) {
    (size.0.div_ceil(div).max(1), size.1.div_ceil(div).max(1))
}

impl Stage {
    /// `format` is the swapchain's: the shape pipeline was built for it,
    /// and the stage has to be drawable by that pipeline.
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("stage blit"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../shaders/blit.wgsl").into()),
        });
        let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("stage"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("stage"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("stage blit"),
            bind_group_layouts: &[&bgl],
            ..Default::default()
        });
        // The same premultiplied-over the shape pass uses, so the cached
        // stack lands on the backdrop exactly as a live one would — and a
        // halo layer, alpha 0 throughout, lands as pure added light.
        let over = wgpu::BlendComponent {
            src_factor: wgpu::BlendFactor::One,
            dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
            operation: wgpu::BlendOperation::Add,
        };
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("stage blit"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState {
                        color: over,
                        alpha: over,
                    }),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });
        Self {
            format,
            pipeline,
            bgl,
            sampler,
            stage: None,
            halo: None,
            opaque: None,
            meshes: MeshPass::new(device, queue, format),
            held: None,
        }
    }

    /// Put a mesh on the GPU for [`MeshInstance`]s to draw.
    pub fn upload_mesh(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        data: &MeshData,
        texture: Option<&TextureData>,
    ) -> GpuMesh {
        self.meshes.upload(device, queue, data, texture)
    }

    fn make_target(&self, device: &wgpu::Device, size: (u32, u32), onto: (u32, u32)) -> Target {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("stage"),
            size: wgpu::Extent3d {
                width: size.0,
                height: size.1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: self.format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("stage blit onto"),
            size: 16,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: true,
        });
        uniform
            .slice(..)
            .get_mapped_range_mut()
            .copy_from_slice(bytemuck::cast_slice(&[
                onto.0 as f32,
                onto.1 as f32,
                0.0,
                0.0,
            ]));
        uniform.unmap();
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("stage"),
            layout: &self.bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: uniform.as_entire_binding(),
                },
            ],
        });
        Target {
            view,
            depth: depth::make(device, size),
            bind_group,
            size,
            onto,
        }
    }

    /// Forget the held picture: the next `draw` re-renders whatever it is
    /// given. Nothing in the app needs this — every input is in the key —
    /// but a cache you can't drop by hand is a cache you can't debug.
    pub fn invalidate(&mut self) {
        self.held = None;
    }

    /// Composite the document onto `target`, re-rendering the stage first
    /// only when the inputs differ from what it holds. The scene is
    /// `ShapePass::draw`'s, in any order — the stage sorts it; `preview`
    /// renders the stage at half resolution. Returns whether the shape pass
    /// ran — for tests, and for a status readout.
    #[allow(clippy::too_many_arguments)]
    pub fn draw(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        pass: &mut ShapePass,
        scene: &Scene,
        resolution: (u32, u32),
        framing: Framing,
        preview: bool,
    ) -> bool {
        let div = if preview { PREVIEW_DIV } else { 1 };
        let stage_size = over(resolution, div);
        let halo_size = over(stage_size, HALO_DIV);
        if self
            .stage
            .as_ref()
            .is_none_or(|t| t.size != stage_size || t.onto != resolution)
        {
            self.stage = Some(self.make_target(device, stage_size, resolution));
            self.held = None;
        }
        if self
            .halo
            .as_ref()
            .is_none_or(|t| t.size != halo_size || t.onto != stage_size)
        {
            self.halo = Some(self.make_target(device, halo_size, stage_size));
            self.held = None;
        }
        if self
            .opaque
            .as_ref()
            .is_none_or(|t| t.size != stage_size || t.onto != stage_size)
        {
            self.opaque = Some(self.make_target(device, stage_size, stage_size));
            self.held = None;
        }
        let (stage, halo, opaque) = (
            self.stage.as_ref().expect("made above"),
            self.halo.as_ref().expect("made above"),
            self.opaque.as_ref().expect("made above"),
        );
        let mesh_keys: Vec<MeshKey> = scene.meshes.iter().map(MeshInstance::key).collect();
        let fresh = self.held.as_ref().is_none_or(|k| {
            k.shapes != scene.shapes
                || k.models != scene.models
                || k.paths != scene.paths
                || k.meshes != mesh_keys
                || k.lights != scene.lights
                || k.camera != *scene.camera
                || k.resolution != resolution
                || k.framing != framing
                || k.time != scene.time
                || k.div != div
        });
        if fresh {
            // Back to front, once, for both layers.
            let (shapes, models) = scene.sorted();
            let sorted = Scene {
                shapes: &shapes,
                models: &models,
                ..*scene
            };
            let sf = framing.reduced(div);
            let hf = framing.reduced(div * HALO_DIV);
            clear(encoder, &stage.view);
            depth::clear(encoder, &stage.depth);
            clear(encoder, &halo.view);
            depth::clear(encoder, &halo.depth);
            // Opaque objects first: meshes into their multisampled targets,
            // resolved onto the stage under everything, their depth into
            // both layers' attachments so every shape tests against it.
            if !scene.meshes.is_empty() {
                self.meshes.draw(
                    device,
                    queue,
                    encoder,
                    &opaque.view,
                    &[(&stage.depth, 1), (&halo.depth, HALO_DIV)],
                    scene,
                    stage_size,
                    sf,
                );
                if let Some(rect) = sf.paint_rect(stage_size) {
                    self.blit(encoder, opaque, &stage.view, rect);
                }
            }
            // Bodies, at the stage's resolution.
            pass.draw_layer(
                device,
                queue,
                encoder,
                &stage.view,
                &stage.depth,
                Layer::Bodies,
                framing.frame_scale(),
                &sorted,
                stage_size,
                sf,
            );
            // Halos, at half that, then brought up and added onto the bodies.
            pass.draw_layer(
                device,
                queue,
                encoder,
                &halo.view,
                &halo.depth,
                Layer::Halos,
                framing.frame_scale(),
                &sorted,
                halo_size,
                hf,
            );
            if let Some(rect) = sf.paint_rect(stage_size) {
                self.blit(encoder, halo, &stage.view, rect);
            }
            self.held = Some(Key {
                shapes: scene.shapes.to_vec(),
                models: scene.models.to_vec(),
                paths: scene.paths.to_vec(),
                meshes: mesh_keys,
                lights: scene.lights.to_vec(),
                camera: *scene.camera,
                resolution,
                framing,
                time: scene.time,
                div,
            });
        }
        if let Some(rect) = framing.paint_rect(resolution) {
            self.blit(encoder, stage, target, rect);
        }
        fresh
    }

    fn blit(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        src: &Target,
        onto: &wgpu::TextureView,
        (x, y, w, h): (u32, u32, u32, u32),
    ) {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("stage blit"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: onto,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            ..Default::default()
        });
        pass.set_scissor_rect(x, y, w, h);
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &src.bind_group, &[]);
        pass.draw(0..3, 0..1);
    }
}

/// Transparent, not black: a layer composites over what is under it, and
/// the document has no background of its own.
fn clear(encoder: &mut wgpu::CommandEncoder, view: &wgpu::TextureView) {
    encoder
        .begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("stage clear"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                    store: wgpu::StoreOp::Store,
                },
            })],
            ..Default::default()
        })
        .forget_lifetime();
}
