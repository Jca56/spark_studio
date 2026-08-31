//! Built-in meshes: a plane, a cube, a sphere — the surfaces a scene
//! needs for its lights to have something to fall on, made in code
//! rather than read from a file. Alva, on first seeing shadows: "an
//! alien head and two subwoofer meshes isn't a whole lot to really see
//! what's going on" — in Ember there was a ground and trees.
//!
//! Each is an asset like an imported model, under a `builtin:` path, so
//! it rides the same rails: one asset however many times it is added,
//! an `asset` line in the comp, reloaded on open, placed by
//! `meshes::placement` from a footprint fitted the same way. Unit-sized
//! in the model's own units; the shape's size is what scales it.

use spark_render::MeshData;

use crate::meshes::Loaded;

/// The asset paths, in Add-menu order after the lights.
pub const PATHS: [&str; 3] = ["builtin:plane", "builtin:cube", "builtin:sphere"];
pub const NAMES: [&str; 3] = ["plane", "cube", "sphere"];

/// The model a `builtin:` path names, ready for the GPU; `None` for any
/// other path.
pub fn loaded(path: &str) -> Option<Loaded> {
    let k = PATHS.iter().position(|p| *p == path)?;
    let data = match k {
        0 => plane(),
        1 => cube(),
        _ => sphere(),
    };
    let bounds = bounds(&data.positions);
    Some(Loaded {
        name: NAMES[k].to_string(),
        primitives: vec![(data, None, [1.0; 3])],
        textures: Vec::new(),
        bounds,
    })
}

fn bounds(positions: &[[f32; 3]]) -> ([f32; 3], [f32; 3]) {
    let mut lo = [f32::MAX; 3];
    let mut hi = [f32::MIN; 3];
    for p in positions {
        for i in 0..3 {
            lo[i] = lo[i].min(p[i]);
            hi[i] = hi[i].max(p[i]);
        }
    }
    (lo, hi)
}

/// A 2×2 quad on the canvas plane, facing the camera. Tilt it a quarter
/// turn and it is a floor.
fn plane() -> MeshData {
    MeshData {
        positions: vec![
            [-1.0, -1.0, 0.0],
            [1.0, -1.0, 0.0],
            [1.0, 1.0, 0.0],
            [-1.0, 1.0, 0.0],
        ],
        normals: vec![[0.0, 0.0, 1.0]; 4],
        uvs: vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]],
        indices: vec![0, 1, 2, 0, 2, 3],
    }
}

/// A 2×2×2 box, four vertices a face so every face is flat.
fn cube() -> MeshData {
    let mut m = MeshData {
        positions: Vec::with_capacity(24),
        normals: Vec::with_capacity(24),
        uvs: Vec::with_capacity(24),
        indices: Vec::with_capacity(36),
    };
    // Each face: its normal, and the two axes that span it.
    let faces: [([f32; 3], [f32; 3], [f32; 3]); 6] = [
        ([1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]),
        ([-1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]),
        ([0.0, 1.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]),
        ([0.0, -1.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]),
        ([0.0, 0.0, 1.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]),
        ([0.0, 0.0, -1.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]),
    ];
    for (n, u, v) in faces {
        let base = m.positions.len() as u32;
        for (su, sv) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
            m.positions.push([
                n[0] + u[0] * su + v[0] * sv,
                n[1] + u[1] * su + v[1] * sv,
                n[2] + u[2] * su + v[2] * sv,
            ]);
            m.normals.push(n);
            m.uvs.push([su * 0.5 + 0.5, sv * 0.5 + 0.5]);
        }
        m.indices.extend([base, base + 1, base + 2, base, base + 2, base + 3]);
    }
    m
}

/// A unit sphere, 32 segments round and 16 rings top to bottom, smooth.
fn sphere() -> MeshData {
    const RINGS: u32 = 16;
    const SEGS: u32 = 32;
    let mut m = MeshData {
        positions: Vec::new(),
        normals: Vec::new(),
        uvs: Vec::new(),
        indices: Vec::new(),
    };
    for i in 0..=RINGS {
        let phi = std::f32::consts::PI * i as f32 / RINGS as f32;
        for j in 0..=SEGS {
            let theta = std::f32::consts::TAU * j as f32 / SEGS as f32;
            // y is down: the first ring is the top.
            let p = [phi.sin() * theta.cos(), -phi.cos(), phi.sin() * theta.sin()];
            m.positions.push(p);
            m.normals.push(p);
            m.uvs.push([j as f32 / SEGS as f32, i as f32 / RINGS as f32]);
        }
    }
    for i in 0..RINGS {
        for j in 0..SEGS {
            let a = i * (SEGS + 1) + j;
            let b = a + SEGS + 1;
            m.indices.extend([a, b, a + 1, a + 1, b, b + 1]);
        }
    }
    m
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
        a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
    }

    #[test]
    fn builtin_paths_load_without_a_file_and_others_dont() {
        let p = loaded("builtin:plane").unwrap();
        assert_eq!(p.name, "plane");
        assert_eq!(p.bounds, ([-1.0, -1.0, 0.0], [1.0, 1.0, 0.0]));
        assert!(p.textures.is_empty() && p.primitives.len() == 1);
        assert!(loaded("/home/alva/alien.glb").is_none());
        assert!(loaded("builtin:teapot").is_none());
        for path in PATHS {
            let l = loaded(path).unwrap();
            let (data, _, _) = &l.primitives[0];
            assert_eq!(data.positions.len(), data.normals.len());
            assert_eq!(data.positions.len(), data.uvs.len());
            assert!(data.indices.iter().all(|&i| (i as usize) < data.positions.len()));
            assert_eq!(data.indices.len() % 3, 0);
        }
    }

    #[test]
    fn the_cube_faces_point_out_and_span_two_units() {
        let c = cube();
        assert_eq!(c.positions.len(), 24);
        assert_eq!(c.indices.len(), 36);
        for (p, n) in c.positions.iter().zip(&c.normals) {
            assert!(dot(*p, *n) > 0.0, "{p:?} against {n:?}");
            assert!(p.iter().all(|v| v.abs() == 1.0));
        }
        assert_eq!(bounds(&c.positions), ([-1.0; 3], [1.0; 3]));
    }

    #[test]
    fn the_sphere_is_unit_and_smooth_with_its_top_up() {
        let s = sphere();
        for (p, n) in s.positions.iter().zip(&s.normals) {
            assert!((dot(*p, *p).sqrt() - 1.0).abs() < 1e-5);
            assert_eq!(p, n);
        }
        // The first vertex is the top of the sphere: up is −y.
        assert!((s.positions[0][1] + 1.0).abs() < 1e-6);
        let (lo, hi) = bounds(&s.positions);
        assert!((lo[0] + 1.0).abs() < 1e-5 && (hi[0] - 1.0).abs() < 1e-5);
        assert!((lo[2] + 1.0).abs() < 1e-5 && (hi[2] - 1.0).abs() < 1e-5);
    }
}
