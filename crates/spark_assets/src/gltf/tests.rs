//! Loader tests on hand-built documents — a triangle in a few dozen
//! bytes says more than a real file can — plus a smoke test on Alva's
//! logo when it is where it usually is.

use std::path::Path;

use super::*;
use crate::glb;

fn le(v: &[f32]) -> Vec<u8> {
    v.iter().flat_map(|f| f.to_le_bytes()).collect()
}

/// A model from a document and its BIN chunk.
fn model(json: &str, bin: &[u8]) -> Result<Model, Error> {
    let c = glb::from_bytes(&glb::assemble(json, bin), Path::new(".")).unwrap();
    from_container(&c, "test", Path::new("."))
}

/// One CCW triangle in the glTF frame, normal toward the viewer (+z):
/// positions at 0..36, u16 indices at 36..42, normals at 44..80.
fn triangle_bin() -> Vec<u8> {
    let mut b = le(&[0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0]);
    b.extend_from_slice(&[0, 0, 1, 0, 2, 0, 0, 0]);
    b.extend(le(&[0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0]));
    b
}

const TRI_VIEWS: &str = r#"
    "bufferViews": [
        {"buffer": 0, "byteOffset": 0, "byteLength": 36},
        {"buffer": 0, "byteOffset": 36, "byteLength": 6},
        {"buffer": 0, "byteOffset": 44, "byteLength": 36}
    ],
    "accessors": [
        {"bufferView": 0, "componentType": 5126, "count": 3, "type": "VEC3"},
        {"bufferView": 1, "componentType": 5123, "count": 3, "type": "SCALAR"},
        {"bufferView": 2, "componentType": 5126, "count": 3, "type": "VEC3"}
    ],
    "buffers": [{"byteLength": 80}]"#;

/// A whole document around the triangle: `nodes` and `attrs` are spliced
/// in verbatim.
fn tri_doc(nodes: &str, attrs: &str) -> String {
    format!(
        r#"{{"asset": {{"version": "2.0"}}, "scene": 0, "scenes": [{{"nodes": [0]}}],
        "nodes": {nodes},
        "meshes": [{{"name": "tri", "primitives": [{{"attributes": {{{attrs}}}, "indices": 1}}]}}],
        {TRI_VIEWS}}}"#
    )
}

fn close(a: [f32; 3], b: [f32; 3]) -> bool {
    (0..3).all(|i| (a[i] - b[i]).abs() < 1e-4)
}

#[test]
fn a_triangle_comes_through_in_sparks_frame() {
    let m = model(&tri_doc(r#"[{"mesh": 0}]"#, r#""POSITION": 0"#), &triangle_bin()).unwrap();
    assert_eq!(m.primitives.len(), 1);
    let p = &m.primitives[0];
    assert_eq!(p.name, "tri");
    assert_eq!(p.indices, vec![0, 1, 2]);
    // y flips: glTF's up is Spark's down. z stays: toward the viewer both ways.
    assert!(close(p.positions[1], [1.0, 0.0, 0.0]));
    assert!(close(p.positions[2], [0.0, -1.0, 0.0]));
    // No normals in the file: flat ones, facing the camera (+z).
    assert_eq!(p.normals.len(), 3);
    assert!(close(p.normals[0], [0.0, 0.0, 1.0]), "{:?}", p.normals[0]);
    assert!(p.uvs.is_empty());
    let b = m.bounds.unwrap();
    assert!(close(b.min, [0.0, -1.0, 0.0]) && close(b.max, [1.0, 0.0, 0.0]));
    assert!(close(b.size(), [1.0, 1.0, 0.0]));
}

#[test]
fn stored_normals_turn_with_the_frame() {
    let m = model(
        &tri_doc(r#"[{"mesh": 0}]"#, r#""POSITION": 0, "NORMAL": 2"#),
        &triangle_bin(),
    )
    .unwrap();
    assert!(close(m.primitives[0].normals[1], [0.0, 0.0, 1.0]));
}

#[test]
fn node_transforms_bake_in() {
    let m = model(
        &tri_doc(
            r#"[{"mesh": 0, "translation": [10, 0, 0], "scale": [2, 2, 2]}]"#,
            r#""POSITION": 0"#,
        ),
        &triangle_bin(),
    )
    .unwrap();
    let p = &m.primitives[0];
    assert!(close(p.positions[1], [12.0, 0.0, 0.0]));
    assert!(close(p.positions[2], [10.0, -2.0, 0.0]));
}

#[test]
fn a_quaternion_rotation_applies() {
    let s = std::f32::consts::FRAC_1_SQRT_2;
    // A quarter turn about glTF's z: x-hat -> y-hat.
    let m = model(
        &tri_doc(
            &format!(r#"[{{"mesh": 0, "rotation": [0, 0, {s}, {s}]}}]"#),
            r#""POSITION": 0, "NORMAL": 2"#,
        ),
        &triangle_bin(),
    )
    .unwrap();
    let p = &m.primitives[0];
    assert!(close(p.positions[1], [0.0, -1.0, 0.0]), "{:?}", p.positions[1]);
    assert!(close(p.normals[1], [0.0, 0.0, 1.0]));
}

#[test]
fn a_matrix_node_is_column_major() {
    // Translation lives in the last column: elements 12, 13, 14.
    let m = model(
        &tri_doc(
            r#"[{"mesh": 0, "matrix": [1,0,0,0, 0,1,0,0, 0,0,1,0, 5,6,7,1]}]"#,
            r#""POSITION": 0"#,
        ),
        &triangle_bin(),
    )
    .unwrap();
    assert!(close(m.primitives[0].positions[0], [5.0, -6.0, 7.0]));
}

#[test]
fn children_inherit_their_parents_transform() {
    let m = model(
        &tri_doc(
            r#"[{"translation": [0, 0, 5], "children": [1]}, {"mesh": 0, "translation": [1, 0, 0]}]"#,
            r#""POSITION": 0"#,
        ),
        &triangle_bin(),
    )
    .unwrap();
    assert!(close(m.primitives[0].positions[0], [1.0, 0.0, 5.0]));
}

#[test]
fn a_document_without_scenes_draws_its_parentless_nodes() {
    let doc = format!(
        r#"{{"nodes": [{{"children": [1]}}, {{"mesh": 0}}],
        "meshes": [{{"primitives": [{{"attributes": {{"POSITION": 0}}}}]}}], {TRI_VIEWS}}}"#
    );
    let m = model(&doc, &triangle_bin()).unwrap();
    // Node 1 is reached through node 0 exactly once.
    assert_eq!(m.primitives.len(), 1);
    assert_eq!(m.primitives[0].indices, vec![0, 1, 2]);
}

/// A unit quad — (0,0) (1,0) (0,1) (1,1) — drawn as `mode` through the
/// u8 index list `order`.
fn quad_doc(mode: u32, order: [u8; 4]) -> (String, Vec<u8>) {
    let mut bin = le(&[0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 1.0, 1.0, 0.0]);
    bin.extend_from_slice(&order);
    let doc = format!(
        r#"{{"nodes": [{{"mesh": 0}}],
        "meshes": [{{"primitives": [{{"attributes": {{"POSITION": 0}}, "indices": 1, "mode": {mode}}}]}}],
        "bufferViews": [{{"buffer": 0, "byteLength": 48}}, {{"buffer": 0, "byteOffset": 48, "byteLength": 4}}],
        "accessors": [
            {{"bufferView": 0, "componentType": 5126, "count": 4, "type": "VEC3"}},
            {{"bufferView": 1, "componentType": 5121, "count": 4, "type": "SCALAR"}}
        ],
        "buffers": [{{"byteLength": 52}}]}}"#
    );
    (doc, bin)
}

#[test]
fn strips_and_fans_become_triangles() {
    // A strip zigzags across the quad; the flat-normal pass unwelds it,
    // so the two faces' normals agreeing proves the odd triangle was
    // flipped back to the strip's winding.
    let (doc, bin) = quad_doc(5, [0, 1, 2, 3]);
    let m = model(&doc, &bin).unwrap();
    let p = &m.primitives[0];
    assert_eq!(p.indices.len(), 6);
    assert!(close(p.normals[0], p.normals[3]), "strip winding flipped a face");
    assert!(close(p.normals[0], [0.0, 0.0, 1.0]));
    // A fan walks the perimeter.
    let (doc, bin) = quad_doc(6, [0, 1, 3, 2]);
    let m = model(&doc, &bin).unwrap();
    assert_eq!(m.primitives[0].indices.len(), 6);
    assert!(close(m.primitives[0].normals[0], m.primitives[0].normals[3]));
    // Lines are not a surface: skipped, not an error.
    let (doc, bin) = quad_doc(1, [0, 1, 2, 3]);
    assert!(model(&doc, &bin).unwrap().primitives.is_empty());
}

#[test]
fn normalised_bytes_scale_to_unit() {
    let mut bin = triangle_bin();
    bin.extend_from_slice(&[255, 0, 128, 255, 0, 0]);
    let doc = r#"{"nodes": [{"mesh": 0}],
        "meshes": [{"primitives": [{"attributes": {"POSITION": 0, "TEXCOORD_0": 3}, "indices": 1}]}],
        "bufferViews": [
            {"buffer": 0, "byteOffset": 0, "byteLength": 36},
            {"buffer": 0, "byteOffset": 36, "byteLength": 6},
            {"buffer": 0, "byteOffset": 44, "byteLength": 36},
            {"buffer": 0, "byteOffset": 80, "byteLength": 6}
        ],
        "accessors": [
            {"bufferView": 0, "componentType": 5126, "count": 3, "type": "VEC3"},
            {"bufferView": 1, "componentType": 5123, "count": 3, "type": "SCALAR"},
            {"bufferView": 2, "componentType": 5126, "count": 3, "type": "VEC3"},
            {"bufferView": 3, "componentType": 5121, "count": 3, "type": "VEC2", "normalized": true}
        ],
        "buffers": [{"byteLength": 86}]}"#;
    let m = model(doc, &bin).unwrap();
    let uv = &m.primitives[0].uvs;
    assert_eq!(uv.len(), 3);
    assert!((uv[0][0] - 1.0).abs() < 1e-6 && uv[0][1] == 0.0);
    assert!((uv[1][0] - 128.0 / 255.0).abs() < 1e-6 && (uv[1][1] - 1.0).abs() < 1e-6);
}

#[test]
fn materials_and_textures_resolve() {
    let mut bin = triangle_bin();
    bin.extend_from_slice(b"JPEGBYTES");
    let doc = r#"{"nodes": [{"mesh": 0}],
        "meshes": [{"primitives": [{"attributes": {"POSITION": 0}, "indices": 1, "material": 1}]}],
        "materials": [
            {"name": "plain"},
            {"name": "fancy", "doubleSided": true,
              "pbrMetallicRoughness": {"baseColorFactor": [0.5, 0.25, 1, 1], "metallicFactor": 0, "roughnessFactor": 0.3,
                                       "baseColorTexture": {"index": 0}},
              "normalTexture": {"index": 1},
              "emissiveFactor": [1, 0.5, 0],
              "extensions": {"KHR_materials_emissive_strength": {"emissiveStrength": 4}, "KHR_materials_unlit": {}}}
        ],
        "textures": [{"source": 1}, {"source": 0}],
        "images": [{"name": "n", "mimeType": "image/png", "bufferView": 3}, {"name": "c", "mimeType": "image/jpeg", "bufferView": 3}],
        "bufferViews": [
            {"buffer": 0, "byteOffset": 0, "byteLength": 36},
            {"buffer": 0, "byteOffset": 36, "byteLength": 6},
            {"buffer": 0, "byteOffset": 44, "byteLength": 36},
            {"buffer": 0, "byteOffset": 80, "byteLength": 9}
        ],
        "accessors": [
            {"bufferView": 0, "componentType": 5126, "count": 3, "type": "VEC3"},
            {"bufferView": 1, "componentType": 5123, "count": 3, "type": "SCALAR"}
        ],
        "buffers": [{"byteLength": 89}]}"#;
    let m = model(doc, &bin).unwrap();
    assert_eq!(m.primitives[0].material, Some(1));
    assert_eq!(m.materials[0].base_color, [1.0; 4]);
    assert!(!m.materials[0].double_sided && !m.materials[0].unlit);
    let f = &m.materials[1];
    assert_eq!(f.base_color, [0.5, 0.25, 1.0, 1.0]);
    assert_eq!((f.metallic, f.roughness), (0.0, 0.3));
    // Texture 0 reads image 1, texture 1 reads image 0.
    assert_eq!(f.base_color_texture, Some(1));
    assert_eq!(f.normal_texture, Some(0));
    assert_eq!(f.emissive, [4.0, 2.0, 0.0]);
    assert!(f.double_sided && f.unlit);
    assert_eq!(m.images.len(), 2);
    assert_eq!(m.images[1].mime, "image/jpeg");
    assert_eq!(m.images[1].bytes, b"JPEGBYTES");
}

#[test]
fn broken_files_say_what_is_wrong() {
    let no_pos = tri_doc(r#"[{"mesh": 0}]"#, r#""NORMAL": 2"#);
    assert!(matches!(model(&no_pos, &triangle_bin()), Err(Error::Invalid(_))));
    // An index past the vertices.
    let mut bin = triangle_bin();
    bin[38] = 9;
    assert!(matches!(
        model(&tri_doc(r#"[{"mesh": 0}]"#, r#""POSITION": 0"#), &bin),
        Err(Error::Invalid(_))
    ));
    // A sparse accessor is honestly refused rather than half-read.
    let doc = tri_doc(r#"[{"mesh": 0}]"#, r#""POSITION": 0"#)
        .replace(r#"{"bufferView": 0, "componentType": 5126, "count": 3, "type": "VEC3"}"#,
                 r#"{"bufferView": 0, "componentType": 5126, "count": 3, "type": "VEC3", "sparse": {}}"#);
    assert!(matches!(model(&doc, &triangle_bin()), Err(Error::Unsupported(_))));
    // A cycle in the node tree.
    let doc = tri_doc(r#"[{"mesh": 0, "children": [0]}]"#, r#""POSITION": 0"#);
    assert!(matches!(model(&doc, &triangle_bin()), Err(Error::Invalid(_))));
}

#[test]
fn an_external_bin_loads_beside_the_gltf() {
    let dir = std::env::temp_dir().join(format!("spark_assets_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("tri data.bin"), triangle_bin()).unwrap();
    let doc = tri_doc(r#"[{"mesh": 0}]"#, r#""POSITION": 0"#)
        .replace(r#""buffers": [{"byteLength": 80}]"#, r#""buffers": [{"byteLength": 80, "uri": "tri%20data.bin"}]"#);
    let path = dir.join("tri.gltf");
    std::fs::write(&path, doc).unwrap();
    let m = load(&path).unwrap();
    assert_eq!(m.name, "tri");
    assert_eq!(m.primitives[0].indices, vec![0, 1, 2]);
    let _ = std::fs::remove_dir_all(dir);
}

const LOGO: &str = "/home/alva/alva_logo_3d.glb";

/// The real thing, when it's there: Alva's logo, 580k vertices of Meshy
/// output with three embedded JPEG textures.
#[test]
fn the_logo_loads() {
    let path = Path::new(LOGO);
    if !path.exists() {
        eprintln!("no logo at {LOGO} — skipping");
        return;
    }
    let t = std::time::Instant::now();
    let m = load(path).unwrap();
    eprintln!("logo loaded in {:?}", t.elapsed());
    assert_eq!(m.name, "alva_logo_3d");
    assert_eq!(m.primitives.len(), 1);
    let p = &m.primitives[0];
    assert_eq!(p.positions.len(), 579_640);
    assert_eq!(p.normals.len(), p.positions.len());
    assert_eq!(p.uvs.len(), p.positions.len());
    assert_eq!(p.indices.len(), 3_286_782);
    assert!(p.indices.iter().all(|&i| (i as usize) < p.positions.len()));
    for n in p.normals.iter().step_by(1000) {
        let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
        assert!((len - 1.0).abs() < 1e-3, "normal {n:?}");
    }
    let b = m.bounds.unwrap();
    let s = b.size();
    assert!((s[0] - 1.901).abs() < 0.01 && (s[1] - 0.963).abs() < 0.01 && (s[2] - 0.107).abs() < 0.01, "{s:?}");
    assert_eq!(m.materials.len(), 1);
    assert!(m.materials[0].double_sided);
    assert_eq!(m.materials[0].base_color_texture, Some(0));
    assert_eq!(m.materials[0].normal_texture, Some(2));
    assert_eq!(m.images.len(), 3);
    assert!(m.images.iter().all(|i| i.mime == "image/jpeg" && i.bytes.starts_with(&[0xFF, 0xD8])));
}
