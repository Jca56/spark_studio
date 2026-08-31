//! The .spark text format: versioned header, optional `audio`, `bpm` and
//! `canvas` lines, one
//! shape per line as 30 floats (14 before gradients, 18 before star fields,
//! 22 before opacity, 26 before `space` — all five read), then optional
//! `| x y x y ...` path
//! vertices
//! and an optional `# name`. `anim <prop> <t> <v> <s|l> ...`, `react`, and
//! `group <id>` lines follow their shape. Hand-rolled, diffs clean in git.
//! Saved shape files (.sparkshape) are the same format, minus audio/keys.
//! Destined for the spark_project crate when the timeline document arrives.

use spark_render::{CANVAS, Shape};

use crate::anim::{Ease, Key, ShapeAnim, Target, Track};
use crate::editor::Folder;
use crate::fx::{Effect, EffectKind, Stack};

/// An imported model the comp draws: a file, named by a small id that
/// mesh shapes carry in their `extra[0]`.
#[derive(Clone, Debug, PartialEq)]
pub struct MeshAsset {
    pub id: u32,
    pub path: String,
}

/// Another comp this one places: a .spark file, named by a small id that
/// clips carry. The path is stored as given — moving the file breaks the
/// reference, and the arrangement says so rather than showing nothing.
#[derive(Clone, Debug, PartialEq)]
pub struct CompAsset {
    pub id: u32,
    pub path: String,
}

/// One clip on the arrangement: comp asset `comp` plays on `track` from
/// `start` for `len` seconds of the host's time, looping its comp's own
/// period the whole way — that is what a clip is *for*: a two-second
/// spin placed for a minute spins the minute out.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Clip {
    pub track: u32,
    pub comp: u32,
    pub start: f32,
    pub len: f32,
}

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
    /// The models mesh shapes draw, as `asset <id> mesh <path>` lines.
    pub assets: Vec<MeshAsset>,
    /// The comp's size, as a `canvas <w> <h>` line. Files from before
    /// comps had a size read as the default; a non-positive size (a saved
    /// shape, which is not a comp) writes no line.
    pub canvas: [f32; 2],
    /// The comps this one places, as `asset <id> comp <path>` lines.
    pub comps: Vec<CompAsset>,
    /// The arrangement, one `clip <track> <comp> <start> <len>` line each.
    pub clips: Vec<Clip>,
    /// An explicit length in seconds (`duration <s>`), which is the loop
    /// period when this comp is placed as a clip. `None` — every file
    /// until today — derives it from the last keyframe instead.
    pub duration: Option<f32>,
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
        assets,
        canvas,
        comps,
        clips,
        duration,
    } = doc;
    let mut out = String::from("spark-comp v1\n");
    if canvas[0] > 0.0 && canvas[1] > 0.0 {
        out.push_str(&format!("canvas {} {}\n", canvas[0], canvas[1]));
    }
    if let Some(d) = duration {
        out.push_str(&format!("duration {d}\n"));
    }
    if let Some(a) = audio {
        out.push_str(&format!("audio {a}\n"));
    }
    if let Some(b) = bpm {
        out.push_str(&format!("bpm {b}\n"));
    }
    // The path runs to end of line, like a folder's name does.
    for a in assets {
        out.push_str(&format!("asset {} mesh {}\n", a.id, a.path));
    }
    for c in comps {
        out.push_str(&format!("asset {} comp {}\n", c.id, c.path));
    }
    for c in clips {
        out.push_str(&format!(
            "clip {} {} {} {}\n",
            c.track, c.comp, c.start, c.len
        ));
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
    let mut shapes: Vec<Shape> = Vec::new();
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
    let mut assets: Vec<MeshAsset> = Vec::new();
    let mut canvas = CANVAS;
    let mut comps: Vec<CompAsset> = Vec::new();
    let mut clips: Vec<Clip> = Vec::new();
    let mut duration = None;
    for line in text.lines().skip(1) {
        if let Some(p) = line.strip_prefix("audio ") {
            audio = Some(p.trim().to_string());
            continue;
        }
        if let Some(rest) = line.strip_prefix("canvas ") {
            // `<w> <h>`; anything that isn't a size keeps the default.
            let mut num = rest.split_whitespace().map(str::parse::<f32>);
            if let (Some(Ok(w)), Some(Ok(h))) = (num.next(), num.next())
                && w >= 2.0
                && h >= 2.0
            {
                canvas = [w, h];
            }
            continue;
        }
        if let Some(rest) = line.strip_prefix("asset ") {
            // `<id> <kind> <path...>` — an unknown kind is skipped, so a
            // newer file's image assets read as nothing rather than noise.
            let mut tok = rest.splitn(3, ' ');
            match (tok.next().map(str::parse::<u32>), tok.next(), tok.next()) {
                (Some(Ok(id)), Some("mesh"), Some(path)) => assets.push(MeshAsset {
                    id,
                    path: path.trim().to_string(),
                }),
                (Some(Ok(id)), Some("comp"), Some(path)) => comps.push(CompAsset {
                    id,
                    path: path.trim().to_string(),
                }),
                _ => {}
            }
            continue;
        }
        if let Some(rest) = line.strip_prefix("clip ") {
            let mut tok = rest.split_whitespace();
            if let (Some(Ok(track)), Some(Ok(comp)), Some(Ok(start)), Some(Ok(len))) = (
                tok.next().map(str::parse::<u32>),
                tok.next().map(str::parse::<u32>),
                tok.next().map(str::parse::<f32>),
                tok.next().map(str::parse::<f32>),
            ) && len > 0.0
            {
                clips.push(Clip {
                    track,
                    comp,
                    start,
                    len,
                });
            }
            continue;
        }
        if let Some(rest) = line.strip_prefix("duration ") {
            duration = rest.trim().parse::<f32>().ok().filter(|d| *d > 0.0);
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
            // Additive was an effect for one day. It is the shape's own
            // field again, so a comp that stacked one keeps its pure light
            // rather than quietly going back to occluding — the effect
            // carried the truth while the shape's field sat dead.
            if let Some(tag) = rest.split_whitespace().nth(1)
                && tag == "add"
            {
                let on = rest.split_whitespace().nth(2) == Some("on");
                let lit = rest
                    .split_whitespace()
                    .nth(3)
                    .and_then(|t| t.parse::<f32>().ok())
                    .unwrap_or(1.0);
                if let Some(sh) = shapes.last_mut() {
                    sh.set_additive(on && lit >= 0.5);
                }
                continue;
            }
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
        // The line has grown four times — 14 floats before gradients, 18
        // before `extra`, 22 before opacity, 26 before `space`. Every past
        // width still reads; `from_short_array` knows which of the missing
        // fields mean "off" when they come back zero and which one
        // (opacity) does not.
        if !matches!(vals.len(), 14 | 18 | 22 | 26 | spark_render::FIELDS) {
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
        assets,
        canvas,
        comps,
        clips,
        duration,
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
mod tests;
