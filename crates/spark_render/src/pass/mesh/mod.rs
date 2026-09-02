//! The mesh pass: the scene's meshes — the opaque ones first.
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
//! Lighting is the scene's lights (`crate::light`) — or the default sun
//! when it has none — plus ambient and a rim, with suns and spots casting
//! shadows through the maps in `shadow`.
//!
//! Only the *opaque* meshes are drawn here first. A mesh with an opacity
//! under one is see-through: it sorts into the stack among the shapes
//! (`super::stack`) and is drawn in its turn by `translucent`, over what
//! is behind it and under what is in front, testing the depth the opaque
//! ones wrote and never writing it. At opacity zero a mesh draws nothing
//! and casts nothing.

mod instance;
mod resolve;
mod shadow;
#[cfg(test)]
mod tests;
mod translucent;
#[cfg(test)]
mod translucent_tests;
mod upload;

pub use instance::MeshInstance;
pub(crate) use instance::MeshKey;
pub use upload::{GpuMesh, MeshData, TextureData};

use super::{Scene, depth};
use crate::camera::Framing;
use crate::light::{self, Light, LightsUniform};
use crate::math::Mat4;

/// Multisampling on the opaque targets.
pub const SAMPLES: u32 = 4;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct InstanceData {
    model: [f32; 16],
    normal: [f32; 16],
    color: [f32; 4],
    material: [f32; 4],
}

/// Floats in the globals uniform: view_proj, eye + ambient, then the rim
/// strength and three spare.
const GLOBALS: usize = 24;

/// The mesh pipeline with a depth state, multisampled, premultiplied-over
/// onto `format` — or, with `colour` off, writing no colour at all: the
/// depth prepass, which has to name the same target as the pass it runs
/// in, and the cheapest fragment stage that can.
fn make_pipeline(
    device: &wgpu::Device,
    layout: &wgpu::PipelineLayout,
    shader: &wgpu::ShaderModule,
    format: wgpu::TextureFormat,
    label: &str,
    depth_stencil: wgpu::DepthStencilState,
    colour: bool,
) -> wgpu::RenderPipeline {
    let over = wgpu::BlendComponent {
        src_factor: wgpu::BlendFactor::One,
        dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
        operation: wgpu::BlendOperation::Add,
    };
    let targets = [Some(wgpu::ColorTargetState {
        format,
        blend: Some(wgpu::BlendState {
            color: over,
            alpha: over,
        }),
        write_mask: if colour {
            wgpu::ColorWrites::ALL
        } else {
            wgpu::ColorWrites::empty()
        },
    })];
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(label),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some("vs_main"),
            compilation_options: Default::default(),
            buffers: &[upload::Vertex::layout()],
        },
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some(if colour { "fs_main" } else { "fs_depth" }),
            compilation_options: Default::default(),
            targets: &targets,
        }),
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            // Both sides draw: a logo is a plaque, and the shader lights
            // whichever side faces the camera.
            cull_mode: None,
            ..Default::default()
        },
        depth_stencil: Some(depth_stencil),
        multisample: wgpu::MultisampleState {
            count: SAMPLES,
            mask: !0,
            alpha_to_coverage_enabled: false,
        },
        multiview_mask: None,
        cache: None,
    })
}


/// The multisampled pair the meshes draw into, sized to the stage.
struct Targets {
    size: (u32, u32),
    color: wgpu::TextureView,
    depth: wgpu::TextureView,
    /// Depth-resolve bind groups for a ratio of 1 and of 2.
    resolve: [wgpu::BindGroup; 2],
}

pub struct MeshPass {
    /// The opaque meshes: lit, writing depth.
    pipeline: wgpu::RenderPipeline,
    /// A see-through mesh's own nearest surface into the depth buffer,
    /// no colour — what its colour pass then draws and nothing deeper.
    prepass: wgpu::RenderPipeline,
    /// A see-through mesh's colour: lit the same, testing depth only.
    translucent: wgpu::RenderPipeline,
    pipeline_format: wgpu::TextureFormat,
    resolve: resolve::Resolve,
    globals: wgpu::Buffer,
    lights: wgpu::Buffer,
    instances: wgpu::Buffer,
    capacity: usize,
    bgl: wgpu::BindGroupLayout,
    bind_group: wgpu::BindGroup,
    texture_bgl: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    /// A 1×1 white texture: what an untextured mesh samples.
    white: wgpu::BindGroup,
    targets: Option<Targets>,
    shadows: shadow::ShadowMaps,
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
        let lights = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("mesh lights"),
            size: size_of::<LightsUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let capacity = 16;
        let instances = Self::make_instances(device, capacity);
        let uniform = wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Uniform,
            has_dynamic_offset: false,
            min_binding_size: None,
        };
        let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("mesh globals"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: uniform,
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
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: uniform,
                    count: None,
                },
                // The shadow maps: their matrices, the depth array, and
                // the comparison sampler that reads it.
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: uniform,
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Depth,
                        view_dimension: wgpu::TextureViewDimension::D2Array,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 5,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Comparison),
                    count: None,
                },
            ],
        });
        let shadows = shadow::ShadowMaps::new(device, &instances);
        let bind_group = Self::make_bind_group(device, &bgl, &globals, &instances, &lights, &shadows);
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
        let writes = |compare: wgpu::CompareFunction, write: bool| wgpu::DepthStencilState {
            format: depth::FORMAT,
            depth_write_enabled: write,
            depth_compare: compare,
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState::default(),
        };
        let pipeline = make_pipeline(
            device,
            &layout,
            &shader,
            format,
            "mesh",
            writes(wgpu::CompareFunction::Less, true),
            true,
        );
        let prepass = make_pipeline(
            device,
            &layout,
            &shader,
            format,
            "mesh prepass",
            writes(wgpu::CompareFunction::Less, true),
            false,
        );
        // LessEqual: the prepass put this very surface in the buffer.
        let translucent = make_pipeline(
            device,
            &layout,
            &shader,
            format,
            "mesh see-through",
            writes(wgpu::CompareFunction::LessEqual, false),
            true,
        );

        let resolve = resolve::Resolve::new(device);
        Self {
            pipeline,
            prepass,
            translucent,
            pipeline_format: format,
            resolve,
            globals,
            lights,
            instances,
            capacity,
            bgl,
            bind_group,
            texture_bgl,
            sampler,
            white,
            targets: None,
            shadows,
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
        lights: &wgpu::Buffer,
        shadows: &shadow::ShadowMaps,
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
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: lights.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: shadows.matrices.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::TextureView(&shadows.array),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: wgpu::BindingResource::Sampler(&shadows.sampler),
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
                layout: &self.resolve.bgl,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&depth),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: self.resolve.ratios[i].as_entire_binding(),
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

    /// Draw every opaque mesh in `scene`: into the multisampled pair,
    /// colour resolved into `color`, depth resolved into each of `depths`
    /// — `(view, div)` being the stage's own attachment at 1 and the halo
    /// layer's at its divisor — all within the canvas footprint of
    /// `cview` ∩ `clip` on a `resolution`-sized target. Every mesh is
    /// uploaded and lit and casts, so the see-through ones can follow
    /// through `draw_translucent` against the depth this leaves.
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
        framing: Framing,
    ) {
        let Some(rect) = framing.paint_rect(scene.camera, resolution) else {
            return;
        };
        if self.targets.as_ref().is_none_or(|t| t.size != resolution) {
            self.targets = Some(self.make_targets(device, resolution));
        }
        let n = scene.meshes.len();
        if n > self.capacity {
            self.capacity = n.next_power_of_two();
            self.instances = Self::make_instances(device, self.capacity);
            self.shadows.rebind(device, &self.instances);
            self.bind_group = Self::make_bind_group(
                device,
                &self.bgl,
                &self.globals,
                &self.instances,
                &self.lights,
                &self.shadows,
            );
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
        let mut g = [0.0f32; GLOBALS];
        g[..16].copy_from_slice(&framing.view_proj(cam, resolution).0);
        g[16..20].copy_from_slice(&[cam.eye.x, cam.eye.y, cam.eye.z, Light::DEFAULT_AMBIENT]);
        g[20] = Light::DEFAULT_RIM;
        queue.write_buffer(&self.globals, 0, bytemuck::cast_slice(&g));
        // The lights, resolved (the default sun added when none comes
        // from somewhere), each told which shadow map is its own; then
        // the maps themselves, before anything is lit by them.
        let resolved = light::resolve(scene.lights);
        let plan = shadow::plan(&resolved, shadow::scene_bounds(scene.meshes));
        let mut slots = vec![-1i32; resolved.len()];
        for (slot, (li, _)) in plan.iter().enumerate() {
            slots[*li] = slot as i32;
        }
        queue.write_buffer(
            &self.lights,
            0,
            bytemuck::bytes_of(&LightsUniform::pack_resolved(&resolved, &slots)),
        );
        self.shadows.render(queue, encoder, &plan, scene.meshes);
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
                if !(m.visible() && m.opaque()) {
                    continue;
                }
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
            pass.set_pipeline(&self.resolve.pipeline);
            pass.set_bind_group(0, &t.resolve[if d >= 2 { 1 } else { 0 }], &[]);
            pass.draw(0..3, 0..1);
        }
    }
}
