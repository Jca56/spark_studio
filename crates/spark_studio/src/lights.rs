//! Lights, the frame's view of them: what the meshes are lit by this
//! frame, and the gizmos that show where the lights are.

use spark_render::{LIGHT_PICK, Light, Shape};

/// Every light among `shapes` — the document's display copies, so a
/// keyed, reacting, folder-posed light shines from where it is drawn.
pub(crate) fn scene_lights(shapes: &[Shape]) -> Vec<Light> {
    shapes.iter().filter_map(Shape::as_light).collect()
}

/// Editor-only marks for each light: a ring on the light's own plane —
/// tilted with it, so it reads as a disc facing where the light aims —
/// and, for a sun or a spot, a short line along the aim's on-canvas
/// direction. Drawn as pure light in the light's colour, never occluding.
pub(crate) fn gizmos(shapes: &[Shape]) -> Vec<Shape> {
    let mut out = Vec::new();
    for s in shapes {
        let Some(light) = s.as_light() else { continue };
        let c = s.center();
        let rgb = s.rgb();
        let mut ring = Shape::circle(c, LIGHT_PICK - 6.0)
            .stroke(2.5)
            .color(rgb[0], rgb[1], rgb[2])
            .intensity(1.0);
        ring.set_additive(true);
        ring.set_z(s.z());
        ring.set_tilt(s.tilt());
        ring.set_turn(s.turn());
        out.push(ring);
        let mut dot = Shape::circle(c, 4.0).color(rgb[0], rgb[1], rgb[2]).intensity(1.0);
        dot.set_additive(true);
        dot.set_z(s.z());
        out.push(dot);
        if light.kind != spark_render::LightKind::Point {
            let d = light.direction;
            let len = (d.x * d.x + d.y * d.y).sqrt();
            if len > 1e-3 {
                let to = [c[0] + d.x / len * 70.0 * len.max(0.25), c[1] + d.y / len * 70.0 * len.max(0.25)];
                let mut aim = Shape::line(c, to, 1.6).color(rgb[0], rgb[1], rgb[2]).intensity(0.9);
                aim.set_additive(true);
                aim.set_z(s.z());
                out.push(aim);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use spark_render::LightKind;

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
        // A sun gets a ring, a dot and an aim line; a point just the two.
        let g = gizmos(&shapes);
        assert_eq!(g.len(), 5);
        assert!(g.iter().all(|s| s.additive()));
        assert!(gizmos(&shapes[..1]).is_empty());
    }
}
