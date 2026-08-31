//! The camera: where the scene is looked at from.
//!
//! Spark's frame has always been the canvas — 1920×1080 units, x right, y
//! down — mapped onto the window by the CanvasView's fit, zoom and pan. A
//! scene keeps that frame and adds depth: z runs *toward* the camera, so
//! larger is nearer — the way a higher layer is on top — and a comp that
//! never leaves the `z = 0` plane is the 2D picture it always was. (After
//! Effects runs z the other way; Alva's first hour with it said which was
//! right.) x right, y down, z toward the viewer is a left-handed frame,
//! which only the view basis below has to know.
//!
//! The stage camera is built so that the canvas plane projects to exactly
//! the canvas rectangle — a point at `(x, y, 0)` lands on precisely the
//! window pixel the 2D map used to put it on — and perspective only shows
//! on things that leave the plane. `view_proj` composes the CanvasView's
//! fit into the projection so the shader has one matrix and one multiply.

use crate::math::{Mat4, Vec3};
use crate::shapes::{CANVAS_H, CANVAS_W};

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Camera {
    pub eye: Vec3,
    pub target: Vec3,
    /// Vertical field of view, radians.
    pub fov_y: f32,
    pub near: f32,
    pub far: f32,
}

/// Screen-down, the hint that fixes the camera's roll: the view's x axis
/// is perpendicular to this and to the look direction.
const DOWN: Vec3 = Vec3::new(0.0, 1.0, 0.0);

impl Camera {
    /// The stage camera's field of view: 40° vertical, about a 50 mm lens
    /// on a full frame — the flattering, faintly-dramatic default every
    /// motion tool ships with.
    pub const STAGE_FOV: f32 = 40.0 * std::f32::consts::PI / 180.0;

    /// Looking straight at the middle of the canvas from far enough back
    /// that the canvas fills the frame exactly.
    pub fn stage() -> Self {
        let d = (CANVAS_H * 0.5) / (Self::STAGE_FOV * 0.5).tan();
        let centre = Vec3::new(CANVAS_W * 0.5, CANVAS_H * 0.5, 0.0);
        Self {
            eye: centre + Vec3::new(0.0, 0.0, d),
            target: centre,
            fov_y: Self::STAGE_FOV,
            near: d * 0.01,
            far: d * 50.0,
        }
    }

    /// The direction the camera looks along, unit length.
    pub fn forward(&self) -> Vec3 {
        (self.target - self.eye).normalized()
    }

    /// How far along the view a point is: what back-to-front sorting
    /// orders by. Larger is farther from the camera (which, with z
    /// running toward the camera, is *smaller* z).
    pub fn depth(&self, p: Vec3) -> f32 {
        (p - self.eye).dot(self.forward())
    }

    /// World → view: x right, y down, z forward, camera at the origin.
    pub fn view(&self) -> Mat4 {
        let f = self.forward();
        // Left-handed frame: the cross products go the other way round
        // from the textbook's, so that looking down -z leaves x pointing
        // right and y pointing down.
        let mut right = f.cross(DOWN).normalized();
        if right == Vec3::ZERO {
            // Looking straight up or down the y axis: any perpendicular
            // will do, and z-hat keeps x-hat where it was.
            right = f.cross(Vec3::new(0.0, 0.0, 1.0)).normalized();
        }
        let down = right.cross(f);
        let e = self.eye;
        Mat4::from_rows([
            [right.x, right.y, right.z, -right.dot(e)],
            [down.x, down.y, down.z, -down.dot(e)],
            [f.x, f.y, f.z, -f.dot(e)],
            [0.0, 0.0, 0.0, 1.0],
        ])
    }

    /// View → clip, wgpu conventions (NDC y up, depth 0..1). The frustum's
    /// aspect is the canvas's, so at the target's distance the visible
    /// rectangle *is* the canvas.
    pub fn projection(&self) -> Mat4 {
        let tan_y = (self.fov_y * 0.5).tan();
        let tan_x = tan_y * (CANVAS_W / CANVAS_H);
        let (n, f) = (self.near, self.far);
        let a = f / (f - n);
        let b = -f * n / (f - n);
        Mat4::from_rows([
            [1.0 / tan_x, 0.0, 0.0, 0.0],
            [0.0, -1.0 / tan_y, 0.0, 0.0],
            [0.0, 0.0, a, b],
            [0.0, 0.0, 1.0, 0.0],
        ])
    }

    /// World → the frame's clip space, with the CanvasView's fit composed
    /// in: `cview` is (scale, offset x, offset y), canvas units to window
    /// px, and `resolution` the frame it maps into.
    pub fn view_proj(&self, resolution: (u32, u32), cview: (f32, f32, f32)) -> Mat4 {
        fit(resolution, cview) * self.projection() * self.view()
    }
}

/// NDC of the canvas rectangle → NDC of the frame. Affine in x and y, so
/// it rides the clip coordinates ahead of the divide.
fn fit(resolution: (u32, u32), cview: (f32, f32, f32)) -> Mat4 {
    let (w, h) = (resolution.0 as f32, resolution.1 as f32);
    let (s, ox, oy) = cview;
    let ax = s * CANVAS_W / w;
    let bx = (2.0 * ox + s * CANVAS_W) / w - 1.0;
    let ay = s * CANVAS_H / h;
    let by = 1.0 - (2.0 * oy + s * CANVAS_H) / h;
    Mat4::from_rows([
        [ax, 0.0, 0.0, bx],
        [0.0, ay, 0.0, by],
        [0.0, 0.0, 1.0, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    const RES: (u32, u32) = (64, 64);
    const VIEW: (f32, f32, f32) = (0.1, 3.0, -2.0);

    /// Where a world point lands, in window px, through the stage camera.
    fn px(p: Vec3) -> (f32, f32) {
        let ndc = Camera::stage().view_proj(RES, VIEW).transform_point(p);
        (
            (ndc.x + 1.0) * 0.5 * RES.0 as f32,
            (1.0 - ndc.y) * 0.5 * RES.1 as f32,
        )
    }

    /// The 2D map the shape pass used before there was a camera.
    fn flat(x: f32, y: f32) -> (f32, f32) {
        (VIEW.1 + x * VIEW.0, VIEW.2 + y * VIEW.0)
    }

    #[test]
    fn the_canvas_plane_lands_on_the_2d_maps_pixels() {
        for (x, y) in [
            (0.0, 0.0),
            (CANVAS_W, CANVAS_H),
            (CANVAS_W * 0.5, CANVAS_H * 0.5),
            (123.4, 567.8),
            (-300.0, 1500.0),
        ] {
            let got = px(Vec3::new(x, y, 0.0));
            let want = flat(x, y);
            assert!(
                (got.0 - want.0).abs() < 1e-3 && (got.1 - want.1).abs() < 1e-3,
                "({x}, {y}): got {got:?}, want {want:?}"
            );
        }
    }

    #[test]
    fn twice_as_far_is_half_as_wide() {
        let cam = Camera::stage();
        let d = (cam.target - cam.eye).length();
        let centre = flat(CANVAS_W * 0.5, CANVAS_H * 0.5);
        let far = px(Vec3::new(CANVAS_W * 0.5 + 100.0, CANVAS_H * 0.5, -d));
        assert!((far.0 - centre.0 - 50.0 * VIEW.0).abs() < 1e-3);
        assert!((far.1 - centre.1).abs() < 1e-3);
    }

    #[test]
    fn halfway_to_the_camera_is_twice_as_wide() {
        let cam = Camera::stage();
        let d = (cam.target - cam.eye).length();
        let centre = flat(CANVAS_W * 0.5, CANVAS_H * 0.5);
        let near = px(Vec3::new(CANVAS_W * 0.5, CANVAS_H * 0.5 + 100.0, d * 0.5));
        assert!((near.1 - centre.1 - 200.0 * VIEW.0).abs() < 1e-3);
    }

    #[test]
    fn depth_grows_away_from_the_camera() {
        let cam = Camera::stage();
        let on = cam.depth(Vec3::new(10.0, 20.0, 0.0));
        let behind = cam.depth(Vec3::new(1900.0, 20.0, -100.0));
        let toward = cam.depth(Vec3::new(10.0, 1000.0, 100.0));
        assert!(toward < on && on < behind);
        // On the canvas plane, depth is the camera's distance from it.
        assert!((on - (cam.target - cam.eye).length()).abs() < 1e-3);
    }

    #[test]
    fn the_canvas_plane_is_inside_the_depth_range() {
        let ndc = Camera::stage()
            .view_proj(RES, VIEW)
            .transform_point(Vec3::new(500.0, 500.0, 0.0));
        assert!(ndc.z > 0.0 && ndc.z < 1.0, "clip depth {}", ndc.z);
    }

    #[test]
    fn a_camera_looking_down_the_y_axis_still_has_a_frame() {
        let cam = Camera {
            eye: Vec3::new(0.0, -100.0, 0.0),
            target: Vec3::ZERO,
            ..Camera::stage()
        };
        let v = cam.view();
        // The look direction lands on +z, whatever the roll.
        let f = v.transform_vec(Vec3::new(0.0, 1.0, 0.0));
        assert!((f.z - 1.0).abs() < 1e-5 && f.x.abs() < 1e-5 && f.y.abs() < 1e-5);
    }
}
