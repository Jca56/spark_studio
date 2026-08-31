//! The camera: where the scene is looked at from.
//!
//! Spark's frame has always been the canvas — 1920×1080 units by default,
//! x right, y down — mapped onto the window by the CanvasView's fit, zoom
//! and pan. The canvas's size is the document's, and the camera carries it
//! (see [`Camera::canvas`]): a portrait comp for a phone is a different
//! film gate on the same camera. A
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

use crate::geom::Viewport;
use crate::math::{Mat4, Vec3};

/// How the picture is placed on the target.
///
/// The comp viewer shows the **canvas**: the camera's frame, aspect-fit
/// and then zoomed and panned by the CanvasView, clipped to the panel it
/// sits in — the video, as it will be. The orbit view is **free**: any
/// camera at all, filling a region of the target with its own aspect, no
/// canvas rectangle to fit — the scene, as it is.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Framing {
    /// `cview` is (scale, offset x, offset y), canvas units to window px;
    /// `clip` the region the picture may paint within.
    Canvas { cview: (f32, f32, f32), clip: Viewport },
    Free(Viewport),
}

impl Framing {
    /// World → the target's clip space, through `camera`.
    pub fn view_proj(&self, camera: &Camera, resolution: (u32, u32)) -> Mat4 {
        match *self {
            Framing::Canvas { cview, .. } => camera.view_proj(resolution, cview),
            Framing::Free(vp) => {
                fit_free(resolution, vp)
                    * camera.projection_for(vp.w / vp.h.max(1.0))
                    * camera.view()
            }
        }
    }

    /// The pixels the picture may paint: for the canvas — `camera`'s —
    /// its footprint on the window ∩ the clip ∩ the frame; for a free view,
    /// the region ∩ the frame. `None` when that is empty. Shared by every
    /// pass's scissor and the stage blit's, so a cached frame lands on
    /// exactly the pixels a live one would have.
    pub fn paint_rect(&self, camera: &Camera, resolution: (u32, u32)) -> Option<(u32, u32, u32, u32)> {
        let (fx, fy, x1, y1) = match *self {
            Framing::Canvas { cview, clip } => {
                let (vs, vx, vy) = cview;
                let [cw, ch] = camera.canvas;
                (
                    vx.max(clip.x).max(0.0),
                    vy.max(clip.y).max(0.0),
                    (vx + cw * vs).min(clip.x + clip.w),
                    (vy + ch * vs).min(clip.y + clip.h),
                )
            }
            Framing::Free(vp) => (vp.x.max(0.0), vp.y.max(0.0), vp.x + vp.w, vp.y + vp.h),
        };
        let x1 = x1.min(resolution.0 as f32);
        let y1 = y1.min(resolution.1 as f32);
        if x1 <= fx || y1 <= fy {
            return None;
        }
        Some((fx as u32, fy as u32, (x1 - fx) as u32, (y1 - fy) as u32))
    }

    /// The same framing on a target `div` times smaller.
    pub fn reduced(&self, div: u32) -> Framing {
        let d = div as f32;
        let shrink = |v: Viewport| Viewport {
            x: v.x / d,
            y: v.y / d,
            w: v.w / d,
            h: v.h / d,
        };
        match *self {
            Framing::Canvas { cview, clip } => Framing::Canvas {
                cview: (cview.0 / d, cview.1 / d, cview.2 / d),
                clip: shrink(clip),
            },
            Framing::Free(vp) => Framing::Free(shrink(vp)),
        }
    }

    /// Roughly how many px a canvas unit on the canvas plane covers — what
    /// decides whether a halo is small enough to draw with its body. For a
    /// free view the canvas may be anywhere; the fit's scale stands in.
    pub fn frame_scale(&self, camera: &Camera) -> f32 {
        match *self {
            Framing::Canvas { cview, .. } => cview.0,
            Framing::Free(vp) => (vp.h / camera.canvas[1]).max(0.0001),
        }
    }
}

/// NDC of a free camera's whole frustum → NDC of the viewport it fills.
fn fit_free(resolution: (u32, u32), vp: Viewport) -> Mat4 {
    let (w, h) = (resolution.0 as f32, resolution.1 as f32);
    let ax = vp.w / w;
    let bx = (2.0 * vp.x + vp.w) / w - 1.0;
    let ay = vp.h / h;
    let by = 1.0 - (2.0 * vp.y + vp.h) / h;
    Mat4::from_rows([
        [ax, 0.0, 0.0, bx],
        [0.0, ay, 0.0, by],
        [0.0, 0.0, 1.0, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ])
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Camera {
    pub eye: Vec3,
    pub target: Vec3,
    /// Vertical field of view, radians.
    pub fov_y: f32,
    pub near: f32,
    pub far: f32,
    /// The canvas this camera frames — the comp's size in canvas units,
    /// which is also the video's size in pixels. The film gate: it sets
    /// the projection's aspect, and for the stage camera the distance
    /// that makes the canvas plane fill the frame exactly. Carried on the
    /// camera so a framing, a pass and a pick all read the one size the
    /// document chose, and a changed canvas is a cache miss like any other
    /// camera change.
    pub canvas: [f32; 2],
}

/// Screen-down, the hint that fixes the camera's roll: the view's x axis
/// is perpendicular to this and to the look direction.
const DOWN: Vec3 = Vec3::new(0.0, 1.0, 0.0);

impl Camera {
    /// The stage camera's field of view: 40° vertical, about a 50 mm lens
    /// on a full frame — the flattering, faintly-dramatic default every
    /// motion tool ships with.
    pub const STAGE_FOV: f32 = 40.0 * std::f32::consts::PI / 180.0;

    /// Looking straight at the middle of `canvas` from far enough back
    /// that it fills the frame exactly. The distance follows the canvas's
    /// height, so a portrait comp is looked at from further back than a
    /// landscape one — the lens is the same, the film gate is taller.
    pub fn stage(canvas: [f32; 2]) -> Self {
        let [w, h] = canvas;
        let d = (h * 0.5) / (Self::STAGE_FOV * 0.5).tan();
        let centre = Vec3::new(w * 0.5, h * 0.5, 0.0);
        Self {
            eye: centre + Vec3::new(0.0, 0.0, d),
            target: centre,
            fov_y: Self::STAGE_FOV,
            near: d * 0.01,
            far: d * 50.0,
            canvas,
        }
    }

    /// An orbiting camera: `distance` from `target`, swung `yaw` about y
    /// and `pitch` about x from straight in front of it. Zero and zero
    /// looks at the target the way the stage camera looks at the canvas.
    /// The lens and the film gate are the stage camera's for `canvas`.
    pub fn orbit(target: Vec3, yaw: f32, pitch: f32, distance: f32, canvas: [f32; 2]) -> Self {
        let offset = (Mat4::rotation_y(yaw) * Mat4::rotation_x(pitch))
            .transform_vec(Vec3::new(0.0, 0.0, distance.max(1.0)));
        Self {
            eye: target + offset,
            target,
            ..Self::stage(canvas)
        }
    }

    /// The direction the camera looks along, unit length.
    pub fn forward(&self) -> Vec3 {
        (self.target - self.eye).normalized()
    }

    /// The view's axes in the world: right, down, forward. The frame is
    /// left-handed — x right, y down, z toward the camera — so the cross
    /// products go the other way round from the textbook's.
    pub fn basis(&self) -> (Vec3, Vec3, Vec3) {
        let f = self.forward();
        let mut right = f.cross(DOWN).normalized();
        if right == Vec3::ZERO {
            // Looking straight up or down the y axis: any perpendicular
            // will do, and z-hat keeps x-hat where it was.
            right = f.cross(Vec3::new(0.0, 0.0, 1.0)).normalized();
        }
        (right, right.cross(f), f)
    }

    /// A plane facing the camera: local x is screen-right, local y
    /// screen-down, so a shape drawn on it reads the same from anywhere.
    /// Place it with a translation on the left.
    pub fn billboard(&self) -> Mat4 {
        let (r, d, f) = self.basis();
        Mat4::from_basis(r, d, -f, Vec3::ZERO)
    }

    /// Where a world point lands on the target, in px — `None` behind the
    /// camera.
    pub fn project(&self, framing: &Framing, resolution: (u32, u32), p: Vec3) -> Option<[f32; 2]> {
        let [x, y, _, w] = framing
            .view_proj(self, resolution)
            .transform4([p.x, p.y, p.z, 1.0]);
        if w <= 1e-6 {
            return None;
        }
        Some([
            (x / w + 1.0) * 0.5 * resolution.0 as f32,
            (1.0 - y / w) * 0.5 * resolution.1 as f32,
        ])
    }

    /// The ray through a target pixel: the eye, and a unit direction.
    pub fn ray(&self, framing: &Framing, resolution: (u32, u32), px: [f32; 2]) -> Option<Vec3> {
        let inv = framing.view_proj(self, resolution).inverse()?;
        let ndc = Vec3::new(
            px[0] / resolution.0 as f32 * 2.0 - 1.0,
            1.0 - px[1] / resolution.1 as f32 * 2.0,
            0.5,
        );
        let p = inv.transform_point(ndc);
        Some((p - self.eye).normalized())
    }

    /// Where a target pixel's ray meets the canvas plane, in canvas units
    /// — the cursor as the editor sees it, whatever the camera. `None`
    /// when the ray runs parallel to the canvas or away from it.
    pub fn canvas_hit(&self, framing: &Framing, resolution: (u32, u32), px: [f32; 2]) -> Option<[f32; 2]> {
        let d = self.ray(framing, resolution, px)?;
        if d.z.abs() < 1e-6 * d.length().max(1e-9) {
            return None;
        }
        let t = -self.eye.z / d.z;
        if t < 0.0 {
            return None;
        }
        let p = self.eye + d * t;
        Some([p.x, p.y])
    }

    /// Where a target pixel's ray meets the plane `frame` places (its x/y
    /// plane, in that plane's own units) — the cursor as a gizmo ring or
    /// a turned shape sees it. `None` for a plane seen edge-on or hit
    /// behind the camera.
    pub fn plane_hit(
        &self,
        framing: &Framing,
        resolution: (u32, u32),
        px: [f32; 2],
        frame: &Mat4,
    ) -> Option<[f32; 2]> {
        let dir = self.ray(framing, resolution, px)?;
        let inv = frame.inverse()?;
        let o = inv.transform_point(self.eye);
        let d = inv.transform_vec(dir);
        if d.z.abs() < 1e-6 * d.length().max(1e-9) {
            return None;
        }
        let t = -o.z / d.z;
        if t < 0.0 {
            return None;
        }
        let p = o + d * t;
        Some([p.x, p.y])
    }

    /// How many target px one world unit covers at `p`, measured along
    /// the view's right axis. Zero behind the camera.
    pub fn px_per_unit_at(&self, framing: &Framing, resolution: (u32, u32), p: Vec3) -> f32 {
        let (right, _, _) = self.basis();
        match (
            self.project(framing, resolution, p),
            self.project(framing, resolution, p + right),
        ) {
            (Some(a), Some(b)) => ((b[0] - a[0]).powi(2) + (b[1] - a[1]).powi(2)).sqrt(),
            _ => 0.0,
        }
    }

    /// How far along the view a point is: what back-to-front sorting
    /// orders by. Larger is farther from the camera (which, with z
    /// running toward the camera, is *smaller* z).
    pub fn depth(&self, p: Vec3) -> f32 {
        (p - self.eye).dot(self.forward())
    }

    /// World → view: x right, y down, z forward, camera at the origin.
    pub fn view(&self) -> Mat4 {
        let (right, down, f) = self.basis();
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
        self.projection_for(self.canvas[0] / self.canvas[1].max(1e-6))
    }

    /// The same for any aspect — a free view filling a viewport.
    pub fn projection_for(&self, aspect: f32) -> Mat4 {
        let tan_y = (self.fov_y * 0.5).tan();
        let tan_x = tan_y * aspect;
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
        fit(resolution, cview, self.canvas) * self.projection() * self.view()
    }
}

/// NDC of the canvas rectangle → NDC of the frame. Affine in x and y, so
/// it rides the clip coordinates ahead of the divide.
fn fit(resolution: (u32, u32), cview: (f32, f32, f32), canvas: [f32; 2]) -> Mat4 {
    let (w, h) = (resolution.0 as f32, resolution.1 as f32);
    let (s, ox, oy) = cview;
    let [cw, ch] = canvas;
    let ax = s * cw / w;
    let bx = (2.0 * ox + s * cw) / w - 1.0;
    let ay = s * ch / h;
    let by = 1.0 - (2.0 * oy + s * ch) / h;
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
    use crate::shapes::{CANVAS, CANVAS_H, CANVAS_W};

    const RES: (u32, u32) = (64, 64);
    const VIEW: (f32, f32, f32) = (0.1, 3.0, -2.0);

    /// Where a world point lands, in window px, through the stage camera.
    fn px(p: Vec3) -> (f32, f32) {
        let ndc = Camera::stage(CANVAS).view_proj(RES, VIEW).transform_point(p);
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
        let cam = Camera::stage(CANVAS);
        let d = (cam.target - cam.eye).length();
        let centre = flat(CANVAS_W * 0.5, CANVAS_H * 0.5);
        let far = px(Vec3::new(CANVAS_W * 0.5 + 100.0, CANVAS_H * 0.5, -d));
        assert!((far.0 - centre.0 - 50.0 * VIEW.0).abs() < 1e-3);
        assert!((far.1 - centre.1).abs() < 1e-3);
    }

    #[test]
    fn halfway_to_the_camera_is_twice_as_wide() {
        let cam = Camera::stage(CANVAS);
        let d = (cam.target - cam.eye).length();
        let centre = flat(CANVAS_W * 0.5, CANVAS_H * 0.5);
        let near = px(Vec3::new(CANVAS_W * 0.5, CANVAS_H * 0.5 + 100.0, d * 0.5));
        assert!((near.1 - centre.1 - 200.0 * VIEW.0).abs() < 1e-3);
    }

    #[test]
    fn depth_grows_away_from_the_camera() {
        let cam = Camera::stage(CANVAS);
        let on = cam.depth(Vec3::new(10.0, 20.0, 0.0));
        let behind = cam.depth(Vec3::new(1900.0, 20.0, -100.0));
        let toward = cam.depth(Vec3::new(10.0, 1000.0, 100.0));
        assert!(toward < on && on < behind);
        // On the canvas plane, depth is the camera's distance from it.
        assert!((on - (cam.target - cam.eye).length()).abs() < 1e-3);
    }

    #[test]
    fn the_canvas_plane_is_inside_the_depth_range() {
        let ndc = Camera::stage(CANVAS)
            .view_proj(RES, VIEW)
            .transform_point(Vec3::new(500.0, 500.0, 0.0));
        assert!(ndc.z > 0.0 && ndc.z < 1.0, "clip depth {}", ndc.z);
    }

    fn canvas() -> Framing {
        Framing::Canvas {
            cview: VIEW,
            clip: Viewport {
                x: 0.0,
                y: 0.0,
                w: RES.0 as f32,
                h: RES.1 as f32,
            },
        }
    }

    #[test]
    fn project_agrees_with_the_2d_map_and_rays_come_back() {
        let cam = Camera::stage(CANVAS);
        let p = Vec3::new(400.0, 700.0, 0.0);
        let got = cam.project(&canvas(), RES, p).unwrap();
        let want = flat(400.0, 700.0);
        assert!((got[0] - want.0).abs() < 1e-3 && (got[1] - want.1).abs() < 1e-3);
        // The pixel's ray meets the canvas where the point was.
        let back = cam.canvas_hit(&canvas(), RES, got).unwrap();
        assert!((back[0] - 400.0).abs() < 1e-2 && (back[1] - 700.0).abs() < 1e-2, "{back:?}");
        // A point behind the camera has no pixel.
        assert!(cam.project(&canvas(), RES, cam.eye + Vec3::new(0.0, 0.0, 10.0)).is_none());
        // One canvas unit is VIEW px on the canvas plane.
        assert!((cam.px_per_unit_at(&canvas(), RES, p) - VIEW.0).abs() < 1e-4);
    }

    #[test]
    fn an_orbit_camera_fills_a_free_viewport() {
        let target = Vec3::new(CANVAS_W * 0.5, CANVAS_H * 0.5, 0.0);
        let cam = Camera::orbit(target, 0.6, -0.4, 2500.0, CANVAS);
        assert!(((cam.eye - target).length() - 2500.0).abs() < 1e-2);
        let vp = Viewport {
            x: 10.0,
            y: 20.0,
            w: 40.0,
            h: 30.0,
        };
        let f = Framing::Free(vp);
        // The target sits dead centre of the viewport.
        let c = cam.project(&f, RES, target).unwrap();
        assert!((c[0] - 30.0).abs() < 1e-3 && (c[1] - 35.0).abs() < 1e-3, "{c:?}");
        // A pixel's ray through the viewport lands back on the canvas.
        let hit = cam.canvas_hit(&f, RES, c).unwrap();
        assert!((hit[0] - target.x).abs() < 1e-1 && (hit[1] - target.y).abs() < 1e-1, "{hit:?}");
        assert_eq!(f.paint_rect(&cam, RES), Some((10, 20, 40, 30)));
        // Yaw and pitch of zero is the stage camera's line of sight.
        let front = Camera::orbit(target, 0.0, 0.0, 1000.0, CANVAS);
        assert!((front.forward() - Vec3::new(0.0, 0.0, -1.0)).length() < 1e-5);
    }

    #[test]
    fn a_plane_hit_comes_back_in_the_planes_own_units() {
        let cam = Camera::stage(CANVAS);
        // A plane standing on the canvas, facing +x, 300 units in front.
        let frame = Mat4::translation(Vec3::new(500.0, 500.0, 300.0)) * Mat4::rotation_y(std::f32::consts::FRAC_PI_2);
        let p = frame.transform_point(Vec3::new(40.0, -25.0, 0.0));
        let px = cam.project(&canvas(), RES, p).unwrap();
        let local = cam.plane_hit(&canvas(), RES, px, &frame).unwrap();
        assert!((local[0] - 40.0).abs() < 1e-2 && (local[1] + 25.0).abs() < 1e-2, "{local:?}");
        // The canvas plane itself agrees with `canvas_hit`.
        let on = cam.plane_hit(&canvas(), RES, [20.0, 30.0], &Mat4::IDENTITY).unwrap();
        let hit = cam.canvas_hit(&canvas(), RES, [20.0, 30.0]).unwrap();
        assert!((on[0] - hit[0]).abs() < 1e-3 && (on[1] - hit[1]).abs() < 1e-3);
    }

    #[test]
    fn the_billboard_faces_the_camera() {
        let cam = Camera::stage(CANVAS);
        // Head-on, a billboard is the canvas plane itself.
        assert_eq!(cam.billboard(), Mat4::IDENTITY);
        let orbit = Camera::orbit(Vec3::ZERO, 1.0, 0.3, 500.0, CANVAS);
        let n = orbit.billboard().transform_vec(Vec3::new(0.0, 0.0, 1.0));
        assert!((n + orbit.forward()).length() < 1e-5, "normal points at the camera");
    }

    /// A portrait canvas — a phone's — is a different film gate on the
    /// same lens: its corners land exactly on the corners of the rectangle
    /// the CanvasView gives it, and its centre dead centre, so a comp made
    /// for TikTok is edited and exported through the one camera.
    #[test]
    fn a_portrait_canvas_fills_its_own_rectangle() {
        let canvas = [1080.0, 1920.0];
        let cam = Camera::stage(canvas);
        let res = (100, 200);
        // The canvas at 1/20, sitting at (5, 4): 54×96 px.
        let view = (0.05, 5.0, 4.0);
        let f = Framing::Canvas {
            cview: view,
            clip: Viewport {
                x: 0.0,
                y: 0.0,
                w: 100.0,
                h: 200.0,
            },
        };
        for (x, y) in [(0.0, 0.0), (1080.0, 1920.0), (540.0, 960.0), (200.0, 1700.0)] {
            let got = cam.project(&f, res, Vec3::new(x, y, 0.0)).unwrap();
            let want = [view.1 + x * view.0, view.2 + y * view.0];
            assert!(
                (got[0] - want[0]).abs() < 1e-3 && (got[1] - want[1]).abs() < 1e-3,
                "({x}, {y}): got {got:?}, want {want:?}"
            );
        }
        assert_eq!(f.paint_rect(&cam, res), Some((5, 4, 54, 96)));
        // Looked at from further back than a landscape canvas: the gate is
        // taller and the lens the same.
        assert!(cam.eye.z > Camera::stage(CANVAS).eye.z);
        // And picking comes back through the same gate.
        let hit = cam.canvas_hit(&f, res, [5.0 + 200.0 * 0.05, 4.0 + 1700.0 * 0.05]).unwrap();
        assert!((hit[0] - 200.0).abs() < 1e-2 && (hit[1] - 1700.0).abs() < 1e-2, "{hit:?}");
    }

    #[test]
    fn a_camera_looking_down_the_y_axis_still_has_a_frame() {
        let cam = Camera {
            eye: Vec3::new(0.0, -100.0, 0.0),
            target: Vec3::ZERO,
            ..Camera::stage(CANVAS)
        };
        let v = cam.view();
        // The look direction lands on +z, whatever the roll.
        let f = v.transform_vec(Vec3::new(0.0, 1.0, 0.0));
        assert!((f.z - 1.0).abs() < 1e-5 && f.x.abs() < 1e-5 && f.y.abs() < 1e-5);
    }
}
