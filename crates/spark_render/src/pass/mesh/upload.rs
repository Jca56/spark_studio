//! Meshes and textures on the GPU: what the pass draws, made once per
//! asset and drawn any number of times.

use super::MeshPass;

/// Mesh geometry as the pass wants it — triangles, in the object's own
/// units, with a normal and a UV per vertex. `spark_assets` produces
/// exactly this from a glTF primitive.
pub struct MeshData {
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    /// Empty when the mesh has none; every vertex then reads texel (0, 0).
    pub uvs: Vec<[f32; 2]>,
    pub indices: Vec<u32>,
}

/// An sRGB RGBA8 texture with its mip chain: `levels[0]` is `width` ×
/// `height`, each level after it half the last, down to 1×1.
pub struct TextureData {
    pub width: u32,
    pub height: u32,
    pub levels: Vec<Vec<u8>>,
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub(super) struct Vertex {
    pos: [f32; 3],
    normal: [f32; 3],
    uv: [f32; 2],
}

const ATTRIBUTES: [wgpu::VertexAttribute; 3] =
    wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32x2];

impl Vertex {
    pub(super) fn layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: size_of::<Vertex>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &ATTRIBUTES,
        }
    }
}

/// One uploaded mesh. Drawn by reference from a [`super::MeshInstance`];
/// its `id` is what the stage cache keys on.
pub struct GpuMesh {
    pub id: u64,
    pub(super) vertices: wgpu::Buffer,
    pub(super) indices: wgpu::Buffer,
    pub(super) index_count: u32,
    pub(super) texture: wgpu::BindGroup,
    /// Axis-aligned, in the mesh's own units — what a fit-to-canvas
    /// scale is worked out from.
    pub bounds: ([f32; 3], [f32; 3]),
}

impl MeshPass {
    /// Put a mesh on the GPU. Without a texture it draws plain white, so
    /// its colour is entirely the instance's tint.
    pub fn upload(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        data: &MeshData,
        texture: Option<&TextureData>,
    ) -> GpuMesh {
        let verts: Vec<Vertex> = data
            .positions
            .iter()
            .enumerate()
            .map(|(i, p)| Vertex {
                pos: *p,
                normal: data.normals.get(i).copied().unwrap_or([0.0, 0.0, 1.0]),
                uv: data.uvs.get(i).copied().unwrap_or([0.0, 0.0]),
            })
            .collect();
        let vertices = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("mesh vertices"),
            size: (verts.len().max(1) * size_of::<Vertex>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        queue.write_buffer(&vertices, 0, bytemuck::cast_slice(&verts));
        let indices = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("mesh indices"),
            size: (data.indices.len().max(1) * 4) as u64,
            usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        queue.write_buffer(&indices, 0, bytemuck::cast_slice(&data.indices));
        let mut bounds = ([f32::MAX; 3], [f32::MIN; 3]);
        for p in &data.positions {
            for ((lo, hi), v) in bounds.0.iter_mut().zip(bounds.1.iter_mut()).zip(p) {
                *lo = lo.min(*v);
                *hi = hi.max(*v);
            }
        }
        if data.positions.is_empty() {
            bounds = ([0.0; 3], [0.0; 3]);
        }
        let texture = match texture {
            Some(t) => texture_bind_group(device, queue, &self.texture_bgl, &self.sampler, t),
            None => self.white.clone(),
        };
        self.next_id += 1;
        GpuMesh {
            id: self.next_id,
            vertices,
            indices,
            index_count: data.indices.len() as u32,
            texture,
            bounds,
        }
    }

}

/// An sRGB texture with every mip level written, bound with the pass's
/// trilinear sampler.
pub(super) fn texture_bind_group(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    layout: &wgpu::BindGroupLayout,
    sampler: &wgpu::Sampler,
    t: &TextureData,
) -> wgpu::BindGroup {
    {
        let levels = t.levels.len().max(1) as u32;
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("mesh texture"),
            size: wgpu::Extent3d {
                width: t.width.max(1),
                height: t.height.max(1),
                depth_or_array_layers: 1,
            },
            mip_level_count: levels,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let (mut w, mut h) = (t.width.max(1), t.height.max(1));
        for (level, bytes) in t.levels.iter().enumerate() {
            queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &texture,
                    mip_level: level as u32,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                bytes,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(w * 4),
                    rows_per_image: Some(h),
                },
                wgpu::Extent3d {
                    width: w,
                    height: h,
                    depth_or_array_layers: 1,
                },
            );
            w = (w / 2).max(1);
            h = (h / 2).max(1);
        }
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("mesh texture"),
            layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(sampler),
                },
            ],
        })
    }
}
