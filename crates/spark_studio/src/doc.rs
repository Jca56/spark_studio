//! The .spark text format: versioned header, optional `audio` line, one
//! shape per line as 14 floats. Hand-rolled, diffs clean in git. Destined
//! for the spark_project crate when the timeline document arrives.

use spark_render::Shape;

pub fn serialize(shapes: &[Shape], audio: Option<&str>) -> String {
    let mut out = String::from("spark-comp v0\n");
    if let Some(a) = audio {
        out.push_str(&format!("audio {a}\n"));
    }
    for shape in shapes {
        let vals: Vec<String> = shape.to_array().iter().map(|f| format!("{f}")).collect();
        out.push_str(&vals.join(" "));
        out.push('\n');
    }
    out
}

/// Unknown lines are skipped, so older and newer files both read.
pub fn parse(text: &str) -> (Vec<Shape>, Option<String>) {
    let mut shapes = Vec::new();
    let mut audio = None;
    for line in text.lines().skip(1) {
        if let Some(p) = line.strip_prefix("audio ") {
            audio = Some(p.trim().to_string());
            continue;
        }
        let vals: Vec<f32> = line
            .split_whitespace()
            .filter_map(|t| t.parse().ok())
            .collect();
        if vals.len() == 14 {
            let mut arr = [0.0f32; 14];
            arr.copy_from_slice(&vals);
            shapes.push(Shape::from_array(arr));
        }
    }
    (shapes, audio)
}
