//! Print what the loader makes of a glTF file: `cargo run -p spark_assets
//! --example inspect -- path/to/model.glb`.

use std::path::Path;
use std::time::Instant;

fn main() {
    let Some(path) = std::env::args().nth(1) else {
        eprintln!("usage: inspect <file.glb|file.gltf>");
        std::process::exit(2);
    };
    let t = Instant::now();
    let m = match spark_assets::load(Path::new(&path)) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("{path}: {e}");
            std::process::exit(1);
        }
    };
    println!("{} — loaded in {:?}", m.name, t.elapsed());
    if let Some(b) = m.bounds {
        let s = b.size();
        let c = b.centre();
        println!(
            "bounds {:.3} × {:.3} × {:.3}, centre ({:.3}, {:.3}, {:.3})",
            s[0], s[1], s[2], c[0], c[1], c[2]
        );
    }
    for (i, p) in m.primitives.iter().enumerate() {
        println!(
            "primitive {i} `{}`: {} vertices, {} triangles, uvs {}, material {:?}",
            p.name,
            p.positions.len(),
            p.indices.len() / 3,
            if p.uvs.is_empty() { "no" } else { "yes" },
            p.material
        );
    }
    for (i, mat) in m.materials.iter().enumerate() {
        println!(
            "material {i} `{}`: base {:?} tex {:?}, metal {} rough {} tex {:?}, normal tex {:?}, emissive {:?} tex {:?}{}{}",
            mat.name,
            mat.base_color,
            mat.base_color_texture,
            mat.metallic,
            mat.roughness,
            mat.metallic_roughness_texture,
            mat.normal_texture,
            mat.emissive,
            mat.emissive_texture,
            if mat.double_sided { ", double-sided" } else { "" },
            if mat.unlit { ", unlit" } else { "" },
        );
    }
    for (i, im) in m.images.iter().enumerate() {
        println!("image {i} `{}`: {} {} bytes", im.name, im.mime, im.bytes.len());
    }
}
