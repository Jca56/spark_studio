//! Lights, the frame's view of them: what the meshes are lit by this
//! frame, and the gizmos that show where the lights are and where they
//! point.

use spark_render::{Camera, LIGHT_PICK, Light, LightKind, Mat4, Shape, Vec3};

use crate::overlay::{self, Overlay};

/// Every light among `shapes` — the document's display copies, so a
/// keyed, reacting, folder-posed light shines from where it is drawn.
pub(crate) fn scene_lights(shapes: &[Shape]) -> Vec<Light> {
    shapes.iter().filter_map(Shape::as_light).collect()
}

/// Editor-only marks for each light, in its colour, as pure light: a ring
/// on the light's own plane (tilted with it, so it reads as a disc facing
/// where the light aims) and a dot at its position; for a sun, an arrow
/// along its direction; for a spot, its cone as a wire — the far ring
/// grows when the spot points at the camera and shrinks when it points
/// away, which is the one cue a flat aim line could never give.
pub(crate) fn gizmos(shapes: &[Shape], camera: &Camera) -> Vec<Overlay> {
    let mut out = Vec::new();
    for s in shapes {
        let Some(light) = s.as_light() else { continue };
        let c = s.center();
        let rgb = s.rgb();
        let p = light.position;
        let mut ring = Shape::circle(c, LIGHT_PICK - 6.0)
            .stroke(2.5)
            .color(rgb[0], rgb[1], rgb[2])
            .intensity(1.0);
        ring.set_additive(true);
        out.push((ring, s.model()));
        out.push(overlay::dot(camera, p, 4.0, rgb, 1.0));
        let rot = Mat4::rotation_y(s.turn()) * Mat4::rotation_x(s.tilt());
        match light.kind {
            LightKind::Sun => {
                let tip = p + light.direction * 220.0;
                out.extend(overlay::segment(p, tip, 1.8, rgb, 0.9));
                out.push(overlay::dot(camera, tip, 6.0, rgb, 1.0));
            }
            LightKind::Spot => {
                // The cone's far ring sits at the range, on the plane the
                // light faces; four edges run out to it.
                let reach = light.range.max(1.0);
                let r = reach * light.cone.tan().abs().min(4.0);
                let far = p + light.direction * reach;
                let frame = Mat4::translation(far) * rot;
                out.push(overlay::circle_on(frame, r, 1.5, rgb, 0.8));
                for (x, y) in [(r, 0.0), (-r, 0.0), (0.0, r), (0.0, -r)] {
                    let rim = frame.transform_point(Vec3::new(x, y, 0.0));
                    out.extend(overlay::segment(p, rim, 1.2, rgb, 0.55));
                }
            }
            // Everywhere has no direction to draw: the ring and the dot
            // are its whole mark.
            LightKind::Ambient => {}
            LightKind::Point => {
                // Its reach, as a ring facing the camera.
                out.push(overlay::circle_on(
                    Mat4::translation(p) * camera.billboard(),
                    light.range,
                    1.2,
                    rgb,
                    0.35,
                ));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lights_and_gizmos_come_only_from_lights() {
        let shapes = [
            Shape::circle([10.0; 2], 5.0),
            Shape::sun([100.0, 100.0]),
            Shape::light([300.0, 300.0], LightKind::Point),
        ];
        let lights = scene_lights(&shapes);
        assert_eq!(lights.len(), 2);
        assert_eq!(lights[0].kind, LightKind::Sun);
        let cam = Camera::stage(spark_render::CANVAS);
        // A sun: ring, dot, arrow, tip. A point: ring, dot, reach.
        let g = gizmos(&shapes, &cam);
        assert_eq!(g.len(), 7);
        assert!(g.iter().all(|(s, _)| s.additive()));
        assert!(gizmos(&shapes[..1], &cam).is_empty());
    }

    #[test]
    fn a_spot_cone_ends_at_its_range_along_its_aim() {
        let mut spot = Shape::light([500.0, 500.0], LightKind::Spot);
        spot.set_z(300.0);
        let cam = Camera::stage(spark_render::CANVAS);
        let g = gizmos(&[spot], &cam);
        // ring, dot, far ring, four edges
        assert_eq!(g.len(), 7);
        let (far_ring, frame) = &g[2];
        let centre = frame.transform_point(Vec3::new(0.0, 0.0, 0.0));
        let light = spot.as_light().unwrap();
        let want = light.position + light.direction * light.range;
        assert!((centre - want).length() < 1e-2, "{centre:?} vs {want:?}");
        assert!(far_ring.size() > 300.0, "a 30° cone at 700 is about 400 across");
    }
}
