//! The stage cache: the document's picture, kept between frames.
//!
//! A redraw used to run the shape pass straight into the swapchain, every
//! time — and a redraw is what *any* event asks for, including the cursor
//! crossing a layer card. With seventy glowing shapes each shading a
//! near-canvas-sized quad, hovering the right panel re-lit the whole stage
//! at 4K on every mouse move, which is how a GPU ends up at full throttle
//! with nothing on screen changing.
//!
//! The stage now renders into its own texture and the frame is composited
//! from that. The texture is redrawn only when something the shape pass
//! reads has changed — and "something" is decided by comparing *every*
//! input it takes, not by a dirty flag some edit path might forget to set.
//! A hit costs one window-sized blit; a miss costs what a frame always did.
//!
//! Nothing about the picture changes. The shape pass composites
//! premultiplied-over, which is associative: drawing the stack onto a
//! transparent texture and that over the backdrop is the same arithmetic as
//! drawing the stack onto the backdrop. `tests::the_stage_is_the_live_frame`
//! reads both back and holds them to a byte.

use super::{ShapePass, paint_rect};
use crate::geom::Viewport;
use crate::shapes::Shape;

/// Everything `ShapePass::draw` reads. Equal inputs, equal picture.
#[derive(Clone, PartialEq)]
struct Key {
    shapes: Vec<Shape>,
    paths: Vec<[f32; 2]>,
    resolution: (u32, u32),
    cview: (f32, f32, f32),
    time: f32,
    clip: Viewport,
}

pub struct Stage {
    format: wgpu::TextureFormat,
    pipeline: wgpu::RenderPipeline,
    bgl: wgpu::BindGroupLayout,
    /// The texture, sized to the window; made on first use and remade on
    /// resize.
    target: Option<Target>,
    /// What the texture currently shows.
    held: Option<Key>,
}

struct Target {
    view: wgpu::TextureView,
    bind_group: wgpu::BindGroup,
    size: (u32, u32),
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
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: false },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            }],
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("stage blit"),
            bind_group_layouts: &[&bgl],
            ..Default::default()
        });
        // The same premultiplied-over the shape pass uses, so the cached
        // stack lands on the backdrop exactly as a live one would.
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
            target: None,
            held: None,
        }
    }

    fn make_target(&self, device: &wgpu::Device, size: (u32, u32)) -> Target {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("stage"),
            size: wgpu::Extent3d {
                width: size.0.max(1),
                height: size.1.max(1),
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
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("stage"),
            layout: &self.bgl,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&view),
            }],
        });
        Target {
            view,
            bind_group,
            size,
        }
    }

    /// Forget the held picture: the next `draw` re-renders whatever it is
    /// given. Nothing in the app needs this — every input is in the key —
    /// but a cache you can't drop by hand is a cache you can't debug.
    pub fn invalidate(&mut self) {
        self.held = None;
    }

    /// Composite the document onto `target`, re-rendering the stage first
    /// only when the inputs differ from what it holds. The parameters are
    /// `ShapePass::draw`'s, passed straight through. Returns whether the
    /// shape pass actually ran — for tests, and for a status readout.
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
    ) -> bool {
        if self.target.as_ref().is_none_or(|t| t.size != resolution) {
            self.target = Some(self.make_target(device, resolution));
            self.held = None;
        }
        let stage = self.target.as_ref().expect("made above");
        let fresh = self.held.as_ref().is_none_or(|k| {
            k.shapes != shapes
                || k.paths != path_verts
                || k.resolution != resolution
                || k.cview != cview
                || k.time != time
                || k.clip != clip
        });
        if fresh {
            // Transparent, not black: the stage composites over the
            // backdrop later, and the document has no background of its
            // own.
            encoder
                .begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("stage clear"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &stage.view,
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
            pass.draw(
                device,
                queue,
                encoder,
                &stage.view,
                shapes,
                path_verts,
                resolution,
                cview,
                time,
                clip,
            );
            self.held = Some(Key {
                shapes: shapes.to_vec(),
                paths: path_verts.to_vec(),
                resolution,
                cview,
                time,
                clip,
            });
        }
        let Some((x, y, w, h)) = paint_rect(resolution, cview, clip) else {
            return fresh;
        };
        let mut blit = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("stage blit"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            ..Default::default()
        });
        blit.set_scissor_rect(x, y, w, h);
        blit.set_pipeline(&self.pipeline);
        blit.set_bind_group(0, &stage.bind_group, &[]);
        blit.draw(0..3, 0..1);
        fresh
    }
}
