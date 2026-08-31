//! The mesh pass: the scene's opaque objects.
//!
//! Meshes are the first thing in a comp with a real inside and outside,
//! and they are drawn first: into a 4× multisampled colour target with a
//! multisampled depth buffer, resolved to a plain texture the stage lays
//! down before any shape. The shapes are analytically anti-aliased by
//! their distance fields; a rasterised triangle next to them without
//! multisampling would read as a different kind of picture.
//!
//! The depth the meshes wrote is what the shape passes test against. A
//! multisampled depth buffer can't be resolved by the GPU the way colour
//! is, so a small pass does it — the nearest sample wins — into the
//! stage's single-sample depth attachment, and again at half size into the
//! halo layer's, so a halo behind a mesh no longer glows through it.
//!
//! Lighting is one sun, ambient and a rim: the default a comp gets until
//! it has lights of its own. Opacity multiplies colour and alpha in the
//! resolved picture; the mesh still writes depth at full strength, so a
//! fading mesh hides what is behind it until it is gone — honest, and the
//! one thing a proper fade of solid geometry would need more than this.

#[cfg(test)]
mod tests;
mod upload;

pub use upload::{GpuMesh, MeshData, TextureData};

use super::{Scene, depth, paint_rect};
use crate::geom::Viewport;
use crate::math::{Mat4, Vec3};

/// Multisampling on the opaque targets.
pub const SAMPLES: u32 = 4;

/// One drawing of a mesh: where it is and how it's coloured.
#[derive(Clone, Copy)]
pub struct MeshInstance<'a> {
    pub mesh: &'a GpuMesh,
    /// The mesh's own units → the world.
    pub model: Mat4,
    /// rgb = tint × brightness, a = opacity.
    pub color: [f32; 4],
    /// Draw the colour as is, without lighting.
    pub unlit: bool,
}

/// What the stage cache keys a mesh draw on: everything a draw reads.
#[derive(Clone, PartialEq)]
pub(crate) struct MeshKey {
    id: u64,
    model: Mat4,
    color: [f32; 4],
    unlit: bool,
}

impl MeshInstance<'_> {
    pub(crate) fn key(&self) -> MeshKey {
        MeshKey {
            id: self.mesh.id,
            model: self.model,
            color: self.color,
            unlit: self.unlit,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct InstanceData {
    model: [f32; 16],
    normal: [f32; 16],
    color: [f32; 4],
    material: [f32; 4],
}

/// Floats in the globals uniform: view_proj, eye, sun, sun colour.
const GLOBALS: usize = 28;

/// The default sun: from the upper left, in front of the canvas —
/// travelling right, down and away (-z) — so a face turned toward the
/// camera is lit and a turned edge falls off.
const SUN_DIR: [f32; 3] = [0.3, 0.5, -0.8];
const SUN_INTENSITY: f32 = 1.0;
const AMBIENT: f32 = 0.22;

/// The multisampled pair the meshes draw into, sized to the stage.
struct Targets {
    size: (u32, u32),
    color: wgpu::TextureView,
    depth: wgpu::TextureView,
    /// Depth-resolve bind groups for a ratio of 1 and of 2.
    resolve: [wgpu::BindGroup; 2],
}

pub struct MeshPass {
    pipeline: wgpu::RenderPipeline,
    pipeline_format: wgpu::TextureFormat,
    resolve_pipeline: wgpu::RenderPipeline,
    globals: wgpu::Buffer,
    instances: wgpu::Buffer,
    capacity: usize,
    bgl: wgpu::BindGroupLayout,
    bind_group: wgpu::BindGroup,
    texture_bgl: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    /// A 1×1 white texture: what an untextured mesh samples.
    white: wgpu::BindGroup,
    resolve_bgl: wgpu::BindGroupLayout,
    /// `Params` uniforms for a resolve ratio of 1 and of 2.
    ratios: [wgpu::Buffer; 2],
    targets: Option<Targets>,
    next_id: u64,
}

impl MeshPass {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("mesh"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../../shaders/mesh.wgsl").into()),
        });
        let globals = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("mesh globals"),
            size: (GLOBALS * 4) as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let capacity = 16;
        let instances = Self::make_instances(device, capacity);
        let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("mesh globals"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
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
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });
        let bind_group = Self::make_bind_group(device, &bgl, &globals, &instances);
        let texture_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("mesh texture"),
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
            ],
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("mesh"),
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            anisotropy_clamp: 4,
            ..Default::default()
        });
        let white = upload::texture_bind_group(
            device,
            queue,
            &texture_bgl,
            &sampler,
            &TextureData {
                width: 1,
                height: 1,
                levels: vec![vec![255; 4]],
            },
        );
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("mesh"),
            bind_group_layouts: &[&bgl, &texture_bgl],
            ..Default::default()
        });
        let over = wgpu::BlendComponent {
            src_factor: wgpu::BlendFactor::One,
            dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
            operation: wgpu::BlendOperation::Add,
        };
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("mesh"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[upload::Vertex::layout()],
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
                // Both sides draw: a logo is a plaque, and the shader lights
                // whichever side faces the camera.
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: depth::FORMAT,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::Less,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState {
                count: SAMPLES,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview_mask: None,
            cache: None,
        });

        // The depth resolve: a fullscreen pass that writes frag_depth.
        let resolve_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("depth resolve"),
            source: wgpu::ShaderSource::Wgsl(
                include_str!("../../shaders/depth_resolve.wgsl").into(),
            ),
        });
        let resolve_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("depth resolve"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Depth,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: true,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
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
        let resolve_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("depth resolve"),
            bind_group_layouts: &[&resolve_bgl],
            ..Default::default()
        });
        let resolve_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("depth resolve"),
            layout: Some(&resolve_layout),
            vertex: wgpu::VertexState {
                module: &resolve_shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &resolve_shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: depth::FORMAT,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::Always,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });
        let ratios = [1.0f32, 2.0].map(|r| {
            let b = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("depth resolve ratio"),
                size: 16,
                usage: wgpu::BufferUsages::UNIFORM,
                mapped_at_creation: true,
            });
            b.slice(..)
                .get_mapped_range_mut()
                .copy_from_slice(bytemuck::cast_slice(&[r, r, 0.0, 0.0]));
            b.unmap();
            b
        });
        Self {
            pipeline,
            pipeline_format: format,
            resolve_pipeline,
            globals,
            instances,
            capacity,
            bgl,
            bind_group,
            texture_bgl,
            sampler,
            white,
            resolve_bgl,
            ratios,
            targets: None,
            next_id: 0,
        }
    }

    fn make_instances(device: &wgpu::Device, capacity: usize) -> wgpu::Buffer {
        device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("mesh instances"),
            size: (capacity * size_of::<InstanceData>()) as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        })
    }

    fn make_bind_group(
        device: &wgpu::Device,
        bgl: &wgpu::BindGroupLayout,
        globals: &wgpu::Buffer,
        instances: &wgpu::Buffer,
    ) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("mesh globals"),
            layout: bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: globals.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: instances.as_entire_binding(),
                },
            ],
        })
    }

    fn make_targets(&self, device: &wgpu::Device, size: (u32, u32)) -> Targets {
        let color = device
            .create_texture(&wgpu::TextureDescriptor {
                label: Some("mesh msaa colour"),
                size: wgpu::Extent3d {
                    width: size.0.max(1),
                    height: size.1.max(1),
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: SAMPLES,
                dimension: wgpu::TextureDimension::D2,
                format: self.format(),
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[],
            })
            .create_view(&wgpu::TextureViewDescriptor::default());
        let depth = depth::make_msaa(device, size, SAMPLES);
        let resolve = std::array::from_fn(|i| {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("depth resolve"),
                layout: &self.resolve_bgl,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&depth),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: self.ratios[i].as_entire_binding(),
                    },
                ],
            })
        });
        Targets {
            size,
            color,
            depth,
            resolve,
        }
    }

    /// The colour format the pipeline was built for — the multisampled
    /// target has to match it.
    fn format(&self) -> wgpu::TextureFormat {
        self.pipeline_format
    }

    /// Draw every mesh in `scene`: into the multisampled pair, colour
    /// resolved into `color`, depth resolved into each of `depths` —
    /// `(view, div)` being the stage's own attachment at 1 and the halo
    /// layer's at its divisor — all within the canvas footprint of
    /// `cview` ∩ `clip` on a `resolution`-sized target.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn draw(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        color: &wgpu::TextureView,
        depths: &[(&wgpu::TextureView, u32)],
        scene: &Scene,
        resolution: (u32, u32),
        cview: (f32, f32, f32),
        clip: Viewport,
    ) {
        let Some(rect) = paint_rect(resolution, cview, clip) else {
            return;
        };
        if self.targets.as_ref().is_none_or(|t| t.size != resolution) {
            self.targets = Some(self.make_targets(device, resolution));
        }
        let n = scene.meshes.len();
        if n > self.capacity {
            self.capacity = n.next_power_of_two();
            self.instances = Self::make_instances(device, self.capacity);
            self.bind_group = Self::make_bind_group(device, &self.bgl, &self.globals, &self.instances);
        }
        let data: Vec<InstanceData> = scene
            .meshes
            .iter()
            .map(|m| InstanceData {
                model: m.model.0,
                normal: m.model.inverse().unwrap_or(Mat4::IDENTITY).transpose().0,
                color: m.color,
                material: [if m.unlit { 1.0 } else { 0.0 }, 0.0, 0.0, 0.0],
            })
            .collect();
        if !data.is_empty() {
            queue.write_buffer(&self.instances, 0, bytemuck::cast_slice(&data));
        }
        let cam = scene.camera;
        let sun = Vec3::new(SUN_DIR[0], SUN_DIR[1], SUN_DIR[2]).normalized();
        let mut g = [0.0f32; GLOBALS];
        g[..16].copy_from_slice(&cam.view_proj(resolution, cview).0);
        g[16..20].copy_from_slice(&[cam.eye.x, cam.eye.y, cam.eye.z, 0.0]);
        g[20..24].copy_from_slice(&[sun.x, sun.y, sun.z, SUN_INTENSITY]);
        g[24..28].copy_from_slice(&[1.0, 1.0, 1.0, AMBIENT]);
        queue.write_buffer(&self.globals, 0, bytemuck::cast_slice(&g));
        let t = self.targets.as_ref().expect("made above");
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("meshes"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &t.color,
                    depth_slice: None,
                    resolve_target: Some(color),
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Discard,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &t.depth,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                ..Default::default()
            });
            pass.set_scissor_rect(rect.0, rect.1, rect.2, rect.3);
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &self.bind_group, &[]);
            for (i, m) in scene.meshes.iter().enumerate() {
                pass.set_bind_group(1, &m.mesh.texture, &[]);
                pass.set_vertex_buffer(0, m.mesh.vertices.slice(..));
                pass.set_index_buffer(m.mesh.indices.slice(..), wgpu::IndexFormat::Uint32);
                let i = i as u32;
                pass.draw_indexed(0..m.mesh.index_count, 0, i..i + 1);
            }
        }
        for &(view, div) in depths {
            let d = div.max(1);
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("depth resolve"),
                color_attachments: &[],
                depth_stencil_attachment: Some(depth::attachment(view, wgpu::LoadOp::Load)),
                ..Default::default()
            });
            pass.set_scissor_rect(rect.0 / d, rect.1 / d, (rect.2 / d).max(1), (rect.3 / d).max(1));
            pass.set_pipeline(&self.resolve_pipeline);
            pass.set_bind_group(0, &t.resolve[if d >= 2 { 1 } else { 0 }], &[]);
            pass.draw(0..3, 0..1);
        }
    }
}
