//! The .spark text format: versioned header, optional `audio` line, one
//! shape per line as 18 floats (14 before gradients — both read), then
//! optional `| x y x y ...` path vertices
//! and an optional `# name`. `anim <prop> <t> <v> <s|l> ...`, `react`, and
//! `group <id>` lines follow their shape. Hand-rolled, diffs clean in git.
//! Saved shape files (.sparkshape) are the same format, minus audio/keys.
//! Destined for the spark_project crate when the timeline document arrives.

use spark_render::Shape;

use crate::anim::{self, Ease, Key, ShapeAnim, Track};
use crate::editor::Folder;

/// One comp's worth of document: the parallel per-shape arrays plus the
/// document-level bits. Everything that round-trips through the format.
#[derive(Default)]
pub struct Doc {
    pub shapes: Vec<Shape>,
    pub paths: Vec<Vec<[f32; 2]>>,
    pub names: Vec<String>,
    pub anims: Vec<ShapeAnim>,
    pub reacts: Vec<[f32; 3]>,
    pub groups: Vec<u32>,
    pub hidden: Vec<bool>,
    /// Folder id per shape (0 = loose).
    pub folder: Vec<u32>,
    /// Folder definitions, in stack order.
    pub folders: Vec<Folder>,
    pub audio: Option<String>,
}

pub fn serialize(doc: &Doc) -> String {
    let Doc {
        shapes,
        paths,
        names,
        anims,
        reacts,
        groups,
        hidden,
        folder,
        folders,
        audio,
    } = doc;
    let mut out = String::from("spark-comp v1\n");
    if let Some(a) = audio {
        out.push_str(&format!("audio {a}\n"));
    }
    // Folder definitions lead, so the per-shape `folder` lines below always
    // resolve against something already known.
    for f in folders {
        out.push_str(&format!(
            "folderdef {} {} {} {} {} {} {} {}\n",
            f.id,
            if f.collapsed { "c" } else { "e" },
            if f.hidden { "h" } else { "v" },
            f.x,
            f.y,
            f.rotation,
            f.scale,
            f.name
        ));
        for track in &f.anim.tracks {
            if track.keys.is_empty() {
                continue;
            }
            out.push_str(&format!("folderanim {}", anim::prop_tag(track.prop)));
            for k in &track.keys {
                let e = if k.ease == Ease::Linear { "l" } else { "s" };
                out.push_str(&format!(" {} {} {e}", k.t, k.v));
            }
            out.push('\n');
        }
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
        if let Some(f) = folder.get(i).filter(|f| **f != 0) {
            out.push_str(&format!("folder {f}\n"));
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
pub fn parse(text: &str) -> Doc {
    let mut shapes = Vec::new();
    let mut paths: Vec<Vec<[f32; 2]>> = Vec::new();
    let mut names = Vec::new();
    let mut anims: Vec<ShapeAnim> = Vec::new();
    let mut reacts: Vec<[f32; 3]> = Vec::new();
    let mut groups: Vec<u32> = Vec::new();
    let mut hidden: Vec<bool> = Vec::new();
    let mut folder: Vec<u32> = Vec::new();
    let mut folders: Vec<Folder> = Vec::new();
    let mut audio = None;
    for line in text.lines().skip(1) {
        if let Some(p) = line.strip_prefix("audio ") {
            audio = Some(p.trim().to_string());
            continue;
        }
        if let Some(rest) = line.strip_prefix("folderdef ") {
            // `<id> <c|e> <h|v> <x> <y> <rot> <scale> <name...>` — the name
            // runs to end of line. Pre-transform files stop after <h|v>, and
            // their folders just come back at identity.
            let mut tok = rest.splitn(8, ' ');
            if let (Some(Ok(id)), Some(c), Some(h)) =
                (tok.next().map(str::parse::<u32>), tok.next(), tok.next())
            {
                let mut f = Folder::new(id, String::new());
                f.collapsed = c == "c";
                f.hidden = h == "h";
                let mut num = || tok.next().and_then(|t| t.parse::<f32>().ok());
                if let (Some(x), Some(y), Some(r), Some(sc)) = (num(), num(), num(), num()) {
                    f.x = x;
                    f.y = y;
                    f.rotation = r;
                    f.scale = sc;
                    f.name = tok.next().unwrap_or("").trim().to_string();
                } else {
                    // Old layout: whatever followed <h|v> was the name.
                    f.name = rest.splitn(4, ' ').nth(3).unwrap_or("").trim().to_string();
                }
                folders.push(f);
            }
            continue;
        }
        if let Some(rest) = line.strip_prefix("folderanim ") {
            // Attaches to the folderdef above it.
            if let (Some(track), Some(f)) = (parse_track(rest), folders.last_mut()) {
                f.anim.tracks.push(track);
            }
            continue;
        }
        if let Some(rest) = line.strip_prefix("folder ") {
            if let (Ok(f), Some(last)) = (rest.trim().parse(), folder.last_mut()) {
                *last = f;
            }
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
        folder.push(0);
    }
    // A `folderdef` whose members all vanished would be a ghost row.
    folders.retain(|f| folder.contains(&f.id));
    Doc {
        shapes,
        paths,
        names,
        anims,
        reacts,
        groups,
        hidden,
        folder,
        folders,
        audio,
    }
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
    use crate::editor::Folder;
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
        let text = serialize(&Doc {
            shapes,
            names: vec![String::new()],
            anims: vec![a.clone()],
            reacts: vec![[1.0, 0.5, 2.0]],
            groups: vec![3],
            hidden: vec![true],
            folder: vec![7],
            folders: vec![{
                let mut f = Folder::new(7, "Drop stuff".into());
                f.collapsed = true;
                f.hidden = true;
                f.x = 120.0;
                f.rotation = 0.5;
                f.scale = 1.75;
                f
            }],
            audio: Some("x.mp3".into()),
            ..Default::default()
        });
        let d = parse(&text);
        assert_eq!(d.shapes.len(), 1);
        assert_eq!(d.audio.as_deref(), Some("x.mp3"));
        assert_eq!(d.anims[0], a);
        assert_eq!(d.reacts[0], [1.0, 0.5, 2.0]);
        assert_eq!(d.groups[0], 3);
        assert!(d.shapes[0].gradient());
        assert_eq!(d.shapes[0].rgb2(), [0.1, 0.2, 0.3]);
        assert!(d.hidden[0]);
        // Folders round-trip whole: membership, name with a space, flags.
        assert_eq!(d.folder[0], 7);
        assert_eq!(d.folders.len(), 1);
        assert_eq!(d.folders[0].name, "Drop stuff");
        assert!(d.folders[0].collapsed && d.folders[0].hidden);
        // The transform rides along with the rest.
        assert_eq!(d.folders[0].x, 120.0);
        assert_eq!(d.folders[0].rotation, 0.5);
        assert_eq!(d.folders[0].scale, 1.75);
    }

    #[test]
    fn folders_from_before_the_transform_read_as_identity() {
        // A folderdef written before folders had a transform: three fields,
        // then the name. It must not swallow the name as coordinates.
        let text = "spark-comp v1\nfolderdef 2 c h Old Name Here\n\
                    0 0 100 100 0 0 0 1 1 1 1 30 1.4 4\nfolder 2\n";
        let d = parse(text);
        assert_eq!(d.folders.len(), 1);
        let f = &d.folders[0];
        assert_eq!(f.name, "Old Name Here");
        assert!(f.collapsed && f.hidden);
        assert!(f.is_identity(), "no transform means identity, not garbage");
    }

    #[test]
    fn old_files_without_folders_still_read() {
        // A v1 file predating folders: every shape loose, no folderdefs.
        let text = "spark-comp v1\n0 0 100 100 0 0 0 1 1 1 1 30 1.4 4\n";
        let d = parse(text);
        assert_eq!(d.shapes.len(), 1);
        assert_eq!(d.folder, vec![0]);
        assert!(d.folders.is_empty());
    }

    #[test]
    fn ghost_folderdefs_are_dropped() {
        // A folderdef whose members are all gone would draw an empty row.
        let text = "spark-comp v1\nfolderdef 4 e v Orphan\n\
                    0 0 100 100 0 0 0 1 1 1 1 30 1.4 4\n";
        let d = parse(text);
        assert!(d.folders.is_empty());
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
