//! Shadow maps: what the meshes keep the light off.
//!
//! Each casting light — a sun, or a spot; the first [`MAX_SHADOWS`] of
//! them in scene order — gets one layer of a depth array, rendered from
//! the light's point of view by a vertex-only pipeline over the same
//! instances the mesh pass draws. A sun looks through an orthographic
//! box fitted to the world bounds of every mesh in the scene; a spot
//! through a perspective frustum as wide as its cone, from where it is
//! to the far side of those bounds. The mesh shader then asks each map,
//! with a comparison sampler and a 3×3 tap, how lit a point is by that
//! light. Meshes only: shapes are light and cast none.
//!
//! Point lights don't cast yet — six faces each, for another day.

use crate::camera::Camera;
use crate::light::{Light, LightKind};
use crate::math::{Mat4, Vec3};

use super::{MeshInstance, depth};

/// The most maps a scene gets; casters past this many just don't.
pub const MAX_SHADOWS: usize = 4;
/// Each map's side, texels.
pub const SHADOW_RES: u32 = 2048;

/// The world-space box every mesh instance fits in.
pub(super) fn scene_bounds(meshes: &[MeshInstance]) -> Option<(Vec3, Vec3)> {
    let mut lo = Vec3::new(f32::MAX, f32::MAX, f32::MAX);
    let mut hi = Vec3::new(f32::MIN, f32::MIN, f32::MIN);
    for m in meshes {
        let (a, b) = m.mesh.bounds;
        for i in 0..8 {
            let c = Vec3::new(
                if i & 1 == 0 { a[0] } else { b[0] },
                if i & 2 == 0 { a[1] } else { b[1] },
                if i & 4 == 0 { a[2] } else { b[2] },
            );
            let p = m.model.transform_point(c);
            lo = Vec3::new(lo.x.min(p.x), lo.y.min(p.y), lo.z.min(p.z));
            hi = Vec3::new(hi.x.max(p.x), hi.y.max(p.y), hi.z.max(p.z));
        }
    }
    (!meshes.is_empty()).then_some((lo, hi))
}

/// Which lights cast, in slot order — the index of each in `lights`,
/// and the matrix its map is rendered through.
pub(super) fn plan(lights: &[Light], bounds: Option<(Vec3, Vec3)>) -> Vec<(usize, Mat4)> {
    let Some(bounds) = bounds else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for (i, l) in lights.iter().enumerate() {
        if out.len() >= MAX_SHADOWS {
            break;
        }
        match l.kind {
            LightKind::Sun => out.push((i, sun_view_proj(l.direction, bounds))),
            LightKind::Spot => out.push((i, spot_view_proj(l, bounds))),
            LightKind::Point | LightKind::Ambient => {}
        }
    }
    out
}

/// A sun's map: an orthographic box around the sphere the bounds fit in,
/// looking along the light from twice the sphere's radius back.
fn sun_view_proj(dir: Vec3, (lo, hi): (Vec3, Vec3)) -> Mat4 {
    let centre = (lo + hi) * 0.5;
    let r = ((hi - lo).length() * 0.5).max(1.0);
    let d = dir.normalized();
    let cam = Camera {
        eye: centre - d * (2.0 * r),
        target: centre,
        fov_y: 1.0,
        near: r,
        far: 3.0 * r,
    };
    Mat4::orthographic(r, r, r, 3.0 * r) * cam.view()
}

/// A spot's map: a perspective frustum a little wider than its cone,
/// from the light to just past the far corner of the bounds.
fn spot_view_proj(l: &Light, (lo, hi): (Vec3, Vec3)) -> Mat4 {
    let (mut dmin, mut dmax) = (f32::MAX, 0.0f32);
    for i in 0..8 {
        let c = Vec3::new(
            if i & 1 == 0 { lo.x } else { hi.x },
            if i & 2 == 0 { lo.y } else { hi.y },
            if i & 4 == 0 { lo.z } else { hi.z },
        );
        let d = (c - l.position).length();
        dmin = dmin.min(d);
        dmax = dmax.max(d);
    }
    let near = (dmin * 0.5).max(1.0);
    let far = (dmax + 1.0).max(near + 1.0);
    // A cone wider than a map can hold is capped: past the map's edge a
    // point is simply lit.
    let fov_y = (2.0 * l.cone * 1.15).clamp(0.05, 170f32.to_radians());
    let d = l.direction.normalized();
    let cam = Camera {
        eye: l.position,
        target: l.position + d,
        fov_y,
        near,
        far,
    };
    cam.projection_for(1.0) * cam.view()
}

/// The maps, the pipeline that fills them, and what the mesh shader
/// reads them through.
pub(super) struct ShadowMaps {
    pipeline: wgpu::RenderPipeline,
    cast_bgl: wgpu::BindGroupLayout,
    /// One `view_proj` uniform per slot — separate buffers, since every
    /// slot's pass is recorded before any of them runs.
    casts: [wgpu::Buffer; MAX_SHADOWS],
    cast_groups: [wgpu::BindGroup; MAX_SHADOWS],
    layers: [wgpu::TextureView; MAX_SHADOWS],
    /// What the mesh shader binds: every layer, a comparison sampler,
    /// and the slots' matrices.
    pub(super) array: wgpu::TextureView,
    pub(super) sampler: wgpu::Sampler,
    pub(super) matrices: wgpu::Buffer,
}

impl ShadowMaps {
    pub(super) fn new(device: &wgpu::Device, instances: &wgpu::Buffer) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("shadow"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../../shaders/shadow.wgsl").into()),
        });
        let cast_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("shadow cast"),
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
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("shadow"),
            bind_group_layouts: &[&cast_bgl],
            ..Default::default()
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("shadow"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[super::upload::Vertex::layout()],
            },
            fragment: None,
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                // Both sides, as the mesh pass draws them: a plaque has
                // to cast from either face.
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: depth::FORMAT,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::Less,
                stencil: wgpu::StencilState::default(),
                // Pushed away from the light a little, more on slopes, so
                // a surface doesn't shadow itself.
                bias: wgpu::DepthBiasState {
                    constant: 2,
                    slope_scale: 2.0,
                    clamp: 0.0,
                },
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });
        let casts = std::array::from_fn(|_| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("shadow cast"),
                size: 64,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            })
        });
        let cast_groups = Self::make_cast_groups(device, &cast_bgl, &casts, instances);
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("shadow maps"),
            size: wgpu::Extent3d {
                width: SHADOW_RES,
                height: SHADOW_RES,
                depth_or_array_layers: MAX_SHADOWS as u32,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: depth::FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let layers = std::array::from_fn(|i| {
            texture.create_view(&wgpu::TextureViewDescriptor {
                label: Some("shadow layer"),
                dimension: Some(wgpu::TextureViewDimension::D2),
                base_array_layer: i as u32,
                array_layer_count: Some(1),
                ..Default::default()
            })
        });
        let array = texture.create_view(&wgpu::TextureViewDescriptor {
            label: Some("shadow maps"),
            dimension: Some(wgpu::TextureViewDimension::D2Array),
            ..Default::default()
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("shadow"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            compare: Some(wgpu::CompareFunction::LessEqual),
            ..Default::default()
        });
        let matrices = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("shadow matrices"),
            size: (64 * MAX_SHADOWS) as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Self {
            pipeline,
            cast_bgl,
            casts,
            cast_groups,
            layers,
            array,
            sampler,
            matrices,
        }
    }

    fn make_cast_groups(
        device: &wgpu::Device,
        bgl: &wgpu::BindGroupLayout,
        casts: &[wgpu::Buffer; MAX_SHADOWS],
        instances: &wgpu::Buffer,
    ) -> [wgpu::BindGroup; MAX_SHADOWS] {
        std::array::from_fn(|i| {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("shadow cast"),
                layout: bgl,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: casts[i].as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: instances.as_entire_binding(),
                    },
                ],
            })
        })
    }

    /// The instance buffer was remade: bind the new one.
    pub(super) fn rebind(&mut self, device: &wgpu::Device, instances: &wgpu::Buffer) {
        self.cast_groups = Self::make_cast_groups(device, &self.cast_bgl, &self.casts, instances);
    }

    /// Render every planned map, and hand the shader the slots' matrices.
    /// The instance buffer already holds this frame's instances.
    pub(super) fn render(
        &self,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        plan: &[(usize, Mat4)],
        meshes: &[MeshInstance],
    ) {
        let mut mats = [Mat4::IDENTITY; MAX_SHADOWS];
        for (slot, (_, m)) in plan.iter().enumerate().take(MAX_SHADOWS) {
            mats[slot] = *m;
        }
        let flat: Vec<f32> = mats.iter().flat_map(|m| m.0).collect();
        queue.write_buffer(&self.matrices, 0, bytemuck::cast_slice(&flat));
        for (slot, (_, m)) in plan.iter().enumerate().take(MAX_SHADOWS) {
            queue.write_buffer(&self.casts[slot], 0, bytemuck::cast_slice(&m.0));
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("shadow map"),
                color_attachments: &[],
                depth_stencil_attachment: Some(depth::attachment(
                    &self.layers[slot],
                    wgpu::LoadOp::Clear(1.0),
                )),
                ..Default::default()
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &self.cast_groups[slot], &[]);
            for (i, m) in meshes.iter().enumerate() {
                pass.set_vertex_buffer(0, m.mesh.vertices.slice(..));
                pass.set_index_buffer(m.mesh.indices.slice(..), wgpu::IndexFormat::Uint32);
                let i = i as u32;
                pass.draw_indexed(0..m.mesh.index_count, 0, i..i + 1);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(p: Vec3, kind: LightKind) -> Light {
        Light {
            kind,
            position: p,
            direction: Vec3::new(0.0, 0.0, -1.0),
            cone: 30f32.to_radians(),
            ..Light::default_sun()
        }
    }

    #[test]
    fn suns_and_spots_cast_in_order_up_to_the_slots() {
        let bounds = Some((Vec3::new(0.0, 0.0, -50.0), Vec3::new(100.0, 100.0, 50.0)));
        let lights = [
            at(Vec3::ZERO, LightKind::Point),
            Light::default_sun(),
            at(Vec3::new(50.0, 50.0, 400.0), LightKind::Spot),
            at(Vec3::ZERO, LightKind::Ambient),
            Light::default_sun(),
            Light::default_sun(),
            Light::default_sun(),
            Light::default_sun(),
        ];
        let p = plan(&lights, bounds);
        let who: Vec<usize> = p.iter().map(|(i, _)| *i).collect();
        assert_eq!(who, vec![1, 2, 4, 5]);
        // No meshes: nothing to fit a map to, nothing to cast.
        assert!(plan(&lights, None).is_empty());
    }

    #[test]
    fn a_sun_map_puts_the_bounds_centre_mid_depth_and_a_spot_sees_its_target() {
        let bounds = (Vec3::new(0.0, 0.0, -50.0), Vec3::new(100.0, 100.0, 50.0));
        let centre = (bounds.0 + bounds.1) * 0.5;
        let m = sun_view_proj(Light::default_sun().direction, bounds);
        let c = m.transform4([centre.x, centre.y, centre.z, 1.0]);
        assert!((c[0] / c[3]).abs() < 1e-4 && (c[1] / c[3]).abs() < 1e-4);
        assert!((c[2] / c[3] - 0.5).abs() < 1e-4, "{}", c[2] / c[3]);
        // Every corner is inside the box.
        for i in 0..8 {
            let p = Vec3::new(
                if i & 1 == 0 { bounds.0.x } else { bounds.1.x },
                if i & 2 == 0 { bounds.0.y } else { bounds.1.y },
                if i & 4 == 0 { bounds.0.z } else { bounds.1.z },
            );
            let q = m.transform4([p.x, p.y, p.z, 1.0]);
            assert!(q[0].abs() <= 1.0 && q[1].abs() <= 1.0 && (0.0..=1.0).contains(&q[2]), "{q:?}");
        }
        // A spot above the box, looking down its axis: the point straight
        // ahead is the map's centre, in front of the near plane.
        let mut spot = at(Vec3::new(50.0, 50.0, 400.0), LightKind::Spot);
        spot.direction = Vec3::new(0.0, 0.0, -1.0);
        let m = spot_view_proj(&spot, bounds);
        let q = m.transform4([50.0, 50.0, 0.0, 1.0]);
        assert!((q[0] / q[3]).abs() < 1e-4 && (q[1] / q[3]).abs() < 1e-4);
        assert!(q[2] / q[3] > 0.0 && q[2] / q[3] < 1.0, "{}", q[2] / q[3]);
    }
}
