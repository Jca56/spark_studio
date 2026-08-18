//! The .spark text format: versioned header, optional `audio` line, one
//! shape per line as 26 floats (14 before gradients, 18 before star fields,
//! 22 before opacity — all four read), then optional `| x y x y ...` path
//! vertices
//! and an optional `# name`. `anim <prop> <t> <v> <s|l> ...`, `react`, and
//! `group <id>` lines follow their shape. Hand-rolled, diffs clean in git.
//! Saved shape files (.sparkshape) are the same format, minus audio/keys.
//! Destined for the spark_project crate when the timeline document arrives.

use spark_render::Shape;

use crate::anim::{Ease, Key, ShapeAnim, Target, Track};
use crate::editor::Folder;
use crate::fx::{Effect, EffectKind, Stack};

/// One comp's worth of document: the parallel per-shape arrays plus the
/// document-level bits. Everything that round-trips through the format.
#[derive(Default)]
pub struct Doc {
    pub shapes: Vec<Shape>,
    pub paths: Vec<Vec<[f32; 2]>>,
    pub names: Vec<String>,
    pub anims: Vec<ShapeAnim>,
    /// Effect stacks, parallel to `shapes`.
    pub fx: Vec<Stack>,
    pub reacts: Vec<[f32; 3]>,
    pub groups: Vec<u32>,
    pub hidden: Vec<bool>,
    /// Folder id per shape (0 = loose).
    pub folder: Vec<u32>,
    /// Folder definitions, in stack order.
    pub folders: Vec<Folder>,
    pub audio: Option<String>,
    /// A tempo the user typed, overriding what analysis guessed. Detection
    /// is an estimate; the person who made the track knows the number, and
    /// once they've said it the comp has to remember.
    pub bpm: Option<f32>,
}

pub fn serialize(doc: &Doc) -> String {
    let Doc {
        shapes,
        paths,
        names,
        anims,
        fx,
        reacts,
        groups,
        hidden,
        folder,
        folders,
        audio,
        bpm,
    } = doc;
    let mut out = String::from("spark-comp v1\n");
    if let Some(a) = audio {
        out.push_str(&format!("audio {a}\n"));
    }
    if let Some(b) = bpm {
        out.push_str(&format!("bpm {b}\n"));
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
        // Its own line rather than a ninth column on `folderdef`, because
        // the name runs to end of line there: a folder actually named "1"
        // would be read as an opacity and lose its name. Written only when
        // it is not solid, so the common case adds nothing to the file.
        if f.opacity != 1.0 {
            out.push_str(&format!("folderfade {}\n", f.opacity));
        }
        for track in &f.anim.tracks {
            if track.keys.is_empty() {
                continue;
            }
            out.push_str(&format!("folderanim {}", track.target.tag()));
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
        for e in fx.get(i).map(|s| s.effects.as_slice()).unwrap_or(&[]) {
            out.push_str(&format!(
                "fx {} {} {}",
                e.id,
                e.kind.tag(),
                if e.on { "on" } else { "off" }
            ));
            for v in &e.params {
                out.push_str(&format!(" {v}"));
            }
            out.push('\n');
        }
        for track in anims.get(i).map(|a| a.tracks.as_slice()).unwrap_or(&[]) {
            if track.keys.is_empty() {
                continue;
            }
            out.push_str(&format!("anim {}", track.target.tag()));
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
    let mut fx: Vec<Stack> = Vec::new();
    let mut reacts: Vec<[f32; 3]> = Vec::new();
    let mut groups: Vec<u32> = Vec::new();
    let mut hidden: Vec<bool> = Vec::new();
    let mut folder: Vec<u32> = Vec::new();
    let mut folders: Vec<Folder> = Vec::new();
    let mut audio = None;
    let mut bpm = None;
    for line in text.lines().skip(1) {
        if let Some(p) = line.strip_prefix("audio ") {
            audio = Some(p.trim().to_string());
            continue;
        }
        if let Some(p) = line.strip_prefix("bpm ") {
            bpm = p.trim().parse().ok();
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
        if let Some(rest) = line.strip_prefix("folderfade ") {
            // Attaches to the folderdef above it. Absent — which is every
            // file written before folders could fade — means solid.
            if let (Ok(v), Some(f)) = (rest.trim().parse::<f32>(), folders.last_mut()) {
                f.opacity = v.clamp(0.0, 1.0);
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
        if let Some(rest) = line.strip_prefix("fx ") {
            // `<id> <tag> <on|off> <p0> <p1> ...`, attached to the shape above.
            let mut tok = rest.split_whitespace();
            if let (Some(Ok(id)), Some(kind), Some(on), Some(stack)) = (
                tok.next().map(str::parse::<u32>),
                tok.next().and_then(EffectKind::from_tag),
                tok.next(),
                fx.last_mut(),
            ) {
                let mut e = Effect::new(id, kind);
                e.on = on == "on";
                for (i, v) in tok.filter_map(|t| t.parse::<f32>().ok()).enumerate() {
                    e.set(i, v);
                }
                stack.effects.push(e);
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
        // The line has grown three times — 14 floats before gradients, 18
        // before `extra`, 22 before opacity. Every past width still reads;
        // `from_short_array` knows which of the missing fields mean "off"
        // when they come back zero and which one (opacity) does not.
        if !matches!(vals.len(), 14 | 18 | 22 | spark_render::FIELDS) {
            continue;
        }
        let mut arr = [0.0f32; spark_render::FIELDS];
        arr[..vals.len()].copy_from_slice(&vals);
        let mut shape = Shape::from_short_array(arr, vals.len());
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
        fx.push(Stack::default());
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
        fx,
        reacts,
        groups,
        hidden,
        folder,
        folders,
        audio,
        bpm,
    }
}

/// `<prop> <t> <v> <s|l> ...` — the payload of an `anim` line.
fn parse_track(rest: &str) -> Option<Track> {
    let mut tok = rest.split_whitespace();
    let target = Target::parse(tok.next()?)?;
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
    (!keys.is_empty()).then_some(Track { target, keys })
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
            target: Target::Shape(Prop::X),
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

    /// A star field is entirely in its numbers — nobody placed those stars,
    /// so if the seed or the form doesn't survive a save the comp reopens as
    /// a different sky.
    #[test]
    fn star_fields_round_trip() {
        let mut s = Shape::stars([400.0, 300.0], [200.0, 150.0], 42.0);
        s.set_density(75.0);
        s.set_twinkle(0.8);
        s.set_twinkle_rate(5.5);
        s.set_star_form(2);
        s.set_thickness(6.0);
        let text = serialize(&Doc {
            shapes: vec![s],
            names: vec![String::new()],
            anims: vec![ShapeAnim::default()],
            reacts: vec![[1.0; 3]],
            groups: vec![0],
            hidden: vec![false],
            folder: vec![0],
            ..Default::default()
        });
        let back = parse(&text).shapes.remove(0);
        assert_eq!(back.kind(), spark_render::ShapeKind::Stars);
        assert_eq!(back.seed(), Some(42.0));
        assert_eq!(back.density(), Some(75.0));
        assert_eq!(back.twinkle(), Some(0.8));
        assert_eq!(back.twinkle_rate(), Some(5.5));
        assert_eq!(back.star_form(), Some(2));
        assert_eq!(back.thickness(), Some(6.0), "star size");
        assert_eq!(back.box_size(), Some([400.0, 300.0]), "the region");
    }

    /// A typed tempo overrides detection, so it has to survive the file —
    /// otherwise the correction is retyped every time the comp opens.
    #[test]
    fn a_typed_tempo_round_trips() {
        let text = serialize(&Doc {
            audio: Some("track.wav".into()),
            bpm: Some(140.0),
            ..Default::default()
        });
        assert_eq!(parse(&text).bpm, Some(140.0));
        // And a comp that never had one stays untouched, so detection keeps
        // getting to answer.
        let plain = serialize(&Doc {
            audio: Some("track.wav".into()),
            ..Default::default()
        });
        assert!(!plain.contains("bpm"), "wrote a tempo nobody set");
        assert_eq!(parse(&plain).bpm, None);
    }

    /// The shape line grew from 18 floats to 22 for star fields. Comps
    /// written before that still have to open, with the new tail at zero.
    #[test]
    fn eighteen_float_files_still_read() {
        // A gradient-era circle: 18 numbers, no `extra`.
        let text = "spark-comp v1\n\
                    0 0 100 200 50 50 1 0 0 1.4 30 4 0 0 0 0.5 1 1\n";
        let d = parse(text);
        assert_eq!(d.shapes.len(), 1, "an 18-float line still parses");
        assert_eq!(d.shapes[0].center(), [100.0, 200.0]);
        assert!(d.shapes[0].gradient());
        assert_eq!(d.shapes[0].seed(), None, "and it isn't a star field");
    }

    /// ...and 22 to 26 for opacity, which is the one field a zeroed tail
    /// would get *wrong*: a comp written before shapes could fade is a comp
    /// where every shape is solid, so reading the gap as zero would open
    /// every old project blank.
    #[test]
    fn files_from_before_opacity_open_solid() {
        let text = "spark-comp v1\n\
                    0 0 100 200 50 50 1 0 0 1.4 30 4 0 0 0 0.5 1 1 0 0 0 0\n";
        let d = parse(text);
        assert_eq!(d.shapes.len(), 1, "a 22-float line still parses");
        assert_eq!(d.shapes[0].opacity(), 1.0, "an old shape opened faded out");
        // The eras before it too, all the way back.
        let old = parse("spark-comp v1\n0 0 100 100 0 0 0 1 1 1 1 30 1.4 4\n");
        assert_eq!(old.shapes[0].opacity(), 1.0);
    }

    /// And a line that carries the field means what it says.
    #[test]
    fn a_saved_fade_survives_a_round_trip() {
        let mut d = parse("spark-comp v1\n0 0 100 100 0 0 1 1 1 1 0 0 0 0\n");
        d.shapes[0].set_opacity(0.3);
        let back = parse(&serialize(&d));
        assert!(
            (back.shapes[0].opacity() - 0.3).abs() < 1e-6,
            "opacity did not survive save/load"
        );
    }

    /// A folder's fade rides its own line rather than a ninth column on
    /// `folderdef`, because the name there runs to end of line: a folder
    /// actually named "1" would otherwise be read as an opacity and lose
    /// its name.
    #[test]
    fn a_folder_fade_survives_a_round_trip_and_so_does_a_numeric_name() {
        let mut d = parse(
            "spark-comp v1\n0 0 100 100 0 0 1 1 1 1 0 0 0 0\n\
                           0 0 200 200 0 0 1 1 1 1 0 0 0 0\n",
        );
        // A member each, or the loader prunes them as ghost rows.
        d.folder = vec![1, 2];
        let mut f = Folder::new(1, "1".to_string());
        f.opacity = 0.25;
        d.folders.push(f);
        d.folders.push(Folder::new(2, "solid".to_string()));
        let text = serialize(&d);
        assert!(
            !text.contains("folderfade") || text.matches("folderfade").count() == 1,
            "a solid folder wrote a fade line it didn't need"
        );
        let back = parse(&text);
        assert_eq!(back.folders[0].opacity, 0.25, "the fade did not survive");
        assert_eq!(back.folders[0].name, "1", "the name was eaten by the fade");
        assert_eq!(
            back.folders[1].opacity, 1.0,
            "a folder without one is solid"
        );
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

    /// Effects and the curves that drive them both have to survive the
    /// file. A stack that doesn't round-trip is a comp that reopens looking
    /// different; a curve whose target doesn't round-trip is an animation
    /// that silently stops.
    #[test]
    fn effects_and_their_curves_round_trip() {
        let mut stack = Stack::default();
        let glow = stack.add(EffectKind::Glow, stack.next_id());
        stack.find_mut(glow).unwrap().set(0, 85.0);
        let grad = stack.add(EffectKind::Gradient, stack.next_id());
        stack.find_mut(grad).unwrap().set(1, 0.5);
        stack.find_mut(grad).unwrap().on = false;

        let mut a = ShapeAnim::default();
        a.tracks.push(Track {
            target: Target::Effect { id: glow, param: 0 },
            keys: vec![
                Key {
                    t: 0.0,
                    v: 0.0,
                    ease: Ease::Smooth,
                },
                Key {
                    t: 4.0,
                    v: 85.0,
                    ease: Ease::Linear,
                },
            ],
        });

        let text = serialize(&Doc {
            shapes: vec![Shape::circle([10.0, 20.0], 5.0)],
            names: vec![String::new()],
            anims: vec![a.clone()],
            fx: vec![stack.clone()],
            reacts: vec![[1.0; 3]],
            groups: vec![0],
            hidden: vec![false],
            folder: vec![0],
            ..Default::default()
        });
        let d = parse(&text);
        assert_eq!(d.fx.len(), 1);
        assert_eq!(d.fx[0], stack, "the stack came back different");
        assert_eq!(d.anims[0], a, "the curve's target came back different");
        // And an effect id survives as an id, not as a position.
        assert_ne!(glow, grad);
        assert!(d.fx[0].find(glow).is_some() && d.fx[0].find(grad).is_some());
    }

    #[test]
    fn track_sampling() {
        let tr = Track {
            target: Target::Shape(Prop::Brightness),
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
