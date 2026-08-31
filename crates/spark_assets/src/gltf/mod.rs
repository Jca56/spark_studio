//! glTF 2.0 → a [`Model`]: every mesh in the scene, flattened through its
//! node transforms into Spark's frame, with the materials and the encoded
//! images they reference.
//!
//! glTF's frame is +y up, +z toward the viewer; Spark's canvas is y down
//! and z away. The two differ by a half turn about x — a proper rotation,
//! so triangle winding survives — and every vertex comes out already
//! turned, so a mesh drawn with an identity transform stands upright and
//! faces the camera. Units are whatever the file's were (glTF says
//! metres); fitting a model to the canvas is the object's business.
//!
//! Read now: positions, normals (flat ones computed where a file has
//! none), the first UV set, indices (strips and fans unrolled), materials'
//! factors and texture references, images as their stored bytes. Not yet:
//! skins, animations, morph targets, sparse accessors — the rig work of a
//! later milestone.

mod accessor;
#[cfg(test)]
mod tests;

use std::path::Path;

use spark_render::{Mat4, Vec3};

use crate::glb::{self, Container};
use crate::json::Json;
use crate::{Error, invalid};
use accessor::accessor;

pub struct Model {
    pub name: String,
    /// Every triangle primitive in the scene, in Spark's frame.
    pub primitives: Vec<Primitive>,
    pub materials: Vec<Material>,
    /// Encoded as stored — JPEG, PNG — for the renderer to decode.
    pub images: Vec<Image>,
    /// Over every primitive; `None` for an empty model.
    pub bounds: Option<Bounds>,
}

pub struct Primitive {
    pub name: String,
    pub positions: Vec<[f32; 3]>,
    /// Unit length, one per position.
    pub normals: Vec<[f32; 3]>,
    /// One per position, or empty when the file has none.
    pub uvs: Vec<[f32; 2]>,
    /// Triangles, three indices each.
    pub indices: Vec<u32>,
    pub material: Option<usize>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Material {
    pub name: String,
    pub base_color: [f32; 4],
    /// Indices into [`Model::images`].
    pub base_color_texture: Option<usize>,
    pub metallic: f32,
    pub roughness: f32,
    pub metallic_roughness_texture: Option<usize>,
    pub normal_texture: Option<usize>,
    /// `emissiveFactor` with `KHR_materials_emissive_strength` folded in.
    pub emissive: [f32; 3],
    pub emissive_texture: Option<usize>,
    pub double_sided: bool,
    /// `KHR_materials_unlit`: draw the colour, skip the lighting.
    pub unlit: bool,
}

pub struct Image {
    pub name: String,
    pub mime: String,
    pub bytes: Vec<u8>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bounds {
    pub min: [f32; 3],
    pub max: [f32; 3],
}

impl Bounds {
    pub fn size(&self) -> [f32; 3] {
        std::array::from_fn(|i| self.max[i] - self.min[i])
    }

    pub fn centre(&self) -> [f32; 3] {
        std::array::from_fn(|i| (self.max[i] + self.min[i]) * 0.5)
    }
}

/// Read a `.glb` or `.gltf` from disk.
pub fn load(path: &Path) -> Result<Model, Error> {
    let c = glb::open(path)?;
    let name = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    from_container(&c, &name, path.parent().unwrap_or(Path::new(".")))
}

/// Build a model from an already-parsed container. `base` is where the
/// document's relative image URIs resolve.
pub fn from_container(c: &Container, name: &str, base: &Path) -> Result<Model, Error> {
    let json = &c.json;
    let materials = materials(json)?;
    let images = images(json, &c.buffers, base)?;
    let mut primitives = Vec::new();
    let to_spark = Mat4::scaling(Vec3::new(1.0, -1.0, -1.0));
    for root in root_nodes(json) {
        walk(json, &c.buffers, root, to_spark, &mut primitives, 0)?;
    }
    let bounds = bounds(&primitives);
    Ok(Model {
        name: name.to_string(),
        primitives,
        materials,
        images,
        bounds,
    })
}

/// The scene's root nodes — or, for a document with no scene, every node
/// nobody claims as a child.
fn root_nodes(json: &Json) -> Vec<usize> {
    let scene = json.get("scene").and_then(Json::as_usize).unwrap_or(0);
    if let Some(nodes) = json
        .get("scenes")
        .and_then(|s| s.at(scene))
        .and_then(|s| s.get("nodes"))
        .and_then(Json::as_array)
    {
        return nodes.iter().filter_map(Json::as_usize).collect();
    }
    let nodes = json.get("nodes").and_then(Json::as_array).unwrap_or(&[]);
    let mut claimed = vec![false; nodes.len()];
    for n in nodes {
        for c in n.get("children").and_then(Json::as_array).unwrap_or(&[]) {
            if let Some(i) = c.as_usize().filter(|&i| i < nodes.len()) {
                claimed[i] = true;
            }
        }
    }
    (0..nodes.len()).filter(|&i| !claimed[i]).collect()
}

/// A node's own transform: the `matrix` if it has one, else T·R·S.
fn local_transform(node: &Json) -> Result<Mat4, Error> {
    if let Some(m) = node.get("matrix").and_then(Json::f32s) {
        let arr: [f32; 16] = m
            .try_into()
            .map_err(|_| invalid("node matrix is not 16 numbers"))?;
        // glTF stores matrices column-major, exactly as `Mat4` does.
        return Ok(Mat4(arr));
    }
    let v3 = |key: &str, default: [f32; 3]| -> Result<Vec3, Error> {
        match node.get(key).and_then(Json::f32s) {
            None => Ok(Vec3::new(default[0], default[1], default[2])),
            Some(v) if v.len() == 3 => Ok(Vec3::new(v[0], v[1], v[2])),
            Some(_) => Err(invalid(format!("node `{key}` is not 3 numbers"))),
        }
    };
    let t = Mat4::translation(v3("translation", [0.0; 3])?);
    let r = match node.get("rotation").and_then(Json::f32s) {
        None => Mat4::IDENTITY,
        Some(q) if q.len() == 4 => Mat4::from_quat([q[0], q[1], q[2], q[3]]),
        Some(_) => return Err(invalid("node `rotation` is not 4 numbers")),
    };
    let s = Mat4::scaling(v3("scale", [1.0; 3])?);
    Ok(t * r * s)
}

fn walk(
    json: &Json,
    buffers: &[Vec<u8>],
    index: usize,
    parent: Mat4,
    out: &mut Vec<Primitive>,
    depth: usize,
) -> Result<(), Error> {
    if depth > 64 {
        return Err(invalid("node tree is deeper than 64 — a cycle?"));
    }
    let node = json
        .get("nodes")
        .and_then(|n| n.at(index))
        .ok_or_else(|| invalid(format!("node {index} does not exist")))?;
    let world = parent * local_transform(node)?;
    if let Some(m) = node.get("mesh").and_then(Json::as_usize) {
        mesh_primitives(json, buffers, m, world, out)?;
    }
    for c in node.get("children").and_then(Json::as_array).unwrap_or(&[]) {
        let ci = c.as_usize().ok_or_else(|| invalid("child index is not a number"))?;
        walk(json, buffers, ci, world, out, depth + 1)?;
    }
    Ok(())
}

/// Every triangle primitive of `meshes[m]`, transformed by `world`.
/// Points and lines are skipped: a mesh in Spark is a surface.
fn mesh_primitives(
    json: &Json,
    buffers: &[Vec<u8>],
    m: usize,
    world: Mat4,
    out: &mut Vec<Primitive>,
) -> Result<(), Error> {
    let mesh = json
        .get("meshes")
        .and_then(|l| l.at(m))
        .ok_or_else(|| invalid(format!("mesh {m} does not exist")))?;
    let name = mesh.get("name").and_then(Json::as_str).unwrap_or("").to_string();
    // Normals turn by the inverse transpose, so a non-uniform scale
    // doesn't shear them; a flat matrix falls back to the rotation itself.
    let normal_m = world.inverse().map(|i| i.transpose()).unwrap_or(world);
    for p in mesh.get("primitives").and_then(Json::as_array).unwrap_or(&[]) {
        let mode = p.get("mode").and_then(Json::as_usize).unwrap_or(4);
        if !(4..=6).contains(&mode) {
            continue;
        }
        let attrs = p
            .get("attributes")
            .ok_or_else(|| invalid("primitive has no attributes"))?;
        let attr = |key: &str| attrs.get(key).and_then(Json::as_usize);
        let pos_i = attr("POSITION").ok_or_else(|| invalid("primitive has no POSITION"))?;
        let mut positions: Vec<[f32; 3]> = accessor(json, buffers, pos_i)?.vecs()?;
        let mut normals: Vec<[f32; 3]> = match attr("NORMAL") {
            Some(i) => accessor(json, buffers, i)?.vecs()?,
            None => Vec::new(),
        };
        let mut uvs: Vec<[f32; 2]> = match attr("TEXCOORD_0") {
            Some(i) => accessor(json, buffers, i)?.vecs()?,
            None => Vec::new(),
        };
        let mut indices = match p.get("indices").and_then(Json::as_usize) {
            Some(i) => accessor(json, buffers, i)?.indices()?,
            None => (0..positions.len() as u32).collect(),
        };
        if indices.iter().any(|&i| i as usize >= positions.len()) {
            return Err(invalid("an index points past the vertices"));
        }
        if !normals.is_empty() && normals.len() != positions.len()
            || !uvs.is_empty() && uvs.len() != positions.len()
        {
            return Err(invalid("attribute counts differ"));
        }
        indices = match mode {
            5 => strip_to_triangles(&indices),
            6 => fan_to_triangles(&indices),
            _ => indices,
        };
        indices.truncate(indices.len() / 3 * 3);
        if normals.is_empty() {
            (positions, uvs, indices, normals) = flat_normals(positions, uvs, indices);
        }
        for v in &mut positions {
            let q = world.transform_point(Vec3::new(v[0], v[1], v[2]));
            *v = [q.x, q.y, q.z];
        }
        for n in &mut normals {
            let q = normal_m.transform_vec(Vec3::new(n[0], n[1], n[2])).normalized();
            *n = [q.x, q.y, q.z];
        }
        out.push(Primitive {
            name: name.clone(),
            positions,
            normals,
            uvs,
            indices,
            material: p.get("material").and_then(Json::as_usize),
        });
    }
    Ok(())
}

/// A triangle strip as a triangle list; odd triangles flip to keep every
/// face wound the same way.
fn strip_to_triangles(s: &[u32]) -> Vec<u32> {
    let mut out = Vec::with_capacity(s.len().saturating_sub(2) * 3);
    for i in 2..s.len() {
        if i % 2 == 0 {
            out.extend_from_slice(&[s[i - 2], s[i - 1], s[i]]);
        } else {
            out.extend_from_slice(&[s[i - 1], s[i - 2], s[i]]);
        }
    }
    out
}

fn fan_to_triangles(s: &[u32]) -> Vec<u32> {
    let mut out = Vec::with_capacity(s.len().saturating_sub(2) * 3);
    for i in 2..s.len() {
        out.extend_from_slice(&[s[0], s[i - 1], s[i]]);
    }
    out
}

/// Unweld every triangle and give its three vertices the face normal —
/// what the spec asks for when a primitive carries no normals.
#[allow(clippy::type_complexity)]
fn flat_normals(
    positions: Vec<[f32; 3]>,
    uvs: Vec<[f32; 2]>,
    indices: Vec<u32>,
) -> (Vec<[f32; 3]>, Vec<[f32; 2]>, Vec<u32>, Vec<[f32; 3]>) {
    let n = indices.len();
    let mut pos = Vec::with_capacity(n);
    let mut uv = Vec::with_capacity(if uvs.is_empty() { 0 } else { n });
    let mut nrm = Vec::with_capacity(n);
    for tri in indices.chunks_exact(3) {
        let [a, b, c] = [tri[0], tri[1], tri[2]].map(|i| {
            let p = positions[i as usize];
            Vec3::new(p[0], p[1], p[2])
        });
        let fnorm = (b - a).cross(c - a).normalized();
        for &i in tri {
            pos.push(positions[i as usize]);
            if !uvs.is_empty() {
                uv.push(uvs[i as usize]);
            }
            nrm.push([fnorm.x, fnorm.y, fnorm.z]);
        }
    }
    (pos, uv, (0..n as u32).collect(), nrm)
}

fn bounds(primitives: &[Primitive]) -> Option<Bounds> {
    let mut b: Option<Bounds> = None;
    for p in primitives {
        for v in &p.positions {
            let cur = b.get_or_insert(Bounds { min: *v, max: *v });
            for ((lo, hi), x) in cur.min.iter_mut().zip(cur.max.iter_mut()).zip(v) {
                *lo = lo.min(*x);
                *hi = hi.max(*x);
            }
        }
    }
    b
}

/// `textures[t].source`: the image a texture reads.
fn texture_image(json: &Json, slot: Option<&Json>) -> Option<usize> {
    let t = slot?.get("index")?.as_usize()?;
    json.get("textures")?.at(t)?.get("source")?.as_usize()
}

fn materials(json: &Json) -> Result<Vec<Material>, Error> {
    let list = json.get("materials").and_then(Json::as_array).unwrap_or(&[]);
    list.iter()
        .map(|m| {
            let pbr = m.get("pbrMetallicRoughness");
            let f = |slot: Option<&Json>, key: &str, default: f32| {
                slot.and_then(|s| s.get(key)).and_then(Json::as_f32).unwrap_or(default)
            };
            let base_color = pbr
                .and_then(|p| p.get("baseColorFactor"))
                .and_then(Json::f32s)
                .filter(|v| v.len() == 4)
                .map(|v| [v[0], v[1], v[2], v[3]])
                .unwrap_or([1.0; 4]);
            let ext = m.get("extensions");
            let strength = f(
                ext.and_then(|e| e.get("KHR_materials_emissive_strength")),
                "emissiveStrength",
                1.0,
            );
            let emissive = m
                .get("emissiveFactor")
                .and_then(Json::f32s)
                .filter(|v| v.len() == 3)
                .map(|v| [v[0] * strength, v[1] * strength, v[2] * strength])
                .unwrap_or([0.0; 3]);
            Ok(Material {
                name: m.get("name").and_then(Json::as_str).unwrap_or("").to_string(),
                base_color,
                base_color_texture: texture_image(json, pbr.and_then(|p| p.get("baseColorTexture"))),
                metallic: f(pbr, "metallicFactor", 1.0),
                roughness: f(pbr, "roughnessFactor", 1.0),
                metallic_roughness_texture: texture_image(
                    json,
                    pbr.and_then(|p| p.get("metallicRoughnessTexture")),
                ),
                normal_texture: texture_image(json, m.get("normalTexture")),
                emissive,
                emissive_texture: texture_image(json, m.get("emissiveTexture")),
                double_sided: m.get("doubleSided").and_then(Json::as_bool).unwrap_or(false),
                unlit: ext.and_then(|e| e.get("KHR_materials_unlit")).is_some(),
            })
        })
        .collect()
}

fn images(json: &Json, buffers: &[Vec<u8>], base: &Path) -> Result<Vec<Image>, Error> {
    let list = json.get("images").and_then(Json::as_array).unwrap_or(&[]);
    list.iter()
        .enumerate()
        .map(|(i, im)| {
            let name = im.get("name").and_then(Json::as_str).unwrap_or("").to_string();
            let mut mime = im.get("mimeType").and_then(Json::as_str).unwrap_or("").to_string();
            let bytes = if let Some(bv_i) = im.get("bufferView").and_then(Json::as_usize) {
                let bv = json
                    .get("bufferViews")
                    .and_then(|l| l.at(bv_i))
                    .ok_or_else(|| invalid(format!("image {i}: bufferView {bv_i} does not exist")))?;
                let buf = bv
                    .get("buffer")
                    .and_then(Json::as_usize)
                    .and_then(|b| buffers.get(b))
                    .ok_or_else(|| invalid(format!("image {i}: bad buffer")))?;
                let off = bv.get("byteOffset").and_then(Json::as_usize).unwrap_or(0);
                let len = bv.get("byteLength").and_then(Json::as_usize).unwrap_or(0);
                buf.get(off..off + len)
                    .ok_or_else(|| invalid(format!("image {i} runs past its buffer")))?
                    .to_vec()
            } else if let Some(uri) = im.get("uri").and_then(Json::as_str) {
                if mime.is_empty() {
                    mime = match uri.rsplit('.').next().map(str::to_ascii_lowercase).as_deref() {
                        Some("jpg" | "jpeg") => "image/jpeg",
                        Some("png") => "image/png",
                        _ => "",
                    }
                    .to_string();
                }
                glb::load_uri(uri, base)?
            } else {
                return Err(invalid(format!("image {i} has neither bufferView nor uri")));
            };
            Ok(Image { name, mime, bytes })
        })
        .collect()
}
