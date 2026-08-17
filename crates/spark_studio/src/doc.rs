//! The .spark text format: versioned header, optional `audio` line, one
//! shape per line as 18 floats (14 before gradients — both read), then
//! optional `| x y x y ...` path vertices
//! and an optional `# name`. `anim <prop> <t> <v> <s|l> ...`, `react`, and
//! `group <id>` lines follow their shape. Hand-rolled, diffs clean in git.
//! Saved shape files (.sparkshape) are the same format, minus audio/keys.
//! Destined for the spark_project crate when the timeline document arrives.

use spark_render::Shape;

use crate::anim::{self, Ease, Key, ShapeAnim, Track};

#[allow(clippy::too_many_arguments)]
pub fn serialize(
    shapes: &[Shape],
    paths: &[Vec<[f32; 2]>],
    names: &[String],
    anims: &[ShapeAnim],
    reacts: &[[f32; 3]],
    groups: &[u32],
    hidden: &[bool],
    audio: Option<&str>,
) -> String {
    let mut out = String::from("spark-comp v1\n");
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
        if let Some(r) = reacts.get(i).filter(|r| **r != [1.0; 3]) {
            out.push_str(&format!("react {} {} {}\n", r[0], r[1], r[2]));
        }
        if let Some(g) = groups.get(i).filter(|g| **g != 0) {
            out.push_str(&format!("group {g}\n"));
        }
        if hidden.get(i).copied().unwrap_or(false) {
            out.push_str("hide\n");
        }
        for track in anims.get(i).map(|a| a.tracks.as_slice()).unwrap_or(&[]) {
            if track.keys.is_empty() {
                continue;
            }
            out.push_str(&format!("anim {}", anim::prop_tag(track.prop)));
            for k in &track.keys {
                let e = if k.ease == Ease::Linear { "l" } else { "s" };
                out.push_str(&format!(" {} {} {e}", k.t, k.v));
            }
            out.push('\n');
        }
    }
    out
}

/// Unknown lines are skipped, so older and newer files both read.
#[allow(clippy::type_complexity)]
pub fn parse(
    text: &str,
) -> (
    Vec<Shape>,
    Vec<Vec<[f32; 2]>>,
    Vec<String>,
    Vec<ShapeAnim>,
    Vec<[f32; 3]>,
    Vec<u32>,
    Vec<bool>,
    Option<String>,
) {
    let mut shapes = Vec::new();
    let mut paths: Vec<Vec<[f32; 2]>> = Vec::new();
    let mut names = Vec::new();
    let mut anims: Vec<ShapeAnim> = Vec::new();
    let mut reacts: Vec<[f32; 3]> = Vec::new();
    let mut groups: Vec<u32> = Vec::new();
    let mut hidden: Vec<bool> = Vec::new();
    let mut audio = None;
    for line in text.lines().skip(1) {
        if let Some(p) = line.strip_prefix("audio ") {
            audio = Some(p.trim().to_string());
            continue;
        }
        if let Some(rest) = line.strip_prefix("anim ") {
            // Attaches to the shape above it; a stray line is dropped.
            if let (Some(track), Some(a)) = (parse_track(rest), anims.last_mut()) {
                a.tracks.push(track);
            }
            continue;
        }
        if line.trim() == "hide" {
            if let Some(last) = hidden.last_mut() {
                *last = true;
            }
            continue;
        }
        if let Some(rest) = line.strip_prefix("group ") {
            if let (Ok(g), Some(last)) = (rest.trim().parse(), groups.last_mut()) {
                *last = g;
            }
            continue;
        }
        if let Some(rest) = line.strip_prefix("react ") {
            let vals: Vec<f32> = rest
                .split_whitespace()
                .filter_map(|t| t.parse().ok())
                .collect();
            if let (&[a, b, c], Some(r)) = (vals.as_slice(), reacts.last_mut()) {
                *r = [a, b, c];
            }
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
        if vals.len() != 14 && vals.len() != 18 {
            continue;
        }
        let mut arr = [0.0f32; 18];
        arr[..vals.len()].copy_from_slice(&vals);
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
        anims.push(ShapeAnim::default());
        reacts.push([1.0; 3]);
        groups.push(0);
        hidden.push(false);
    }
    (shapes, paths, names, anims, reacts, groups, hidden, audio)
}

/// `<prop> <t> <v> <s|l> ...` — the payload of an `anim` line.
fn parse_track(rest: &str) -> Option<Track> {
    let mut tok = rest.split_whitespace();
    let prop = anim::parse_prop(tok.next()?)?;
    let mut keys = Vec::new();
    while let Some(t) = tok.next() {
        let (Some(v), Some(e)) = (tok.next(), tok.next()) else {
            break;
        };
        let (Ok(t), Ok(v)) = (t.parse(), v.parse()) else {
            break;
        };
        keys.push(Key {
            t,
            v,
            ease: if e == "l" { Ease::Linear } else { Ease::Smooth },
        });
    }
    keys.sort_by(|a, b| a.t.total_cmp(&b.t));
    (!keys.is_empty()).then_some(Track { prop, keys })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::props::Prop;

    #[test]
    fn anim_round_trip() {
        let mut shape = Shape::circle([100.0, 200.0], 50.0);
        shape.set_gradient(true);
        shape.set_rgb2([0.1, 0.2, 0.3]);
        let shapes = vec![shape];
        let mut a = ShapeAnim::default();
        a.tracks.push(Track {
            prop: Prop::X,
            keys: vec![
                Key {
                    t: 1.0,
                    v: 100.0,
                    ease: Ease::Smooth,
                },
                Key {
                    t: 3.0,
                    v: 500.0,
                    ease: Ease::Linear,
                },
            ],
        });
        let text = serialize(
            &shapes,
            &[],
            &[String::new()],
            &[a.clone()],
            &[[1.0, 0.5, 2.0]],
            &[3],
            &[true],
            Some("x.mp3"),
        );
        let (s2, _, _, a2, r2, g2, h2, audio) = parse(&text);
        assert_eq!(s2.len(), 1);
        assert_eq!(audio.as_deref(), Some("x.mp3"));
        assert_eq!(a2[0], a);
        assert_eq!(r2[0], [1.0, 0.5, 2.0]);
        assert_eq!(g2[0], 3);
        assert!(s2[0].gradient());
        assert_eq!(s2[0].rgb2(), [0.1, 0.2, 0.3]);
        assert!(h2[0]);
    }

    #[test]
    fn track_sampling() {
        let tr = Track {
            prop: Prop::Glow,
            keys: vec![
                Key {
                    t: 1.0,
                    v: 10.0,
                    ease: Ease::Smooth,
                },
                Key {
                    t: 3.0,
                    v: 20.0,
                    ease: Ease::Smooth,
                },
            ],
        };
        // Clamped outside, exact midpoint in the middle (smoothstep(0.5)=0.5).
        assert_eq!(tr.sample(0.0), Some(10.0));
        assert_eq!(tr.sample(9.0), Some(20.0));
        assert_eq!(tr.sample(2.0), Some(15.0));
        // Smooth eases: quarter-way in time is less than quarter-way in value.
        assert!(tr.sample(1.5).unwrap() < 12.5);
    }
}
