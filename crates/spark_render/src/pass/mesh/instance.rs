//! One drawing of a mesh — where it is, how it's coloured, which shape
//! it is drawn for — and what the stage cache keys such a drawing on.

use super::GpuMesh;
use crate::math::{Mat4, Vec3};

/// How far under a full opacity a mesh has to be before it is
/// see-through at all — see [`MeshInstance::opaque`].
const SEE_THROUGH: f32 = 1e-3;

/// One drawing of a mesh: where it is, how it's coloured, and which
/// shape in the scene it is drawn for.
#[derive(Clone, Copy)]
pub struct MeshInstance<'a> {
    pub mesh: &'a GpuMesh,
    /// The mesh's own units → the world.
    pub model: Mat4,
    /// rgb = tint × brightness, a = opacity. Under one the mesh is
    /// see-through and sorts among the shapes; at zero it isn't drawn.
    pub color: [f32; 4],
    /// Draw the colour as is, without lighting.
    pub unlit: bool,
    /// The shape in `Scene::shapes` this is drawn for — a mesh object is
    /// a kind-6 shape that draws no quad of its own — and so the place a
    /// see-through mesh takes in the stack. `None` sorts it by its own
    /// centre (see `super::stack`).
    pub slot: Option<usize>,
}

/// What the stage cache keys a mesh draw on: everything a draw reads.
#[derive(Clone, PartialEq)]
pub(crate) struct MeshKey {
    id: u64,
    model: Mat4,
    color: [f32; 4],
    unlit: bool,
    slot: Option<usize>,
}

impl MeshInstance<'_> {
    pub(crate) fn key(&self) -> MeshKey {
        MeshKey {
            id: self.mesh.id,
            model: self.model,
            color: self.color,
            unlit: self.unlit,
            slot: self.slot,
        }
    }

    /// Drawn at all: an opacity above zero.
    pub fn visible(&self) -> bool {
        self.color[3] > 0.0
    }

    /// Drawn with the opaque ones, writing depth: a full opacity — or
    /// within a whisker of one. A keyed opacity lands a float short of 1
    /// (Alva's ghost held at 0.99999994 for eight bars, and the lightning
    /// behind him glowed through), and a mesh at 99.9% is a wall: what
    /// it hides must not flip on float noise.
    pub fn opaque(&self) -> bool {
        self.color[3] >= 1.0 - SEE_THROUGH
    }

    /// The middle of the mesh's bounds, in the world.
    pub(crate) fn centre(&self) -> Vec3 {
        let (lo, hi) = self.mesh.bounds;
        let mid = Vec3::new(lo[0] + hi[0], lo[1] + hi[1], lo[2] + hi[2]) * 0.5;
        self.model.transform_point(mid)
    }
}
