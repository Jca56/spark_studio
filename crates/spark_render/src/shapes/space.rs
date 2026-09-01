//! Where a shape's plane sits in the scene.
//!
//! Every shape is flat, and its 2D fields — centre, rotation, size — say
//! where it is *on its plane*. `space` says where the plane is: moved
//! along z (toward the camera for positive), tilted about x, turned about
//! y, all about the shape's own centre. A shape that has never left the canvas has all three at zero
//! and draws through the identity, which is every shape there was before
//! the comp became a scene.
//!
//! Tilt and Turn count turns the way Rotation does: two full turns is 4π
//! and means it, so a logo can spin as long as the song lasts.

use crate::camera::Camera;
use crate::math::{Mat4, Vec3};

use super::Shape;

impl Shape {
    /// Height off the canvas toward the camera: zero is the canvas,
    /// larger is nearer.
    pub fn z(&self) -> f32 {
        self.space[0]
    }

    /// Rotation about the plane's horizontal axis, radians.
    pub fn tilt(&self) -> f32 {
        self.space[1]
    }

    /// Rotation about the plane's vertical axis, radians.
    pub fn turn(&self) -> f32 {
        self.space[2]
    }

    pub fn set_z(&mut self, z: f32) {
        self.space[0] = z;
    }

    pub fn set_tilt(&mut self, r: f32) {
        self.space[1] = r;
    }

    pub fn set_turn(&mut self, r: f32) {
        self.space[2] = r;
    }

    /// Whether the plane is the canvas — the identity, and the fast path.
    pub fn on_plane(&self) -> bool {
        self.space[0] == 0.0 && self.space[1] == 0.0 && self.space[2] == 0.0
    }

    /// The plane → the world: turn, then tilt, about the shape's centre,
    /// then push back by z.
    pub fn model(&self) -> Mat4 {
        if self.on_plane() {
            return Mat4::IDENTITY;
        }
        let c = self.center();
        Mat4::translation(Vec3::new(0.0, 0.0, self.z()))
            * Mat4::about(
                Vec3::new(c[0], c[1], 0.0),
                Mat4::rotation_y(self.turn()) * Mat4::rotation_x(self.tilt()),
            )
    }

    /// Where a canvas point lands on this shape's plane: the ray from the
    /// camera through `(canvas, 0)`, met with the plane, in plane-local
    /// coordinates — the ones every 2D query (`distance`, handles) speaks.
    /// `None` when the ray misses the plane or meets it behind the camera.
    pub fn unproject(&self, camera: &Camera, canvas: [f32; 2]) -> Option<[f32; 2]> {
        self.unproject_depth(camera, canvas).map(|(p, _)| p)
    }

    /// [`Shape::unproject`] with how far along the ray the plane was
    /// met: 1 is the canvas plane itself, less is nearer the camera,
    /// more is farther — what a picker compares to find the *nearest*
    /// hit rather than the topmost of the stack (a backdrop plane added
    /// last sat on top of the stack and swallowed every click, 2026-08-31).
    pub fn unproject_depth(&self, camera: &Camera, canvas: [f32; 2]) -> Option<([f32; 2], f32)> {
        if self.on_plane() {
            return Some((canvas, 1.0));
        }
        let inv = self.model().inverse()?;
        let o = inv.transform_point(camera.eye);
        let d = inv.transform_vec(Vec3::new(canvas[0], canvas[1], 0.0) - camera.eye);
        // Parallel, or as near as float rotation gets: a plane seen
        // edge-on has no face to hit.
        if d.z.abs() < 1e-6 * d.length() {
            return None;
        }
        let t = -o.z / d.z;
        if t < 0.0 {
            return None;
        }
        let p = o + d * t;
        Some(([p.x, p.y], t))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shapes::CANVAS;
    use std::f32::consts::FRAC_PI_2;

    #[test]
    fn a_shape_on_the_canvas_is_the_identity() {
        let s = Shape::rect([300.0, 200.0], [50.0, 20.0]);
        assert!(s.on_plane());
        assert_eq!(s.model(), Mat4::IDENTITY);
        let cam = Camera::stage(CANVAS);
        assert_eq!(s.unproject(&cam, [123.0, 456.0]), Some([123.0, 456.0]));
    }

    #[test]
    fn turning_keeps_the_centre_and_swings_the_edge() {
        let mut s = Shape::rect([300.0, 200.0], [50.0, 20.0]);
        s.set_turn(FRAC_PI_2);
        let m = s.model();
        let c = m.transform_point(Vec3::new(300.0, 200.0, 0.0));
        assert!((c - Vec3::new(300.0, 200.0, 0.0)).length() < 1e-4);
        // The right edge, a quarter turn about y, now points along -z.
        let e = m.transform_point(Vec3::new(350.0, 200.0, 0.0));
        assert!((e.x - 300.0).abs() < 1e-4 && (e.z + 50.0).abs() < 1e-4, "{e:?}");
    }

    #[test]
    fn z_pushes_the_plane_back() {
        let mut s = Shape::circle([100.0, 100.0], 10.0);
        s.set_z(500.0);
        let p = s.model().transform_point(Vec3::new(100.0, 100.0, 0.0));
        assert!((p.z - 500.0).abs() < 1e-4);
    }

    #[test]
    fn unprojecting_a_pushed_back_shape_finds_its_plane() {
        // Half a canvas back: from the camera the shape looks smaller and
        // sits nearer the vanishing point, so a screen point near the
        // canvas centre lands on the plane farther from it.
        let cam = Camera::stage(CANVAS);
        let d = (cam.target - cam.eye).length();
        let mut s = Shape::rect([960.0, 540.0], [100.0, 100.0]);
        s.set_z(-d);
        // The vanishing point maps to itself.
        let c = s.unproject(&cam, [960.0, 540.0]).unwrap();
        assert!((c[0] - 960.0).abs() < 1e-3 && (c[1] - 540.0).abs() < 1e-3);
        // Fifty canvas units right of it on screen is a hundred on a
        // plane twice as far away.
        let p = s.unproject(&cam, [1010.0, 540.0]).unwrap();
        assert!((p[0] - 1060.0).abs() < 1e-2, "{p:?}");
    }

    #[test]
    fn unprojecting_a_turned_shape_walks_along_its_plane() {
        let cam = Camera::stage(CANVAS);
        let mut s = Shape::rect([960.0, 540.0], [100.0, 100.0]);
        s.set_turn(60f32.to_radians());
        // The centre is on the axis of the turn: it maps to itself.
        let c = s.unproject(&cam, [960.0, 540.0]).unwrap();
        assert!((c[0] - 960.0).abs() < 1e-3 && (c[1] - 540.0).abs() < 1e-3);
        // A screen point to the right lands farther out along the plane
        // than the flat distance, since the plane runs away from the eye.
        let p = s.unproject(&cam, [1000.0, 540.0]).unwrap();
        assert!(p[0] > 960.0 + 40.0 / 0.5 * 0.9, "{p:?}");
        assert!((p[1] - 540.0).abs() < 1e-3);
    }

    #[test]
    fn a_plane_edge_on_to_the_camera_cannot_be_hit() {
        let cam = Camera::stage(CANVAS);
        let mut s = Shape::rect([960.0, 540.0], [100.0, 100.0]);
        s.set_turn(FRAC_PI_2);
        // Straight down the plane through its centre: the ray lies in it.
        assert!(s.unproject(&cam, [960.0, 540.0]).is_none());
    }

    #[test]
    fn space_rides_the_serialized_line() {
        let mut s = Shape::circle([1.0, 2.0], 3.0);
        s.set_z(40.0);
        s.set_tilt(0.5);
        s.set_turn(-7.0);
        let back = Shape::from_array(s.to_array());
        assert_eq!(back, s);
        assert_eq!((back.z(), back.tilt(), back.turn()), (40.0, 0.5, -7.0));
        // A line from before there was a scene reads as on the canvas.
        let flat = Shape::from_short_array(s.to_array(), 26);
        assert!(!flat.on_plane(), "from_short_array only zero-fills past `count`");
        let mut arr = s.to_array();
        arr[26..].fill(0.0);
        assert!(Shape::from_short_array(arr, 26).on_plane());
    }
}
