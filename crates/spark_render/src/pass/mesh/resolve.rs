//! The depth resolve: the multisampled depth the meshes wrote, brought
//! down to a single-sample attachment by a fullscreen pass that writes
//! `frag_depth` — the nearest sample wins. The GPU resolves colour on
//! its own; depth it does not.

use super::depth;

pub(super) struct Resolve {
    pub(super) pipeline: wgpu::RenderPipeline,
    pub(super) bgl: wgpu::BindGroupLayout,
    /// `Params` uniforms for a resolve ratio of 1 and of 2.
    pub(super) ratios: [wgpu::Buffer; 2],
}

impl Resolve {
    pub(super) fn new(device: &wgpu::Device) -> Self {
    // The depth resolve: a fullscreen pass that writes frag_depth.
    let resolve_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("depth resolve"),
        source: wgpu::ShaderSource::Wgsl(
            include_str!("../../shaders/depth_resolve.wgsl").into(),
        ),
    });
    let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
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
        bind_group_layouts: &[&bgl],
        ..Default::default()
    });
    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
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
            bgl,
            ratios,
        }
    }
}
