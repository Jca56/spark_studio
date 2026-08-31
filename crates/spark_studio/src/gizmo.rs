//! The transform gizmo: drag the selection through space without typing.
//!
//! Three **arrows** along the world's axes — X right, Y down, Z toward
//! the camera, red, green, blue — slide the selection along that axis;
//! three **rings** turn it. The rings are a gimbal: each sits on the axis
//! its angle actually rotates about — Turn on the world's Y, Tilt on the
//! turned X, Spin on the plane's own normal — so a drag around a ring is
//! exactly that angle changing, and an angle that has counted three turns
//! keeps its count. The whole thing is one size on screen whatever its
//! depth, and it is built from ordinary shapes placed in 3D: an arrow is
//! a segment and a billboarded dot, a ring is a circle on a plane.
//!
//! Hit testing and dragging happen in **pixels** through the camera the
//! viewport is looking through, so the gizmo works the same in the comp
//! viewer and the fly view.

use spark_render::{Camera, Framing, Mat4, Vec3};

use crate::editor::Editor;
use crate::overlay::{self, Overlay};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Axis {
    X,
    Y,
    Z,
}

impl Axis {
    const ALL: [Axis; 3] = [Axis::X, Axis::Y, Axis::Z];

    fn index(self) -> usize {
        self as usize
    }

    fn world(self) -> Vec3 {
        match self {
            Axis::X => Vec3::new(1.0, 0.0, 0.0),
            Axis::Y => Vec3::new(0.0, 1.0, 0.0),
            Axis::Z => Vec3::new(0.0, 0.0, 1.0),
        }
    }

    fn color(self) -> [f32; 3] {
        match self {
            Axis::X => [1.0, 0.30, 0.30],
            Axis::Y => [0.35, 1.0, 0.40],
            Axis::Z => [0.35, 0.60, 1.0],
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Part {
    Arrow(Axis),
    /// The ring about `Axis`: X tilts, Y turns, Z spins.
    Ring(Axis),
}

/// On-screen sizes, px.
const ARROW_PX: f32 = 110.0;
const RING_PX: f32 = 78.0;
const TIP_PX: f32 = 9.0;
const GRAB_PX: f32 = 11.0;

pub struct Gizmo {
    /// The pivot: the primary's centre, in the world.
    pub centre: Vec3,
    /// World units per px at the pivot — what keeps the gizmo one size.
    unit: f32,
    /// Each arrow's shaft on screen, px, and its tip (`None` when the
    /// tip is behind the camera).
    shafts: [([f32; 2], [f32; 2]); 3],
    tips: [Option<[f32; 2]>; 3],
    /// Each ring's plane: local x/y span it, origin at the centre.
    rings: [Mat4; 3],
}

/// A drag in progress on one part.
pub enum Drag {
    Arrow {
        axis: Axis,
        start: [f32; 2],
        /// The axis's direction on screen, px per world unit.
        screen: [f32; 2],
        /// World units per px, for an axis pointing at the camera.
        unit: f32,
        /// How far along the axis the selection has been moved so far.
        moved: f32,
    },
    Ring {
        axis: Axis,
        plane: Mat4,
        /// The last angle the cursor was at on the ring's plane.
        angle: f32,
    },
}

/// The gimbal's ring planes for a shape turned `turn` and tilted `tilt`:
/// each ring's local x/y are the axes its rotation carries one toward
/// the other, so a positive local angle is a positive rotation.
fn ring_planes(turn: f32, tilt: f32, centre: Vec3) -> [Mat4; 3] {
    let x = Axis::X.world();
    let y = Axis::Y.world();
    let z = Axis::Z.world();
    let ry = Mat4::rotation_y(turn);
    let r = ry * Mat4::rotation_x(tilt);
    [
        // Tilt: about the turned X; rotation_x carries y toward z.
        Mat4::from_basis(y, ry.transform_vec(z), ry.transform_vec(x), centre),
        // Turn: about the world's Y; rotation_y carries z toward x.
        Mat4::from_basis(z, x, y, centre),
        // Spin: about the plane's normal; in-plane, x toward y.
        Mat4::from_basis(r.transform_vec(x), r.transform_vec(y), r.transform_vec(z), centre),
    ]
}

pub fn build(editor: &Editor, camera: &Camera, framing: &Framing, res: (u32, u32)) -> Option<Gizmo> {
    let primary = editor.primary()?;
    if editor.is_hidden(primary) {
        return None;
    }
    let s = editor.posed_shape(primary, editor.shapes()[primary]);
    let c = s.center();
    let centre = Vec3::new(c[0], c[1], s.z());
    let ppu = camera.px_per_unit_at(framing, res, centre);
    if ppu <= 1e-6 {
        return None;
    }
    let unit = 1.0 / ppu;
    let base = camera.project(framing, res, centre)?;
    let len = ARROW_PX * unit;
    let mut shafts = [(base, base); 3];
    let mut tips = [None; 3];
    for a in Axis::ALL {
        let tip = camera.project(framing, res, centre + a.world() * len);
        shafts[a.index()] = (base, tip.unwrap_or(base));
        tips[a.index()] = tip;
    }
    Some(Gizmo {
        centre,
        unit,
        shafts,
        tips,
        rings: ring_planes(s.turn(), s.tilt(), centre),
    })
}

fn dist_to_segment(p: [f32; 2], a: [f32; 2], b: [f32; 2]) -> f32 {
    spark_render::sd_segment(p, a, b)
}

impl Gizmo {
    /// What the cursor is over: tips first (small and easy to miss),
    /// then shafts, then rings.
    pub fn hit(&self, camera: &Camera, framing: &Framing, res: (u32, u32), px: [f32; 2]) -> Option<Part> {
        for a in Axis::ALL {
            if let Some(t) = self.tips[a.index()]
                && ((px[0] - t[0]).powi(2) + (px[1] - t[1]).powi(2)).sqrt() <= TIP_PX + 4.0
            {
                return Some(Part::Arrow(a));
            }
        }
        for a in Axis::ALL {
            let (from, to) = self.shafts[a.index()];
            if dist_to_segment(px, from, to) <= GRAB_PX * 0.7 {
                return Some(Part::Arrow(a));
            }
        }
        let r = RING_PX * self.unit;
        let mut best: Option<(f32, Axis)> = None;
        for a in Axis::ALL {
            if let Some(q) = camera.plane_hit(framing, res, px, &self.rings[a.index()]) {
                let off = ((q[0] * q[0] + q[1] * q[1]).sqrt() - r).abs() / self.unit;
                if off <= GRAB_PX && best.is_none_or(|(b, _)| off < b) {
                    best = Some((off, a));
                }
            }
        }
        best.map(|(_, a)| Part::Ring(a))
    }

    /// Start dragging `part` from the cursor at `px`.
    pub fn begin(&self, part: Part, camera: &Camera, framing: &Framing, res: (u32, u32), px: [f32; 2]) -> Option<Drag> {
        match part {
            Part::Arrow(axis) => {
                let (from, to) = self.shafts[axis.index()];
                let len = ARROW_PX * self.unit;
                Some(Drag::Arrow {
                    axis,
                    start: px,
                    screen: [(to[0] - from[0]) / len, (to[1] - from[1]) / len],
                    unit: self.unit,
                    moved: 0.0,
                })
            }
            Part::Ring(axis) => {
                let plane = self.rings[axis.index()];
                let q = camera.plane_hit(framing, res, px, &plane)?;
                Some(Drag::Ring {
                    axis,
                    plane,
                    angle: q[1].atan2(q[0]),
                })
            }
        }
    }

    /// The gizmo as overlays; `hover` lights one part.
    pub fn overlays(&self, camera: &Camera, hover: Option<Part>) -> Vec<Overlay> {
        let mut out = Vec::new();
        let len = ARROW_PX * self.unit;
        for a in Axis::ALL {
            let lit = hover == Some(Part::Arrow(a));
            let rgb = if lit { [1.0; 3] } else { a.color() };
            let tip = self.centre + a.world() * len;
            out.extend(overlay::segment(self.centre, tip, 2.2 * self.unit, rgb, 1.0));
            out.push(overlay::dot(camera, tip, TIP_PX * self.unit, rgb, 1.0));
        }
        for a in Axis::ALL {
            let lit = hover == Some(Part::Ring(a));
            let rgb = if lit { [1.0; 3] } else { a.color() };
            out.push(overlay::circle_on(
                self.rings[a.index()],
                RING_PX * self.unit,
                1.7 * self.unit,
                rgb,
                if lit { 1.0 } else { 0.8 },
            ));
        }
        out
    }
}

impl Drag {
    /// The cursor moved to `px`: move the selection to match. Returns
    /// whether anything changed.
    pub fn update(&mut self, editor: &mut Editor, camera: &Camera, framing: &Framing, res: (u32, u32), px: [f32; 2]) -> bool {
        match self {
            Drag::Arrow {
                axis,
                start,
                screen,
                unit,
                moved,
            } => {
                let d = [px[0] - start[0], px[1] - start[1]];
                let s2 = screen[0] * screen[0] + screen[1] * screen[1];
                // An axis pointing straight at the camera has no direction
                // on screen: dragging up brings the selection toward it.
                let t = if s2.sqrt() > 0.05 {
                    (d[0] * screen[0] + d[1] * screen[1]) / s2
                } else {
                    -d[1] * *unit
                };
                let step = t - *moved;
                if step.abs() < 1e-6 {
                    return false;
                }
                *moved = t;
                match axis {
                    Axis::X => editor.move_selection_by([step, 0.0]),
                    Axis::Y => editor.move_selection_by([0.0, step]),
                    Axis::Z => editor.shift_selection_z(step),
                }
            }
            Drag::Ring { axis, plane, angle } => {
                let Some(q) = camera.plane_hit(framing, res, px, plane) else {
                    return false;
                };
                let now = q[1].atan2(q[0]);
                let mut delta = now - *angle;
                while delta > std::f32::consts::PI {
                    delta -= std::f32::consts::TAU;
                }
                while delta < -std::f32::consts::PI {
                    delta += std::f32::consts::TAU;
                }
                *angle = now;
                match axis {
                    Axis::X => editor.tilt_selection(delta),
                    Axis::Y => editor.turn_selection(delta),
                    Axis::Z => editor.spin_selection(delta),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use spark_render::{CANVAS_H, CANVAS_W, Shape, Viewport};

    const RES: (u32, u32) = (1920, 1080);

    fn setup() -> (Editor, Camera, Framing) {
        let mut e = Editor::empty();
        let i = e.push_shape(Shape::rect([CANVAS_W * 0.5, CANVAS_H * 0.5], [100.0, 60.0]));
        e.select(Some(i));
        let framing = Framing::Canvas {
            cview: (1.0, 0.0, 0.0),
            clip: Viewport {
                x: 0.0,
                y: 0.0,
                w: 1920.0,
                h: 1080.0,
            },
        };
        (e, Camera::stage(), framing)
    }

    #[test]
    fn the_gizmo_sits_on_the_selection_one_size_on_screen() {
        let (e, cam, f) = setup();
        let g = build(&e, &cam, &f, RES).unwrap();
        assert_eq!(g.centre, Vec3::new(960.0, 540.0, 0.0));
        // At 1 px per unit on the canvas the X tip is ARROW_PX to the right.
        let tip = g.tips[0].unwrap();
        assert!((tip[0] - (960.0 + ARROW_PX)).abs() < 0.5 && (tip[1] - 540.0).abs() < 0.5, "{tip:?}");
        assert_eq!(g.hit(&cam, &f, RES, tip), Some(Part::Arrow(Axis::X)));
        assert_eq!(g.hit(&cam, &f, RES, [960.0 + 50.0, 540.0]), Some(Part::Arrow(Axis::X)));
        // On the spin ring, straight up from the centre.
        assert_eq!(g.hit(&cam, &f, RES, [960.0, 540.0 - RING_PX]), Some(Part::Ring(Axis::Z)));
        assert_eq!(g.hit(&cam, &f, RES, [100.0, 100.0]), None);
        // Nothing selected: no gizmo.
        let mut none = Editor::empty();
        none.select(None);
        assert!(build(&none, &cam, &f, RES).is_none());
    }

    #[test]
    fn dragging_an_arrow_moves_along_its_axis() {
        let (mut e, cam, f) = setup();
        let g = build(&e, &cam, &f, RES).unwrap();
        let tip = g.tips[0].unwrap();
        let mut d = g.begin(Part::Arrow(Axis::X), &cam, &f, RES, tip).unwrap();
        // 40 px right, and a little down that the axis ignores.
        assert!(d.update(&mut e, &cam, &f, RES, [tip[0] + 40.0, tip[1] + 15.0]));
        let c = e.shapes()[0].center();
        assert!((c[0] - 1000.0).abs() < 0.5 && (c[1] - 540.0).abs() < 1e-3, "{c:?}");
        // The Z arrow points at the stage camera: dragging up comes nearer.
        let g = build(&e, &cam, &f, RES).unwrap();
        let from = g.shafts[2].0;
        let mut d = g.begin(Part::Arrow(Axis::Z), &cam, &f, RES, from).unwrap();
        assert!(d.update(&mut e, &cam, &f, RES, [from[0], from[1] - 30.0]));
        assert!(e.shapes()[0].z() > 20.0, "{}", e.shapes()[0].z());
    }

    #[test]
    fn dragging_a_ring_keeps_the_grabbed_point_under_the_cursor() {
        // Grab the spin ring at its top and drag to its right: a quarter
        // turn, and the shape's rotation follows by exactly that.
        let (mut e, cam, f) = setup();
        let g = build(&e, &cam, &f, RES).unwrap();
        let top = [960.0, 540.0 - RING_PX];
        let right = [960.0 + RING_PX, 540.0];
        let mut d = g.begin(Part::Ring(Axis::Z), &cam, &f, RES, top).unwrap();
        assert!(d.update(&mut e, &cam, &f, RES, right));
        let rot = e.shapes()[0].rotation();
        assert!((rot - std::f32::consts::FRAC_PI_2).abs() < 1e-3, "{rot}");
        // The turn ring's plane, for a shape at the canvas centre, passes
        // through the stage camera's eye: edge-on, no hit — honestly.
        let g = build(&e, &cam, &f, RES).unwrap();
        assert!(cam.plane_hit(&f, RES, [960.0 + 40.0, 540.0], &g.rings[1]).is_none());
        // Off-centre it is a plane like any other.
        e.move_selection_by([0.0, 200.0]);
        let g = build(&e, &cam, &f, RES).unwrap();
        assert!(cam.plane_hit(&f, RES, [960.0 + 40.0, 740.0], &g.rings[1]).is_some());
    }

    #[test]
    fn ring_planes_carry_positive_local_angles_to_positive_rotations() {
        // A point at local angle 0 on each ring, rotated by +0.3 about the
        // ring's axis, lands where local angle 0.3 is.
        let planes = ring_planes(0.7, -0.4, Vec3::ZERO);
        let rots = [
            Mat4::rotation_y(0.7) * Mat4::rotation_x(0.3) * Mat4::rotation_y(-0.7),
            Mat4::rotation_y(0.3),
            {
                let r = Mat4::rotation_y(0.7) * Mat4::rotation_x(-0.4);
                r * Mat4::rotation_z(0.3) * r.inverse().unwrap()
            },
        ];
        for (plane, rot) in planes.iter().zip(rots.iter()) {
            let p0 = plane.transform_point(Vec3::new(1.0, 0.0, 0.0));
            let want = plane.transform_point(Vec3::new(0.3f32.cos(), 0.3f32.sin(), 0.0));
            let got = rot.transform_point(p0);
            assert!((got - want).length() < 1e-4, "{got:?} vs {want:?}");
        }
    }
}
