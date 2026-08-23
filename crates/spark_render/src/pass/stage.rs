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

use super::{Layer, ShapePass, paint_rect};
use crate::geom::Viewport;
use crate::shapes::Shape;

/// Halos render at the stage's resolution over this, per axis.
const HALO_DIV: u32 = 2;
/// The stage's own resolution divisor in half-resolution playback.
const PREVIEW_DIV: u32 = 2;

/// Everything that decides the picture. Equal inputs, equal texture.
#[derive(Clone, PartialEq)]
struct Key {
    shapes: Vec<Shape>,
    paths: Vec<[f32; 2]>,
    resolution: (u32, u32),
    cview: (f32, f32, f32),
    time: f32,
    clip: Viewport,
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
    /// What the stage texture currently shows.
    held: Option<Key>,
}

/// A render target that knows what it will be blitted onto.
struct Target {
    view: wgpu::TextureView,
    bind_group: wgpu::BindGroup,
    size: (u32, u32),
    /// The size of the texture this one is composited onto — the frame
    /// for the stage, the stage for the halo layer. Baked into the blit's
    /// uniform, so the bind group is made once per size.
    onto: (u32, u32),
}

/// A view transform and clip region for a target `div` times smaller than
/// the frame.
fn reduced(cview: (f32, f32, f32), clip: Viewport, div: u32) -> ((f32, f32, f32), Viewport) {
    let d = div as f32;
    (
        (cview.0 / d, cview.1 / d, cview.2 / d),
        Viewport {
            x: clip.x / d,
            y: clip.y / d,
            w: clip.w / d,
            h: clip.h / d,
        },
    )
}

fn over(size: (u32, u32), div: u32) -> (u32, u32) {
    (size.0.div_ceil(div).max(1), size.1.div_ceil(div).max(1))
}

impl Stage {
    /// `format` is the swapchain's: the shape pipeline was built for it,
    /// and the stage has to be drawable by that pipeline.
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
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
            held: None,
        }
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
    /// only when the inputs differ from what it holds. The shape parameters
    /// are `ShapePass::draw`'s, passed straight through; `preview` renders
    /// the stage at half resolution. Returns whether the shape pass ran —
    /// for tests, and for a status readout.
    #[allow(clippy::too_many_arguments)]
    pub fn draw(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        pass: &mut ShapePass,
        shapes: &[Shape],
        path_verts: &[[f32; 2]],
        resolution: (u32, u32),
        cview: (f32, f32, f32),
        time: f32,
        clip: Viewport,
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
        let (stage, halo) = (
            self.stage.as_ref().expect("made above"),
            self.halo.as_ref().expect("made above"),
        );
        let fresh = self.held.as_ref().is_none_or(|k| {
            k.shapes != shapes
                || k.paths != path_verts
                || k.resolution != resolution
                || k.cview != cview
                || k.time != time
                || k.clip != clip
                || k.div != div
        });
        if fresh {
            // Bodies, at the stage's resolution.
            let (sv, sclip) = reduced(cview, clip, div);
            clear(encoder, &stage.view);
            pass.draw_layer(
                device,
                queue,
                encoder,
                &stage.view,
                Layer::Bodies,
                cview.0,
                shapes,
                path_verts,
                stage_size,
                sv,
                time,
                sclip,
            );
            // Halos, at half that, then brought up and added onto the bodies.
            let (hv, hclip) = reduced(cview, clip, div * HALO_DIV);
            clear(encoder, &halo.view);
            pass.draw_layer(
                device,
                queue,
                encoder,
                &halo.view,
                Layer::Halos,
                cview.0,
                shapes,
                path_verts,
                halo_size,
                hv,
                time,
                hclip,
            );
            if let Some(rect) = paint_rect(stage_size, sv, sclip) {
                self.blit(encoder, halo, &stage.view, rect);
            }
            self.held = Some(Key {
                shapes: shapes.to_vec(),
                paths: path_verts.to_vec(),
                resolution,
                cview,
                time,
                clip,
                div,
            });
        }
        if let Some(rect) = paint_rect(resolution, cview, clip) {
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
