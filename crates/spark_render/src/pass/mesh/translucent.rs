//! See-through meshes: drawn in their turn in the stack, over what is
//! behind them and under what comes after.
//!
//! A run of them goes through the same multisampled colour target as the
//! opaque ones, cleared, against the depth the opaque pass left. Each
//! mesh in the run gets a depth prepass of its own first — its nearest
//! surface into the buffer, no colour — and its colour pass then draws
//! only that surface: a half-faded solid shows the scene behind it, not
//! its own inside. The prepass leaves the mesh's depth in the buffer for
//! the rest of the run, so a see-through mesh drawn after it (nearer, by
//! the sort) tests against it too — the same answer the sort gives,
//! sharper where the two meet. Nothing here reaches the stage's own
//! depth attachment: the shapes after a see-through mesh still see only
//! the opaque scene, and draw over the mesh as the sort says.

use super::MeshPass;
use super::super::Scene;
use crate::camera::Framing;

impl MeshPass {
    /// Draw the see-through meshes `which` — indices into `scene.meshes`,
    /// back to front — resolved into `color` within the same footprint
    /// `draw` used. `draw` has run this frame: the instances are on the
    /// GPU, the lights are set, and the depth is there to test.
    pub(crate) fn draw_translucent(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        color: &wgpu::TextureView,
        which: &[usize],
        scene: &Scene,
        resolution: (u32, u32),
        framing: Framing,
    ) {
        let Some(rect) = framing.paint_rect(scene.camera, resolution) else {
            return;
        };
        let t = self.targets.as_ref().expect("the opaque pass runs first");
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("see-through meshes"),
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
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            ..Default::default()
        });
        pass.set_scissor_rect(rect.0, rect.1, rect.2, rect.3);
        pass.set_bind_group(0, &self.bind_group, &[]);
        for &i in which {
            let m = &scene.meshes[i];
            pass.set_bind_group(1, &m.mesh.texture, &[]);
            pass.set_vertex_buffer(0, m.mesh.vertices.slice(..));
            pass.set_index_buffer(m.mesh.indices.slice(..), wgpu::IndexFormat::Uint32);
            let i = i as u32;
            pass.set_pipeline(&self.prepass);
            pass.draw_indexed(0..m.mesh.index_count, 0, i..i + 1);
            pass.set_pipeline(&self.translucent);
            pass.draw_indexed(0..m.mesh.index_count, 0, i..i + 1);
        }
    }
}
