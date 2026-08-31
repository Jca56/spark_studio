//! The depth attachment every scene target carries.
//!
//! Opaque objects (meshes, and the raymarched solids to come) write it;
//! the shape pass, whose shapes are flat and translucent and sorted
//! back-to-front, only tests against it, so a shape behind a mesh is hidden
//! by the mesh and a shape in front of one is not. Nothing writes depth yet
//! — the attachment is here so the pass that will has somewhere to.

pub const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

pub(crate) fn make(device: &wgpu::Device, size: (u32, u32)) -> wgpu::TextureView {
    device
        .create_texture(&wgpu::TextureDescriptor {
            label: Some("scene depth"),
            size: wgpu::Extent3d {
                width: size.0.max(1),
                height: size.1.max(1),
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        })
        .create_view(&wgpu::TextureViewDescriptor::default())
}

/// A multisampled depth attachment the opaque passes draw into, readable
/// by the resolve pass that copies its nearest samples out.
pub(crate) fn make_msaa(device: &wgpu::Device, size: (u32, u32), samples: u32) -> wgpu::TextureView {
    device
        .create_texture(&wgpu::TextureDescriptor {
            label: Some("scene depth msaa"),
            size: wgpu::Extent3d {
                width: size.0.max(1),
                height: size.1.max(1),
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: samples,
            dimension: wgpu::TextureDimension::D2,
            format: FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        })
        .create_view(&wgpu::TextureViewDescriptor::default())
}

/// Everything is at the far plane until something is drawn.
pub(crate) fn clear(encoder: &mut wgpu::CommandEncoder, view: &wgpu::TextureView) {
    encoder
        .begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("depth clear"),
            color_attachments: &[],
            depth_stencil_attachment: Some(attachment(view, wgpu::LoadOp::Clear(1.0))),
            ..Default::default()
        })
        .forget_lifetime();
}

pub(crate) fn attachment(
    view: &wgpu::TextureView,
    load: wgpu::LoadOp<f32>,
) -> wgpu::RenderPassDepthStencilAttachment<'_> {
    wgpu::RenderPassDepthStencilAttachment {
        view,
        depth_ops: Some(wgpu::Operations {
            load,
            store: wgpu::StoreOp::Store,
        }),
        stencil_ops: None,
    }
}

/// Test, never write: the shape pass's relationship to depth.
pub(crate) fn test_only() -> wgpu::DepthStencilState {
    wgpu::DepthStencilState {
        format: FORMAT,
        depth_write_enabled: false,
        depth_compare: wgpu::CompareFunction::LessEqual,
        stencil: wgpu::StencilState::default(),
        bias: wgpu::DepthBiasState::default(),
    }
}
