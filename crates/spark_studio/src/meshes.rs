//! Mesh objects, the app side: reading a model off-thread, putting it on
//! the GPU, and placing it every frame from the shape that draws it.
//!
//! A mesh file is read and its textures decoded on a worker thread — the
//! logo is 68 MB and a 4K JPEG takes FFmpeg a moment — and arrives as an
//! [`AppEvent::MeshLoaded`]; the upload happens here, on the thread that
//! owns the device. A fresh import also gets its shape then, fitted from
//! the model's bounds, since the bounds aren't known until it's read.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use spark_render::{GpuMesh, Mat4, MeshData, MeshInstance, Shape, TextureData, Vec3};

use crate::{AppEvent, Studio};

/// What the loader thread hands back: geometry and base-colour factor per
/// primitive, the base-colour textures those reference (decoded, with
/// mips), and the model's bounds in its own units, Spark's frame.
pub struct Loaded {
    pub name: String,
    pub primitives: Vec<(MeshData, Option<usize>, [f32; 3])>,
    pub textures: Vec<TextureData>,
    pub bounds: ([f32; 3], [f32; 3]),
}

/// A model on the GPU: one mesh per primitive with its material's colour
/// factor, and the bounds the shape's footprint was fitted from.
pub struct MeshAssetGpu {
    pub meshes: Vec<(GpuMesh, [f32; 3])>,
    pub bounds: ([f32; 3], [f32; 3]),
}

/// Read a model and decode what it needs to draw. Runs on a worker. A
/// `builtin:` path is one of the primitives, made rather than read.
pub fn load(path: &Path) -> Result<Loaded, String> {
    if let Some(built) = crate::primitives::loaded(&path.to_string_lossy()) {
        return Ok(built);
    }
    let model = spark_assets::load(path).map_err(|e| e.to_string())?;
    let bounds = model
        .bounds
        .ok_or_else(|| "the model has no geometry".to_string())?;
    let mut textures = Vec::new();
    let mut by_image: HashMap<usize, Option<usize>> = HashMap::new();
    let mut primitives = Vec::with_capacity(model.primitives.len());
    for p in model.primitives {
        let material = p.material.and_then(|m| model.materials.get(m));
        let factor = material
            .map(|m| [m.base_color[0], m.base_color[1], m.base_color[2]])
            .unwrap_or([1.0; 3]);
        let texture = match material.and_then(|m| m.base_color_texture) {
            Some(image) => *by_image.entry(image).or_insert_with(|| {
                let bytes = &model.images.get(image)?.bytes;
                match spark_assets::image::decode(bytes) {
                    Ok(rgba) => {
                        let (width, height) = (rgba.width, rgba.height);
                        let levels = spark_assets::image::mips(rgba)
                            .into_iter()
                            .map(|l| l.pixels)
                            .collect();
                        textures.push(TextureData {
                            width,
                            height,
                            levels,
                        });
                        Some(textures.len() - 1)
                    }
                    Err(e) => {
                        println!("texture {image} of {}: {e} — drawing without it", model.name);
                        None
                    }
                }
            }),
            None => None,
        };
        primitives.push((
            MeshData {
                positions: p.positions,
                normals: p.normals,
                uvs: p.uvs,
                indices: p.indices,
            },
            texture,
            factor,
        ));
    }
    Ok(Loaded {
        name: model.name,
        primitives,
        textures,
        bounds: (bounds.min, bounds.max),
    })
}

/// The mesh's own units → the shape's footprint on its plane: centred on
/// the shape, scaled so the model's larger side spans the shape's size
/// (the footprint was fitted the same way, so the aspect agrees), and
/// spun by the shape's rotation about the plane's normal — the same turn
/// the 2D field makes, x toward y, so the model and its footprint turn
/// together. (They didn't, until 2026-08-31: Rotation on a mesh's card
/// and the Spin ring turned the box and not the model.)
///
/// Width and height scale the model's x and y each to the footprint's
/// side, so a stretched plane is a floor and a stretched cube a slab;
/// depth follows the smaller of the two, so a slab is thin. A footprint
/// fitted from the model (`mesh_shape`) keeps its aspect and scales
/// uniformly, as it always did.
pub fn placement(s: &Shape, (lo, hi): ([f32; 3], [f32; 3])) -> Mat4 {
    let half = s
        .box_size()
        .map(|[w, h]| [w * 0.5, h * 0.5])
        .unwrap_or([s.size(), s.size()]);
    let kx = half[0] / ((hi[0] - lo[0]) * 0.5).max(1e-6);
    let ky = half[1] / ((hi[1] - lo[1]) * 0.5).max(1e-6);
    let kz = match s.depth() {
        Some(d) if d > 0.0 => d / (hi[2] - lo[2]).max(1e-6),
        _ => kx.min(ky),
    };
    let c = s.center();
    let bc = Vec3::new(
        (lo[0] + hi[0]) * 0.5,
        (lo[1] + hi[1]) * 0.5,
        (lo[2] + hi[2]) * 0.5,
    );
    Mat4::translation(Vec3::new(c[0], c[1], 0.0))
        * Mat4::rotation_z(s.rotation())
        * Mat4::scaling(Vec3::new(kx, ky, kz))
        * Mat4::translation(-bc)
}

/// This frame's mesh instances: one per primitive of every mesh shape in
/// `shapes` (display copies — posed, reacted, effects applied) whose model
/// is on the GPU. A mesh still loading draws nothing yet.
pub(crate) fn instances<'a>(
    cache: &'a HashMap<u32, MeshAssetGpu>,
    shapes: &[Shape],
) -> Vec<MeshInstance<'a>> {
    let mut out = Vec::new();
    for s in shapes {
        let Some(asset) = s.mesh_asset().and_then(|id| cache.get(&id)) else {
            continue;
        };
        let model = s.model() * placement(s, asset.bounds);
        let rgb = s.rgb();
        let e = s.brightness();
        for (mesh, factor) in &asset.meshes {
            out.push(MeshInstance {
                mesh,
                model,
                color: [
                    rgb[0] * e * factor[0],
                    rgb[1] * e * factor[1],
                    rgb[2] * e * factor[2],
                    s.opacity(),
                ],
                unlit: false,
            });
        }
    }
    out
}

impl Studio {
    /// File > Import Mesh…: read it off-thread; the shape appears when
    /// the model arrives.
    pub(crate) fn import_mesh(&mut self, path: PathBuf) {
        self.spawn_mesh_load(None, path);
    }

    /// Load every asset the comp names that isn't on the GPU yet.
    pub(crate) fn sync_meshes(&mut self) {
        let missing: Vec<(u32, String)> = self
            .editor
            .assets()
            .iter()
            .filter(|a| !self.meshes.contains_key(&a.id))
            .map(|a| (a.id, a.path.clone()))
            .collect();
        for (id, path) in missing {
            self.spawn_mesh_load(Some(id), PathBuf::from(path));
        }
    }

    pub(crate) fn spawn_mesh_load(&mut self, id: Option<u32>, path: PathBuf) {
        self.mesh_loading += 1;
        println!("reading mesh {}", path.display());
        let proxy = self.proxy.clone();
        std::thread::spawn(move || {
            let result = load(&path);
            let _ = proxy.send_event(AppEvent::MeshLoaded(
                id,
                path.to_string_lossy().into_owned(),
                result,
            ));
        });
    }

    /// A model arrived: upload it, and for a fresh import give it a shape.
    pub(crate) fn mesh_loaded(
        &mut self,
        id: Option<u32>,
        path: String,
        result: Result<Loaded, String>,
    ) {
        self.mesh_loading = self.mesh_loading.saturating_sub(1);
        let loaded = match result {
            Ok(l) => l,
            Err(e) => {
                println!("mesh import failed: {e}");
                return;
            }
        };
        let (Some(gpu), Some(stage)) = (&self.gpu, &mut self.stage) else {
            return;
        };
        let meshes: Vec<(GpuMesh, [f32; 3])> = loaded
            .primitives
            .iter()
            .map(|(data, tex, factor)| {
                let t = tex.and_then(|k| loaded.textures.get(k));
                (stage.upload_mesh(&gpu.device, &gpu.queue, data, t), *factor)
            })
            .collect();
        let count = meshes.len();
        let id = match id {
            Some(id) => id,
            None => {
                let id = self.editor.add_asset(path);
                self.editor.add_mesh_shape(id, &loaded.name, loaded.bounds);
                id
            }
        };
        self.editor.backfill_mesh_depth(id, loaded.bounds);
        self.meshes.insert(
            id,
            MeshAssetGpu {
                meshes,
                bounds: loaded.bounds,
            },
        );
        println!(
            "mesh ready: {} — {count} primitive{}",
            loaded.name,
            if count == 1 { "" } else { "s" }
        );
    }
}

#[cfg(test)]
mod tests {
    /// Spin turns the model with its footprint: a quarter turn carries
    /// the model's +x side to the shape's +y — down the canvas, the way
    /// the 2D field turns for a positive rotation.
    #[test]
    fn placement_spins_the_model_with_the_shape() {
        use spark_render::{Shape, Vec3};
        let bounds = ([-1.0, -1.0, -1.0], [1.0, 1.0, 1.0]);
        let mut s = Shape::rect([500.0, 300.0], [100.0, 100.0]);
        let flat = super::placement(&s, bounds).transform_point(Vec3::new(1.0, 0.0, 0.0));
        assert!((flat - Vec3::new(500.0 + s.size(), 300.0, 0.0)).length() < 1e-3, "{flat:?}");
        s.rotate_by(std::f32::consts::FRAC_PI_2);
        let spun = super::placement(&s, bounds).transform_point(Vec3::new(1.0, 0.0, 0.0));
        assert!((spun - Vec3::new(500.0, 300.0 + s.size(), 0.0)).length() < 1e-3, "{spun:?}");
    }

    use super::*;
    use crate::editor::{mesh_fit, mesh_shape};
    use spark_render::{CANVAS, CANVAS_H, CANVAS_W};

    /// A stretched footprint stretches the model with it, its depth
    /// following the thinner side.
    #[test]
    fn a_stretched_footprint_stretches_the_model() {
        let bounds = ([-1.0, -1.0, -1.0], [1.0, 1.0, 1.0]);
        let mut m = Shape::mesh([0.0, 0.0], [1.0, 1.0], 1);
        m.set_box_width(600.0);
        m.set_box_height(100.0);
        let p = placement(&m, bounds).transform_point(Vec3::new(1.0, 1.0, 1.0));
        assert!((p - Vec3::new(300.0, 50.0, 50.0)).length() < 1e-3, "{p:?}");
        // With a depth of its own, the third side is that.
        m.set_depth(800.0);
        let p = placement(&m, bounds).transform_point(Vec3::new(1.0, 1.0, 1.0));
        assert!((p - Vec3::new(300.0, 50.0, 400.0)).length() < 1e-3, "{p:?}");
        // A fitted import carries the model's depth at its scale.
        let fitted = mesh_shape(1, ([-1.0, -0.5, -0.25], [1.0, 0.5, 0.25]), CANVAS);
        let k = mesh_fit(CANVAS) / 2.0;
        assert!((fitted.depth().unwrap() - 0.5 * k).abs() < 1e-3);
    }

    /// About the logo's bounds: wide, short, thin.
    const LOGO: ([f32; 3], [f32; 3]) = ([-0.95, -0.48, -0.05], [0.95, 0.48, 0.06]);

    #[test]
    fn an_imported_mesh_is_fitted_and_centred() {
        let s = mesh_shape(7, LOGO, CANVAS);
        assert_eq!(s.mesh_asset(), Some(7));
        assert_eq!(s.center(), [CANVAS_W * 0.5, CANVAS_H * 0.5]);
        // The wide side spans MESH_FIT; the short side keeps the aspect.
        let half = s.mesh_half().unwrap();
        assert!((half[0] * 2.0 - mesh_fit(CANVAS)).abs() < 1e-3, "{half:?}");
        assert!((half[1] / half[0] - 0.96 / 1.9).abs() < 1e-3, "{half:?}");
        assert_eq!(s.rgb(), [1.0, 1.0, 1.0]);
    }

    #[test]
    fn placement_maps_the_bounds_onto_the_footprint() {
        let s = mesh_shape(1, LOGO, CANVAS);
        let m = placement(&s, LOGO);
        let c = s.center();
        // The bounds' centre lands on the shape's centre, on its plane.
        let mid = m.transform_point(Vec3::new(0.0, 0.0, 0.005));
        assert!((mid.x - c[0]).abs() < 1e-3 && (mid.y - c[1]).abs() < 1e-3 && mid.z.abs() < 1e-3);
        // The far right of the model lands `size` to the right.
        let right = m.transform_point(Vec3::new(0.95, 0.0, 0.0));
        assert!((right.x - (c[0] + s.size())).abs() < 1e-2, "{right:?}");
        // Scaling the shape scales the mesh with it.
        let mut big = s;
        big.scale_by(2.0);
        let right = placement(&big, LOGO).transform_point(Vec3::new(0.95, 0.0, 0.0));
        assert!((right.x - (c[0] + s.size() * 2.0)).abs() < 1e-2, "{right:?}");
    }

    #[test]
    fn instances_come_only_from_loaded_meshes() {
        let cache = HashMap::new();
        let shapes = [mesh_shape(1, LOGO, CANVAS), Shape::circle([0.0; 2], 5.0)];
        assert!(instances(&cache, &shapes).is_empty());
    }

    /// The whole import path on the real logo, when it is where it lives:
    /// what File > Import Mesh… waits for.
    #[test]
    fn the_logo_loads_for_the_gpu() {
        let path = Path::new("/home/alva/alva_logo_3d.glb");
        if !path.exists() {
            eprintln!("no logo — skipping");
            return;
        }
        let t = std::time::Instant::now();
        let l = load(path).expect("logo loads");
        eprintln!("logo read + textures decoded in {:?}", t.elapsed());
        assert_eq!(l.primitives.len(), 1);
        assert_eq!(l.primitives[0].1, Some(0), "the base colour texture");
        assert_eq!(l.textures.len(), 1, "one texture decoded, not three");
        let tex = &l.textures[0];
        eprintln!("base colour {}×{}, {} mip levels", tex.width, tex.height, tex.levels.len());
        assert!(tex.width >= 256 && tex.levels.len() > 1);
        assert_eq!(tex.levels[0].len(), (tex.width * tex.height * 4) as usize);
        let s = mesh_shape(1, l.bounds, CANVAS);
        assert!((s.mesh_half().unwrap()[0] * 2.0 - mesh_fit(CANVAS)).abs() < 1e-3);
    }
}
