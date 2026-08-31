//! Aligning objects in space: the world-space box every object fits in,
//! and, along one axis, the values a gizmo drag snaps that box to — the
//! selection's edges and centre to every other object's edges and
//! centre, and to the canvas's — with the guide that shows where it
//! locked. The 3D half of View > Smart Guides (`editor/snap.rs` is the
//! 2D half): Alva built a room out of planes and asked for objects that
//! snap to each other (2026-08-31).

use std::collections::HashMap;

use spark_render::{Shape, Vec3};

use crate::editor::Editor;
use crate::gizmo::Axis;
use crate::meshes::{self, MeshAssetGpu};
use crate::overlay::{self, Overlay};

/// How close, in logical px on screen, a lock happens.
pub const SNAP_PX: f32 = 12.0;

fn along(v: Vec3, axis: Axis) -> f32 {
    match axis {
        Axis::X => v.x,
        Axis::Y => v.y,
        Axis::Z => v.z,
    }
}

fn fold(points: impl IntoIterator<Item = Vec3>) -> Option<(Vec3, Vec3)> {
    let mut out: Option<(Vec3, Vec3)> = None;
    for p in points {
        out = Some(match out {
            None => (p, p),
            Some((lo, hi)) => (
                Vec3::new(lo.x.min(p.x), lo.y.min(p.y), lo.z.min(p.z)),
                Vec3::new(hi.x.max(p.x), hi.y.max(p.y), hi.z.max(p.z)),
            ),
        });
    }
    out
}

/// The world-space box of a shape as it is drawn: a mesh's model through
/// its placement, any other shape's footprint (spun, on its plane). A
/// light has no box; nor does a mesh whose model hasn't arrived.
pub fn world_box(s: &Shape, meshes: &HashMap<u32, MeshAssetGpu>) -> Option<(Vec3, Vec3)> {
    let bounds = s.mesh_asset().and_then(|id| meshes.get(&id)).map(|a| a.bounds);
    world_box_with(s, bounds)
}

/// The same, given a mesh's bounds directly.
pub fn world_box_with(s: &Shape, bounds: Option<([f32; 3], [f32; 3])>) -> Option<(Vec3, Vec3)> {
    if s.is_light() {
        return None;
    }
    if s.is_mesh() {
        let (lo, hi) = bounds?;
        let m = s.model() * meshes::placement(s, (lo, hi));
        return fold((0..8).map(|i| {
            m.transform_point(Vec3::new(
                if i & 1 == 0 { lo[0] } else { hi[0] },
                if i & 2 == 0 { lo[1] } else { hi[1] },
                if i & 4 == 0 { lo[2] } else { hi[2] },
            ))
        }));
    }
    let c = s.center();
    let half = s
        .box_size()
        .map(|[w, h]| [w * 0.5, h * 0.5])
        .unwrap_or([s.size(), s.size()]);
    let (sn, cs) = s.rotation().sin_cos();
    let m = s.model();
    fold([(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)].map(|(sx, sy)| {
        let (dx, dy) = (half[0] * sx, half[1] * sy);
        m.transform_point(Vec3::new(
            c[0] + dx * cs - dy * sn,
            c[1] + dx * sn + dy * cs,
            0.0,
        ))
    }))
}

/// Something to lock to: a value along the axis, and the box it belongs
/// to, for the guide.
#[derive(Clone, Copy, PartialEq, Debug)]
struct Target {
    at: f32,
    lo: Vec3,
    hi: Vec3,
}

/// Where a drag is locked: the target's box, sliced at `at` along `axis`.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Guide {
    pub axis: Axis,
    pub at: f32,
    pub lo: Vec3,
    pub hi: Vec3,
}

impl Guide {
    /// The slice as a rectangle of light, `thick` world units wide.
    pub fn overlays(&self, thick: f32) -> Vec<Overlay> {
        let (lo, hi, at) = (self.lo, self.hi, self.at);
        let corners: [Vec3; 4] = match self.axis {
            Axis::X => [
                Vec3::new(at, lo.y, lo.z),
                Vec3::new(at, hi.y, lo.z),
                Vec3::new(at, hi.y, hi.z),
                Vec3::new(at, lo.y, hi.z),
            ],
            Axis::Y => [
                Vec3::new(lo.x, at, lo.z),
                Vec3::new(hi.x, at, lo.z),
                Vec3::new(hi.x, at, hi.z),
                Vec3::new(lo.x, at, hi.z),
            ],
            Axis::Z => [
                Vec3::new(lo.x, lo.y, at),
                Vec3::new(hi.x, lo.y, at),
                Vec3::new(hi.x, hi.y, at),
                Vec3::new(lo.x, hi.y, at),
            ],
        };
        let gold = [1.0, 0.78, 0.09];
        (0..4)
            .filter_map(|i| overlay::segment(corners[i], corners[(i + 1) % 4], thick, gold, 1.0))
            .collect()
    }
}

/// The values along one axis a drag of the selection can lock to.
#[derive(Clone, Debug)]
pub struct AxisSnap {
    axis: Axis,
    /// The selection's box along the axis when the drag began.
    lo: f32,
    hi: f32,
    targets: Vec<Target>,
    /// World units within which a lock happens.
    threshold: f32,
}

impl AxisSnap {
    /// For the selection as it stands: its box, and every other visible
    /// object's edges and centre along `axis`, plus the canvas's. `None`
    /// when the selection has no box to align.
    pub fn build(
        editor: &Editor,
        meshes: &HashMap<u32, MeshAssetGpu>,
        axis: Axis,
        threshold: f32,
    ) -> Option<Self> {
        let shapes = editor.shapes();
        let boxed = |i: usize| world_box(&editor.posed_shape(i, shapes[i]), meshes);
        let sel = fold(
            editor
                .selection()
                .iter()
                .filter_map(|&i| boxed(i))
                .flat_map(|(lo, hi)| [lo, hi]),
        )?;
        let mut targets = Vec::new();
        let mut push = |lo: Vec3, hi: Vec3| {
            let (a, b) = (along(lo, axis), along(hi, axis));
            for at in [a, (a + b) * 0.5, b] {
                targets.push(Target { at, lo, hi });
            }
        };
        let [cw, ch] = editor.canvas();
        push(Vec3::ZERO, Vec3::new(cw, ch, 0.0));
        for i in 0..shapes.len() {
            if editor.selection().contains(&i) || editor.is_hidden(i) {
                continue;
            }
            if let Some((lo, hi)) = boxed(i) {
                push(lo, hi);
            }
        }
        Some(Self {
            axis,
            lo: along(sel.0, axis),
            hi: along(sel.1, axis),
            targets,
            threshold,
        })
    }

    /// The drag's offset `t` along the axis, locked to the nearest target
    /// within reach of the moved box's low edge, centre or high edge — and
    /// the guide for where it locked.
    pub fn apply(&self, t: f32) -> (f32, Option<Guide>) {
        let mid = (self.lo + self.hi) * 0.5;
        let mut best: Option<(f32, f32, Target)> = None;
        for tg in &self.targets {
            for edge in [self.lo, mid, self.hi] {
                let d = tg.at - (edge + t);
                if d.abs() < self.threshold && best.is_none_or(|b| d.abs() < b.1) {
                    best = Some((t + d, d.abs(), *tg));
                }
            }
        }
        match best {
            Some((locked, _, tg)) => (
                locked,
                Some(Guide {
                    axis: self.axis,
                    at: tg.at,
                    lo: tg.lo,
                    hi: tg.hi,
                }),
            ),
            None => (t, None),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn near(a: Vec3, b: Vec3) -> bool {
        (a - b).length() < 1e-3
    }

    #[test]
    fn a_shapes_box_follows_its_footprint_spin_and_place() {
        let mut r = Shape::rect([100.0, 100.0], [50.0, 20.0]);
        let (lo, hi) = world_box_with(&r, None).unwrap();
        assert!(near(lo, Vec3::new(50.0, 80.0, 0.0)) && near(hi, Vec3::new(150.0, 120.0, 0.0)));
        // A quarter spin swaps the sides.
        r.rotate_by(std::f32::consts::FRAC_PI_2);
        let (lo, hi) = world_box_with(&r, None).unwrap();
        assert!(near(lo, Vec3::new(80.0, 50.0, 0.0)) && near(hi, Vec3::new(120.0, 150.0, 0.0)), "{lo:?} {hi:?}");
        // A quarter tilt stands it up: the height becomes depth.
        let mut t = Shape::rect([100.0, 100.0], [50.0, 20.0]);
        t.set_tilt(std::f32::consts::FRAC_PI_2);
        t.set_z(300.0);
        let (lo, hi) = world_box_with(&t, None).unwrap();
        assert!((hi.y - lo.y).abs() < 1e-3 && (hi.z - lo.z - 40.0).abs() < 1e-3, "{lo:?} {hi:?}");
        assert!((lo.z - 280.0).abs() < 1e-3 && (hi.x - 150.0).abs() < 1e-3);
        // A light has no box; a mesh without its model, none yet.
        assert!(world_box_with(&Shape::sun([0.0; 2]), None).is_none());
        assert!(world_box_with(&Shape::mesh([0.0; 2], [10.0; 2], 1), None).is_none());
        // A mesh's is its model's, placed.
        let m = Shape::mesh([100.0, 100.0], [50.0, 50.0], 1);
        let (lo, hi) = world_box_with(&m, Some(([-1.0; 3], [1.0; 3]))).unwrap();
        assert!(near(lo, Vec3::new(50.0, 50.0, -50.0)) && near(hi, Vec3::new(150.0, 150.0, 50.0)));
    }

    #[test]
    fn a_drag_locks_its_edges_and_centre_to_the_nearest_target() {
        let s = AxisSnap {
            axis: Axis::X,
            lo: 0.0,
            hi: 100.0,
            targets: vec![Target {
                at: 200.0,
                lo: Vec3::new(200.0, 0.0, 0.0),
                hi: Vec3::new(300.0, 10.0, 0.0),
            }],
            threshold: 10.0,
        };
        // Far off: free.
        assert_eq!(s.apply(50.0), (50.0, None));
        // The high edge nearly at the target: locked there.
        let (t, g) = s.apply(97.0);
        assert_eq!(t, 100.0);
        assert_eq!(g.map(|g| g.at), Some(200.0));
        // The centre, and the low edge, lock too.
        assert_eq!(s.apply(147.0).0, 150.0);
        assert_eq!(s.apply(204.0).0, 200.0);
        // Just out of reach: free.
        assert_eq!(s.apply(111.0).0, 111.0);
        // The guide is the target's box sliced at the lock: on X, a
        // rectangle spanning its y and z — two of its four sides here,
        // the box being flat.
        let g = g.unwrap();
        assert_eq!(g.axis, Axis::X);
        assert_eq!(g.overlays(2.0).len(), 2);
    }

    #[test]
    fn the_snap_is_built_from_the_others_and_the_canvas() {
        let mut e = Editor::empty();
        let a = e.push_shape(Shape::rect([100.0, 100.0], [50.0, 50.0]));
        let _b = e.push_shape(Shape::rect([500.0, 100.0], [100.0, 50.0]));
        e.select(Some(a));
        let snap = AxisSnap::build(&e, &HashMap::new(), Axis::X, 10.0).unwrap();
        assert_eq!((snap.lo, snap.hi), (50.0, 150.0));
        let ats: Vec<f32> = snap.targets.iter().map(|t| t.at).collect();
        // The canvas: 0, its middle, its width; then b: 400, 500, 600.
        let w = spark_render::CANVAS_W;
        assert_eq!(ats, vec![0.0, w * 0.5, w, 400.0, 500.0, 600.0]);
        // Dragging a right by 245 puts its right edge at 395: locks to 400.
        assert_eq!(snap.apply(245.0).0, 250.0);
        // Nothing selected: nothing to align.
        e.select(None);
        assert!(AxisSnap::build(&e, &HashMap::new(), Axis::X, 10.0).is_none());
    }
}
