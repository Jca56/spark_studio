//! The camera shake, by projection: a jolt pans the eye and the target
//! together and moves the canvas plane by exactly that many units.

use super::*;
use crate::shapes::CANVAS;

/// A shake pans: eye and target move together by the jolt in the
/// view's own axes, the look direction unchanged, and a zero jolt is
/// the same camera.
#[test]
fn a_shaken_camera_pans_without_turning() {
    let cam = Camera::stage(CANVAS);
    assert_eq!(cam.shaken([0.0, 0.0]), cam);
    let s = cam.shaken([30.0, -12.0]);
    let d = s.eye - cam.eye;
    assert!((d.x - 30.0).abs() < 1e-4 && (d.y + 12.0).abs() < 1e-4 && d.z.abs() < 1e-4, "{d:?}");
    assert!(((s.target - cam.target) - d).length() < 1e-4);
    assert!((s.forward() - cam.forward()).length() < 1e-6);
    // The canvas plane's centre lands 30 left and 12 down of where it
    // did: the picture jolts by exactly the amount, in canvas units.
    let centre = Vec3::new(CANVAS[0] * 0.5, CANVAS[1] * 0.5, 0.0);
    let framing = Framing::Canvas {
        cview: (1.0, 0.0, 0.0),
        clip: Viewport { x: 0.0, y: 0.0, w: CANVAS[0], h: CANVAS[1] },
    };
    let res = (CANVAS[0] as u32, CANVAS[1] as u32);
    let (a, b) = (cam.project(&framing, res, centre).unwrap(), s.project(&framing, res, centre).unwrap());
    assert!((b[0] - a[0] + 30.0).abs() < 0.05 && (b[1] - a[1] - 12.0).abs() < 0.05, "{a:?} → {b:?}");
}
