//! The .spark text format: versioned header, optional `audio` line, one
//! shape per line as 14 floats, then optional `| x y x y ...` path vertices
//! and an optional `# name`. Hand-rolled, diffs clean in git. Destined for
//! the spark_project crate when the timeline document arrives.

use spark_render::Shape;

pub fn serialize(
    shapes: &[Shape],
    paths: &[Vec<[f32; 2]>],
    names: &[String],
    audio: Option<&str>,
) -> String {
    let mut out = String::from("spark-comp v0\n");
    if let Some(a) = audio {
        out.push_str(&format!("audio {a}\n"));
    }
    for (i, shape) in shapes.iter().enumerate() {
        let vals: Vec<String> = shape.to_array().iter().map(|f| format!("{f}")).collect();
        out.push_str(&vals.join(" "));
        if let Some((id, _, _)) = shape.path_meta() {
            out.push_str(" |");
            for v in paths.get(id).map(Vec::as_slice).unwrap_or(&[]) {
                out.push_str(&format!(" {} {}", v[0], v[1]));
            }
        }
        if let Some(name) = names.get(i).filter(|n| !n.is_empty()) {
            out.push_str(&format!(" # {name}"));
        }
        out.push('\n');
    }
    out
}

/// Unknown lines are skipped, so older and newer files both read.
#[allow(clippy::type_complexity)]
pub fn parse(text: &str) -> (Vec<Shape>, Vec<Vec<[f32; 2]>>, Vec<String>, Option<String>) {
    let mut shapes = Vec::new();
    let mut paths: Vec<Vec<[f32; 2]>> = Vec::new();
    let mut names = Vec::new();
    let mut audio = None;
    for line in text.lines().skip(1) {
        if let Some(p) = line.strip_prefix("audio ") {
            audio = Some(p.trim().to_string());
            continue;
        }
        let (rest, name) = match line.split_once('#') {
            Some((a, b)) => (a, b.trim()),
            None => (line, ""),
        };
        let (nums, vert_str) = match rest.split_once('|') {
            Some((a, b)) => (a, Some(b)),
            None => (rest, None),
        };
        let vals: Vec<f32> = nums
            .split_whitespace()
            .filter_map(|t| t.parse().ok())
            .collect();
        if vals.len() != 14 {
            continue;
        }
        let mut arr = [0.0f32; 14];
        arr.copy_from_slice(&vals);
        let mut shape = Shape::from_array(arr);
        if shape.is_path() {
            let flat: Vec<f32> = vert_str
                .unwrap_or("")
                .split_whitespace()
                .filter_map(|t| t.parse().ok())
                .collect();
            let verts: Vec<[f32; 2]> = flat.chunks_exact(2).map(|c| [c[0], c[1]]).collect();
            if verts.len() < 2 {
                continue;
            }
            let bound = verts
                .iter()
                .map(|v| (v[0] * v[0] + v[1] * v[1]).sqrt())
                .fold(1.0f32, f32::max);
            let closed = shape.path_meta().is_some_and(|(_, _, c)| c);
            shape.set_path_start(paths.len());
            shape.set_path_shape(verts.len(), closed, bound);
            paths.push(verts);
        }
        shapes.push(shape);
        names.push(name.to_string());
    }
    (shapes, paths, names, audio)
}
