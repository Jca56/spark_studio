//! Path editing: convert primitives to editable polylines, drag vertices,
//! add/remove points, open/close — the seven-costume feature.

use spark_render::{Shape, ShapeKind};

use super::Editor;
use crate::history::Tag;

/// Farthest vertex distance from the center.
fn bound(verts: &[[f32; 2]]) -> f32 {
    verts
        .iter()
        .map(|v| (v[0] * v[0] + v[1] * v[1]).sqrt())
        .fold(1.0f32, f32::max)
}

impl Editor {
    pub fn path(&self, id: usize) -> &[[f32; 2]] {
        self.paths.get(id).map(Vec::as_slice).unwrap_or(&[])
    }

    /// Uniform-scale one shape, vertex list included for paths.
    pub(super) fn scale_index(&mut self, i: usize, f: f32) {
        self.shapes[i].scale_by(f);
        if let Some((id, _, _)) = self.shapes[i].path_meta()
            && let Some(vs) = self.paths.get_mut(id)
        {
            for v in vs {
                v[0] *= f;
                v[1] *= f;
            }
        }
    }

    /// `P`: turn the primary shape into an editable path.
    pub fn convert_to_path(&mut self) -> bool {
        let Some(i) = self.primary() else {
            return false;
        };
        let s = self.shapes[i];
        let (verts, closed): (Vec<[f32; 2]>, bool) = match s.kind() {
            // A field has no outline to convert — it's a region full of
            // stars, not one silhouette — and a mesh is a model, not a
            // shape.
            ShapeKind::Path | ShapeKind::Stars | ShapeKind::Mesh | ShapeKind::Light => return false,
            ShapeKind::Line => {
                let c = s.center();
                let (a, b) = s.line_ends();
                (
                    vec![[a[0] - c[0], a[1] - c[1]], [b[0] - c[0], b[1] - c[1]]],
                    false,
                )
            }
            ShapeKind::Box => {
                let [w, h] = s.box_size().unwrap_or([20.0, 20.0]);
                let (hw, hh) = (w * 0.5, h * 0.5);
                (vec![[-hw, -hh], [hw, -hh], [hw, hh], [-hw, hh]], true)
            }
            ShapeKind::Ngon => {
                let n = s.sides().unwrap_or(5).max(3);
                let r = s.size();
                let verts = (0..n)
                    .map(|k| {
                        let th = -std::f32::consts::FRAC_PI_2
                            + k as f32 * std::f32::consts::TAU / n as f32;
                        [r * th.cos(), r * th.sin()]
                    })
                    .collect();
                (verts, true)
            }
            ShapeKind::Circle => {
                let [w, h] = s.box_size().unwrap_or([20.0, 20.0]);
                let (rx, ry) = (w * 0.5, h * 0.5);
                let verts = (0..16)
                    .map(|k| {
                        let th = k as f32 * std::f32::consts::TAU / 16.0;
                        [rx * th.cos(), ry * th.sin()]
                    })
                    .collect();
                (verts, true)
            }
        };
        let undo = self.snap();
        self.history.push(undo);
        let id = self.paths.len();
        let b = bound(&verts);
        let count = verts.len();
        self.paths.push(verts);
        // Lines fold their direction into the vertices; everything else
        // keeps its rotation on the shape.
        let rot = if s.kind() == ShapeKind::Line {
            0.0
        } else {
            s.rotation()
        };
        let thickness = s.thickness().unwrap_or(4.0);
        let mut p = Shape::path(s.center(), id, count, closed, b, thickness).rot(rot);
        p.set_rgb(s.rgb());
        p.set_brightness(s.brightness());
        p.set_glow(s.glow_radius());
        p.set_additive(s.additive());
        self.shapes[i] = p;
        println!("converted to path ({count} points)");
        true
    }

    /// `O`: open or close the primary path (open triangle = arrow).
    pub fn toggle_path_closed(&mut self) -> bool {
        let Some(i) = self.primary() else {
            return false;
        };
        let Some((id, count, closed)) = self.shapes[i].path_meta() else {
            return false;
        };
        let undo = self.snap();
        self.history.push(undo);
        let b = bound(self.path(id));
        self.shapes[i].set_path_shape(count, !closed, b);
        println!("path {}", if closed { "opened" } else { "closed" });
        true
    }

    /// `=`: insert a vertex at the midpoint of the segment nearest the
    /// cursor.
    pub fn add_vertex(&mut self) -> bool {
        let Some(i) = self.primary() else {
            return false;
        };
        let Some((id, count, closed)) = self.shapes[i].path_meta() else {
            return false;
        };
        if count < 2 {
            return false;
        }
        let local = self.cursor_local(i);
        let vs = &self.paths[id];
        let segs = if closed { count } else { count - 1 };
        let mut best = (0usize, f32::MAX);
        for k in 0..segs {
            let a = vs[k];
            let b = vs[(k + 1) % count];
            let m = [(a[0] + b[0]) * 0.5, (a[1] + b[1]) * 0.5];
            let d = (m[0] - local[0]).powi(2) + (m[1] - local[1]).powi(2);
            if d < best.1 {
                best = (k, d);
            }
        }
        let undo = self.snap();
        self.history.push(undo);
        let vs = &mut self.paths[id];
        let a = vs[best.0];
        let b2 = vs[(best.0 + 1) % count];
        vs.insert(best.0 + 1, [(a[0] + b2[0]) * 0.5, (a[1] + b2[1]) * 0.5]);
        let bnd = bound(&self.paths[id]);
        self.shapes[i].set_path_shape(count + 1, closed, bnd);
        println!("vertex added ({} points)", count + 1);
        true
    }

    /// `-`: remove the vertex nearest the cursor (paths keep at least 2).
    pub fn remove_vertex(&mut self) -> bool {
        let Some(i) = self.primary() else {
            return false;
        };
        let Some((id, count, closed)) = self.shapes[i].path_meta() else {
            return false;
        };
        if count <= 2 {
            return false;
        }
        let local = self.cursor_local(i);
        let vs = &self.paths[id];
        let mut best = (0usize, f32::MAX);
        for (k, v) in vs.iter().enumerate() {
            let d = (v[0] - local[0]).powi(2) + (v[1] - local[1]).powi(2);
            if d < best.1 {
                best = (k, d);
            }
        }
        let undo = self.snap();
        self.history.push(undo);
        self.paths[id].remove(best.0);
        let bnd = bound(&self.paths[id]);
        self.shapes[i].set_path_shape(count - 1, closed, bnd);
        println!("vertex removed ({} points)", count - 1);
        true
    }

    /// Drag vertex `k` of the primary path to the cursor (handle drag).
    pub fn drag_vertex(&mut self, k: usize, cur: [f32; 2]) -> bool {
        let Some(i) = self.primary() else {
            return false;
        };
        let Some((id, count, closed)) = self.shapes[i].path_meta() else {
            return false;
        };
        if k >= count {
            return false;
        }
        self.record(Tag::Handle);
        let c = self.shapes[i].center();
        let rot = self.shapes[i].rotation();
        let (sn, cs) = (-rot).sin_cos();
        let d = [cur[0] - c[0], cur[1] - c[1]];
        self.paths[id][k] = [d[0] * cs - d[1] * sn, d[0] * sn + d[1] * cs];
        let bnd = bound(&self.paths[id]);
        self.shapes[i].set_path_shape(count, closed, bnd);
        true
    }

    /// Polyline pick distance for path shapes (world space).
    pub(super) fn path_pick(&self, s: &Shape, p: [f32; 2]) -> f32 {
        let Some((id, count, closed)) = s.path_meta() else {
            return f32::MAX;
        };
        let vs = self.path(id);
        if vs.len() < count.max(2) {
            return f32::MAX;
        }
        let c = s.center();
        let rot = s.rotation();
        let (sn, cs) = (-rot).sin_cos();
        let d = [p[0] - c[0], p[1] - c[1]];
        let local = [d[0] * cs - d[1] * sn, d[0] * sn + d[1] * cs];
        let segs = if closed && count > 2 {
            count
        } else {
            count - 1
        };
        let mut md = f32::MAX;
        for k in 0..segs {
            md = md.min(seg_dist(local, vs[k], vs[(k + 1) % count]));
        }
        md - s.thickness().unwrap_or(2.0)
    }

    /// Cursor mapped into the shape's local (center-relative, unrotated)
    /// frame.
    fn cursor_local(&self, i: usize) -> [f32; 2] {
        let c = self.shapes[i].center();
        let (sn, cs) = (-self.shapes[i].rotation()).sin_cos();
        let d = [self.cursor[0] - c[0], self.cursor[1] - c[1]];
        [d[0] * cs - d[1] * sn, d[0] * sn + d[1] * cs]
    }
}

fn seg_dist(p: [f32; 2], a: [f32; 2], b: [f32; 2]) -> f32 {
    let pa = [p[0] - a[0], p[1] - a[1]];
    let ba = [b[0] - a[0], b[1] - a[1]];
    let h = ((pa[0] * ba[0] + pa[1] * ba[1]) / (ba[0] * ba[0] + ba[1] * ba[1]).max(0.0001))
        .clamp(0.0, 1.0);
    ((pa[0] - ba[0] * h).powi(2) + (pa[1] - ba[1] * h).powi(2)).sqrt()
}
