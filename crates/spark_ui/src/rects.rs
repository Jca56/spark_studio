//! Flat rectangle + icon glyph renderer: instanced quads in window pixels,
//! standard alpha blending. The base coat of all SparkUI chrome.
//!
//! An instance is either a solid fill (`icon.x == 0`) or an SDF icon glyph
//! rendered centered in the quad (minus / square outline / X).

use spark_render::{Viewport, wgpu};

pub const ICON_NONE: f32 = 0.0;
pub const ICON_MINUS: f32 = 1.0;
pub const ICON_SQUARE: f32 = 2.0;
pub const ICON_X: f32 = 3.0;
pub const ICON_ARROW: f32 = 4.0;
pub const ICON_CIRCLE: f32 = 5.0;
pub const ICON_PENTAGON: f32 = 6.0;
pub const ICON_LINE: f32 = 7.0;
/// Samples the pass's bound image texture (tinted by `color`).
pub const ICON_IMAGE: f32 = 8.0;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct UiRect {
    pub pos: [f32; 2],
    pub size: [f32; 2],
    pub color: [f32; 4],
    /// [kind, stroke thickness px, corner radius px (fills only), glyph radius factor]
    pub icon: [f32; 4],
    /// Gradient end color for fills; alpha 0 = solid `color`. The gradient
    /// runs left→right across the instance quad.
    pub color2: [f32; 4],
}

impl UiRect {
    pub fn region(v: Viewport, color: [f32; 4]) -> Self {
        Self {
            pos: [v.x, v.y],
            size: [v.w, v.h],
            color,
            icon: [ICON_NONE; 4],
            color2: [0.0; 4],
        }
    }

    pub fn region_rounded(v: Viewport, color: [f32; 4], radius: f32) -> Self {
        Self {
            pos: [v.x, v.y],
            size: [v.w, v.h],
            color,
            icon: [ICON_NONE, 0.0, radius, 0.0],
            color2: [0.0; 4],
        }
    }

    /// Rounded fill with a left→right gradient from `color` to `color2`.
    pub fn region_rounded_gradient(
        v: Viewport,
        color: [f32; 4],
        color2: [f32; 4],
        radius: f32,
    ) -> Self {
        Self {
            pos: [v.x, v.y],
            size: [v.w, v.h],
            color,
            icon: [ICON_NONE, 0.0, radius, 0.0],
            color2,
        }
    }

    pub fn icon(v: Viewport, kind: f32, thickness: f32, color: [f32; 4]) -> Self {
        Self {
            pos: [v.x, v.y],
            size: [v.w, v.h],
            color,
            icon: [kind, thickness, 0.0, 0.0],
            color2: [0.0; 4],
        }
    }

    /// Like [`UiRect::icon`] with an explicit glyph radius factor (fraction
    /// of the quad's short side; the default is 0.20).
    pub fn icon_sized(
        v: Viewport,
        kind: f32,
        thickness: f32,
        color: [f32; 4],
        radius_factor: f32,
    ) -> Self {
        Self {
            pos: [v.x, v.y],
            size: [v.w, v.h],
            color,
            icon: [kind, thickness, 0.0, radius_factor],
            color2: [0.0; 4],
        }
    }
}

pub struct UiPass {
    pipeline: wgpu::RenderPipeline,
    globals: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    image_bind_group: wgpu::BindGroup,
    instances: wgpu::Buffer,
    capacity: usize,
}

impl UiPass {
    /// `image` is raw RGBA (sRGB) pixels for the pass's image texture — a
    /// square of `image_dim` × `image_dim` (the app icon, for now; a real
    /// image atlas later).
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        format: wgpu::TextureFormat,
        image: &[u8],
        image_dim: u32,
    ) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("ui"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/ui.wgsl").into()),
        });
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("ui image"),
            size: wgpu::Extent3d {
                width: image_dim,
                height: image_dim,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            image,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(image_dim * 4),
                rows_per_image: Some(image_dim),
            },
            wgpu::Extent3d {
                width: image_dim,
                height: image_dim,
                depth_or_array_layers: 1,
            },
        );
        let tex_view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("ui image"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let globals = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("ui globals"),
            size: 16,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("ui globals"),
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
            label: Some("ui globals"),
            layout: &bgl,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: globals.as_entire_binding(),
            }],
        });
        let image_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("ui image"),
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
        let image_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("ui image"),
            layout: &image_bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&tex_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("ui"),
            bind_group_layouts: &[&bgl, &image_bgl],
            ..Default::default()
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("ui"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: size_of::<UiRect>() as u64,
                    step_mode: wgpu::VertexStepMode::Instance,
                    attributes: &wgpu::vertex_attr_array![
                        0 => Float32x2,
                        1 => Float32x2,
                        2 => Float32x4,
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
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
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
            image_bind_group,
            instances,
            capacity,
        }
    }

    fn make_instance_buffer(device: &wgpu::Device, capacity: usize) -> wgpu::Buffer {
        device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("ui instances"),
            size: (capacity * size_of::<UiRect>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        })
    }

    /// Draw rects over whatever is already in `view` (LoadOp::Load).
    pub fn draw(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        rects: &[UiRect],
        resolution: (u32, u32),
    ) {
        if rects.is_empty() {
            return;
        }
        if rects.len() > self.capacity {
            self.capacity = rects.len().next_power_of_two();
            self.instances = Self::make_instance_buffer(device, self.capacity);
        }
        let globals = [resolution.0 as f32, resolution.1 as f32, 0.0, 0.0];
        queue.write_buffer(&self.globals, 0, bytemuck::cast_slice(&globals));
        queue.write_buffer(&self.instances, 0, bytemuck::cast_slice(rects));

        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("ui"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            ..Default::default()
        });
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.bind_group, &[]);
        pass.set_bind_group(1, &self.image_bind_group, &[]);
        pass.set_vertex_buffer(0, self.instances.slice(..));
        pass.draw(0..4, 0..rects.len() as u32);
    }
}
