//! Mesh objects on the shape side.
//!
//! A mesh is an object in the outliner like any shape: it has a centre, a
//! size, a colour, an opacity, a place in space, keyframes, and a card.
//! What it draws lives elsewhere — an imported model, named by asset id
//! in `extra[0]` and drawn by the mesh pass — so here it is the footprint
//! and the knobs. `b` holds the half extents of that footprint on the
//! object's plane, fitted when the model was imported, so `size()`,
//! scaling and the selection ants all work as they do for a box.

use super::{KIND_MESH, Shape};

impl Shape {
    /// A mesh object at `center` whose fitted footprint is `center ± half`,
    /// drawing the imported model registered as `asset`.
    pub fn mesh(center: [f32; 2], half: [f32; 2], asset: u32) -> Self {
        let mut s = Self::base(KIND_MESH, center, half);
        s.extra[0] = asset as f32;
        s
    }

    pub fn is_mesh(&self) -> bool {
        self.kind_rot[0] == KIND_MESH
    }

    /// Which imported model this draws — `None` off a mesh.
    pub fn mesh_asset(&self) -> Option<u32> {
        self.is_mesh().then(|| self.extra[0] as u32)
    }

    pub fn set_mesh_asset(&mut self, id: u32) {
        if self.is_mesh() {
            self.extra[0] = id as f32;
        }
    }

    /// The footprint's half extents on the plane — `None` off a mesh.
    pub fn mesh_half(&self) -> Option<[f32; 2]> {
        self.is_mesh().then_some(self.b)
    }

    /// How deep the model is drawn, canvas units — the third side, since
    /// width and height are the footprint's. Zero means "not set": the
    /// depth follows the thinner of the two (a mesh from before depth
    /// existed, until its model arrives and it is filled in).
    pub fn depth(&self) -> Option<f32> {
        self.is_mesh().then_some(self.extra[1])
    }

    pub fn set_depth(&mut self, d: f32) {
        if self.is_mesh() {
            self.extra[1] = d.max(0.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::ShapeKind;
    use super::*;

    /// A mesh has a width and a height — its footprint's — with no
    /// ceiling, so a plane can be stretched into a floor.
    #[test]
    fn a_mesh_has_a_width_and_a_height_of_its_own() {
        let mut m = Shape::mesh([100.0, 100.0], [50.0, 50.0], 1);
        assert_eq!(m.box_size(), Some([100.0, 100.0]));
        m.set_box_width(9000.0);
        m.set_box_height(40.0);
        assert_eq!(m.mesh_half(), Some([4500.0, 20.0]));
        assert_eq!(m.size(), 4500.0);
        // Depth is the third side, and scales with the whole.
        assert_eq!(m.depth(), Some(0.0));
        m.set_depth(30.0);
        m.scale_by(2.0);
        assert_eq!(m.depth(), Some(60.0));
        assert_eq!(m.mesh_half(), Some([9000.0, 40.0]));
        assert_eq!(Shape::circle([0.0; 2], 5.0).depth(), None);
    }

    #[test]
    fn a_mesh_is_a_footprint_with_an_asset() {
        let m = Shape::mesh([300.0, 200.0], [270.0, 137.0], 3);
        assert!(m.is_mesh());
        assert_eq!(m.kind(), ShapeKind::Mesh);
        assert_eq!(m.mesh_asset(), Some(3));
        assert_eq!(m.mesh_half(), Some([270.0, 137.0]));
        assert_eq!(m.size(), 270.0);
        assert_eq!(m.center(), [300.0, 200.0]);
        // No fill/outline: the model is what it is. Its footprint is its
        // width and height, though — a plane has to be a floor.
        assert_eq!(m.outline(), None);
        assert_eq!(m.box_size(), Some([540.0, 274.0]));
        assert_eq!(m.thickness(), None);
        // Scaling keeps the fitted aspect.
        let mut s = m;
        s.scale_by(2.0);
        assert_eq!(s.mesh_half(), Some([540.0, 274.0]));
        // Off a mesh, nothing.
        let c = Shape::circle([0.0; 2], 5.0);
        assert_eq!((c.mesh_asset(), c.mesh_half()), (None, None));
    }

    #[test]
    fn a_mesh_picks_and_outlines_by_its_footprint() {
        let m = Shape::mesh([300.0, 200.0], [100.0, 50.0], 1);
        assert!(m.distance([300.0, 200.0]) < 0.0);
        assert!(m.distance([390.0, 240.0]) < 0.0);
        assert!(m.distance([420.0, 200.0]) > 0.0);
        assert_eq!(m.selection_halo().kind(), ShapeKind::Box);
    }
}
