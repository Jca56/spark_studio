//! Marks drawn *in* the scene but never part of the picture: gizmo arrows
//! and rings, the floor, the canvas's frame, the render camera's frustum.
//!
//! Every one is an ordinary shape — a line, a circle — handed to the stage
//! with its own matrix, so it can lie on any plane in space. A segment
//! between two world points is a line on a plane that contains it; a dot
//! is a circle on a plane that faces the camera. Overlays draw as pure
//! light so they never occlude the work they are marking.

use spark_render::{Camera, Mat4, Shape, Vec3};

/// A shape and the matrix that places its plane in the world.
pub type Overlay = (Shape, Mat4);

/// A segment from `a` to `b`, `thick` canvas units wide.
pub fn segment(a: Vec3, b: Vec3, thick: f32, rgb: [f32; 3], intensity: f32) -> Option<Overlay> {
    let d = b - a;
    let len = d.length();
    if len < 1e-4 {
        return None;
    }
    let x = d * (1.0 / len);
    // The plane's other axis: any perpendicular will do, leaning on
    // whichever world axis the segment isn't running along.
    let hint = if x.y.abs() < 0.9 {
        Vec3::new(0.0, 1.0, 0.0)
    } else {
        Vec3::new(0.0, 0.0, 1.0)
    };
    let y = (hint - x * hint.dot(x)).normalized();
    let z = x.cross(y);
    let mut s = Shape::line([0.0, 0.0], [len, 0.0], thick)
        .color(rgb[0], rgb[1], rgb[2])
        .intensity(intensity);
    s.set_additive(true);
    Some((s, Mat4::from_basis(x, y, z, a)))
}

/// A circle of radius `r` about `frame`'s origin, on its x/y plane — an
/// outline `stroke` wide, or filled at zero.
pub fn circle_on(frame: Mat4, r: f32, stroke: f32, rgb: [f32; 3], intensity: f32) -> Overlay {
    let mut s = Shape::circle([0.0, 0.0], r)
        .color(rgb[0], rgb[1], rgb[2])
        .intensity(intensity);
    if stroke > 0.0 {
        s = s.stroke(stroke);
    }
    s.set_additive(true);
    (s, frame)
}

/// A filled dot at `p` that faces the camera from anywhere.
pub fn dot(camera: &Camera, p: Vec3, r: f32, rgb: [f32; 3], intensity: f32) -> Overlay {
    circle_on(Mat4::translation(p) * camera.billboard(), r, 0.0, rgb, intensity)
}

/// The floor: a grid on a plane just under the canvas's bottom edge,
/// running back into the scene and forward past the camera, so
/// perspective has lines to draw depth with. Faint; the line under the
/// canvas itself a little brighter.
pub fn floor_grid(canvas: [f32; 2]) -> Vec<Overlay> {
    const STEP: f32 = 240.0;
    const BACK: f32 = 3600.0;
    const FORWARD: f32 = 2400.0;
    const SIDE: f32 = 2400.0;
    let [cw, ch] = canvas;
    let y = ch + 2.0;
    let mut out = Vec::new();
    let lit = |x: f32, z: f32| if x == 0.0 || z == 0.0 { 0.22 } else { 0.10 };
    let mut x = -SIDE;
    while x <= cw + SIDE + 1.0 {
        out.extend(segment(
            Vec3::new(x, y, -BACK),
            Vec3::new(x, y, FORWARD),
            1.0,
            [1.0; 3],
            lit(x, 1.0),
        ));
        x += STEP;
    }
    let mut z = -BACK;
    while z <= FORWARD + 1.0 {
        out.extend(segment(
            Vec3::new(-SIDE, y, z),
            Vec3::new(cw + SIDE, y, z),
            1.0,
            [1.0; 3],
            lit(1.0, z),
        ));
        z += STEP;
    }
    out
}

/// The four corners of `canvas` on the plane every 2D shape lives on.
fn corners(canvas: [f32; 2]) -> [Vec3; 4] {
    let [w, h] = canvas;
    [
        Vec3::new(0.0, 0.0, 0.0),
        Vec3::new(w, 0.0, 0.0),
        Vec3::new(w, h, 0.0),
        Vec3::new(0.0, h, 0.0),
    ]
}

/// The canvas as a frame in space: its four edges, gold, on the plane
/// every 2D shape lives on.
pub fn canvas_frame(canvas: [f32; 2]) -> Vec<Overlay> {
    let c = corners(canvas);
    (0..4)
        .filter_map(|i| segment(c[i], c[(i + 1) % 4], 2.0, [1.0, 0.78, 0.09], 0.7))
        .collect()
}

/// The render camera as a wire pyramid from its eye to the corners of
/// its canvas — the frustum, which for the stage camera lands exactly on
/// the canvas frame — with a dot at the eye.
pub fn frustum(render_camera: &Camera, view_camera: &Camera) -> Vec<Overlay> {
    let rgb = [0.62, 0.42, 1.0];
    let mut out: Vec<Overlay> = corners(render_camera.canvas)
        .iter()
        .filter_map(|&c| segment(render_camera.eye, c, 1.5, rgb, 0.55))
        .collect();
    out.push(dot(view_camera, render_camera.eye, 9.0, rgb, 1.0));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_segment_lands_on_its_endpoints() {
        let (s, m) = segment(Vec3::new(10.0, 20.0, 30.0), Vec3::new(10.0, 20.0, 130.0), 2.0, [1.0; 3], 1.0)
            .unwrap();
        let (a, b) = s.line_ends();
        assert_eq!(a, [0.0, 0.0]);
        assert!((b[0] - 100.0).abs() < 1e-4 && b[1] == 0.0);
        let end = m.transform_point(Vec3::new(b[0], b[1], 0.0));
        assert!((end - Vec3::new(10.0, 20.0, 130.0)).length() < 1e-3, "{end:?}");
        assert!(segment(Vec3::ZERO, Vec3::ZERO, 1.0, [1.0; 3], 1.0).is_none());
    }

    #[test]
    fn the_floor_and_frame_and_frustum_are_made_of_light() {
        use spark_render::CANVAS;
        let stage = Camera::stage(CANVAS);
        let all: Vec<Overlay> = floor_grid(CANVAS)
            .into_iter()
            .chain(canvas_frame(CANVAS))
            .chain(frustum(&stage, &stage))
            .collect();
        assert!(all.len() > 40);
        assert!(all.iter().all(|(s, _)| s.additive()));
        assert_eq!(canvas_frame(CANVAS).len(), 4);
        // The frustum's edges start at the eye.
        let (s, m) = &frustum(&stage, &stage)[0];
        let (a, _) = s.line_ends();
        let start = m.transform_point(Vec3::new(a[0], a[1], 0.0));
        assert!((start - stage.eye).length() < 1e-2);
        // A portrait comp's frame is portrait, and its frustum reaches it.
        let tall = Camera::stage([1080.0, 1920.0]);
        let (s, m) = &frustum(&tall, &tall)[2];
        let (_, b) = s.line_ends();
        let corner = m.transform_point(Vec3::new(b[0], b[1], 0.0));
        assert!((corner - Vec3::new(1080.0, 1920.0, 0.0)).length() < 1e-2, "{corner:?}");
    }
}
