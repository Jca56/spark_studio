//! The instanced shape render pass: buffers, pipeline, scissored draw.

use crate::geom::Viewport;
use crate::shapes::{CANVAS_H, CANVAS_W, Shape};

pub struct ShapePass {
    pipeline: wgpu::RenderPipeline,
    globals: wgpu::Buffer,
    bgl: wgpu::BindGroupLayout,
    bind_group: wgpu::BindGroup,
    instances: wgpu::Buffer,
    capacity: usize,
    /// Path vertex pool (canvas units, center-relative), flat per frame.
    verts: wgpu::Buffer,
    verts_capacity: usize,
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
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
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
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });
        let verts_capacity = 256;
        let verts = Self::make_verts_buffer(device, verts_capacity);
        let bind_group = Self::make_bind_group(device, &bgl, &globals, &verts);
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
            bgl,
            bind_group,
            instances,
            capacity,
            verts,
            verts_capacity,
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

    fn make_bind_group(
        device: &wgpu::Device,
        bgl: &wgpu::BindGroupLayout,
        globals: &wgpu::Buffer,
        verts: &wgpu::Buffer,
    ) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("shape globals"),
            layout: bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: globals.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: verts.as_entire_binding(),
                },
            ],
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub fn draw(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        shapes: &[Shape],
        path_verts: &[[f32; 2]],
        resolution: (u32, u32),
        viewport: Viewport,
        clear: wgpu::Color,
    ) {
        if shapes.len() > self.capacity {
            self.capacity = shapes.len().next_power_of_two();
            self.instances = Self::make_instance_buffer(device, self.capacity);
        }
        if path_verts.len() > self.verts_capacity {
            self.verts_capacity = path_verts.len().next_power_of_two();
            self.verts = Self::make_verts_buffer(device, self.verts_capacity);
            self.bind_group = Self::make_bind_group(device, &self.bgl, &self.globals, &self.verts);
        }
        if !path_verts.is_empty() {
            queue.write_buffer(&self.verts, 0, bytemuck::cast_slice(path_verts));
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
