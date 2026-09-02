//! The .spark text format, v2: versioned header, optional `audio`, `bpm`,
//! `canvas` and `duration` lines, then one object per line as 30 floats
//! (older widths still read), each followed by its `id`, style lines
//! (`group`, `folder`, `hide`, `fx`), and its **clips** — `oclip
//! <start> <len> <offset> <loop> <looplen>` lines, each carrying `anim`
//! lines whose key times are *clip-local*. An object exists only where a
//! clip covers the playhead; its base state is the floats on its line.
//! Audio is placed too: `sclip` lines put the song (asset 0) and any
//! other `asset … sound` on the timeline, `volume` lines set a track's
//! level (see `doc/audio.rs`). Hand-rolled, diffs clean in git. Saved
//! shape files (.sparkshape) are the same format minus audio and clips.
//!
//! v1 files are read best-effort (shapes and comp clips survive; comp-time
//! keys and folder keys are dropped) — by Alva's call, pre-v2 projects are
//! disposable tests and owe the parser nothing.

mod audio;
mod types;

pub use audio::{AudioClip, SONG, SoundAsset};
pub use types::{Clip, CompAsset, Doc, EDGE, MeshAsset, ObjClip, Session};

use spark_render::{CANVAS, Shape};

use crate::anim::{Ease, Key, ShapeAnim, Target, Track};
use crate::editor::Folder;
use crate::fx::{Effect, EffectKind, Stack};

pub fn serialize(doc: &Doc) -> String {
    let Doc {
        shapes,
        ids,
        paths,
        names,
        oclips,
        fx,
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
        sounds,
        aclips,
        volumes,
        duration,
        loop_region,
        playhead,
        snap,
        wave,
        grid,
    } = doc;
    let mut out = String::from("spark-comp v2\n");
    if canvas[0] > 0.0 && canvas[1] > 0.0 {
        out.push_str(&format!("canvas {} {}\n", canvas[0], canvas[1]));
    }
    if let Some(d) = duration {
        out.push_str(&format!("duration {d}\n"));
    }
    if let Some((a, b, on)) = loop_region {
        out.push_str(&format!("loop {a} {b} {}\n", if *on { 1 } else { 0 }));
    }
    if let Some(t) = playhead {
        out.push_str(&format!("playhead {t}\n"));
    }
    if let Some(on) = snap {
        out.push_str(&format!("snap {}\n", u8::from(*on)));
    }
    if let Some(on) = wave {
        out.push_str(&format!("wave {}\n", u8::from(*on)));
    }
    if let Some(n) = grid {
        out.push_str(&format!("grid {n}\n"));
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
    audio::write(&mut out, sounds, aclips, volumes);
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
        // the name runs to end of line there. Written only when it is not
        // solid, so the common case adds nothing to the file.
        if f.opacity != 1.0 {
            out.push_str(&format!("folderfade {}\n", f.opacity));
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
        if let Some(id) = ids.get(i).filter(|id| **id != 0) {
            out.push_str(&format!("id {id}\n"));
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
        for r in fx.get(i).map(|s| s.reactions.as_slice()).unwrap_or(&[]) {
            out.push_str(&format!(
                "react {} {} {}\n",
                r.target.tag(),
                r.source.tag(),
                r.amount
            ));
        }
        for c in oclips.get(i).map(Vec::as_slice).unwrap_or(&[]) {
            out.push_str(&format!(
                "oclip {} {} {} {} {}\n",
                c.start,
                c.len,
                c.offset,
                if c.loop_on { 1 } else { 0 },
                c.loop_len
            ));
            for track in &c.anim.tracks {
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
    }
    out
}

/// Unknown lines are skipped, so older and newer files both read (a v1
/// file's comp-time `anim` lines land before any `oclip` and are dropped
/// — with its keys, by design).
pub fn parse(text: &str) -> Doc {
    let mut shapes: Vec<Shape> = Vec::new();
    let mut ids: Vec<u32> = Vec::new();
    let mut paths: Vec<Vec<[f32; 2]>> = Vec::new();
    let mut names = Vec::new();
    let mut oclips: Vec<Vec<ObjClip>> = Vec::new();
    let mut fx: Vec<Stack> = Vec::new();
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
    let mut sounds: Vec<SoundAsset> = Vec::new();
    let mut aclips: Vec<AudioClip> = Vec::new();
    let mut volumes: Vec<(u32, f32)> = Vec::new();
    let mut duration = None;
    let mut loop_region = None;
    let mut playhead = None;
    let mut snap = None;
    let mut wave = None;
    let mut grid = None;
    for line in text.lines().skip(1) {
        if let Some(p) = line.strip_prefix("audio ") {
            audio = Some(p.trim().to_string());
            continue;
        }
        if let Some(rest) = line.strip_prefix("canvas ") {
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
                (Some(Ok(id)), Some("sound"), Some(path)) => sounds.push(SoundAsset {
                    id,
                    path: path.trim().to_string(),
                }),
                _ => {}
            }
            continue;
        }
        if audio::parse_line(line, &mut aclips, &mut volumes) {
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
        if let Some(rest) = line.strip_prefix("loop ") {
            let mut tok = rest.split_whitespace().map(str::parse::<f32>);
            if let (Some(Ok(a)), Some(Ok(b)), Some(Ok(on))) = (tok.next(), tok.next(), tok.next())
                && b > a
            {
                loop_region = Some((a, b, on >= 0.5));
            }
            continue;
        }
        if let Some(rest) = line.strip_prefix("playhead ") {
            playhead = rest.trim().parse::<f32>().ok().filter(|t| *t >= 0.0);
            continue;
        }
        if let Some(rest) = line.strip_prefix("snap ") {
            snap = rest.trim().parse::<u8>().ok().map(|v| v != 0);
            continue;
        }
        if let Some(rest) = line.strip_prefix("wave ") {
            wave = rest.trim().parse::<u8>().ok().map(|v| v != 0);
            continue;
        }
        if let Some(rest) = line.strip_prefix("grid ") {
            grid = rest.trim().parse::<u32>().ok();
            continue;
        }
        if let Some(p) = line.strip_prefix("bpm ") {
            bpm = p.trim().parse().ok();
            continue;
        }
        if let Some(rest) = line.strip_prefix("folderdef ") {
            // `<id> <c|e> <h|v> <x> <y> <rot> <scale> <name...>` — the name
            // runs to end of line.
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
                }
                folders.push(f);
            }
            continue;
        }
        if let Some(rest) = line.strip_prefix("folderfade ") {
            if let (Ok(v), Some(f)) = (rest.trim().parse::<f32>(), folders.last_mut()) {
                f.opacity = v.clamp(0.0, 1.0);
            }
            continue;
        }
        if let Some(rest) = line.strip_prefix("id ") {
            if let (Ok(v), Some(last)) = (rest.trim().parse::<u32>(), ids.last_mut()) {
                *last = v;
            }
            continue;
        }
        if let Some(rest) = line.strip_prefix("oclip ") {
            // `<start> <len> <offset> <loop 0|1> <looplen>`, attached to
            // the shape above.
            let v: Vec<f32> = rest
                .split_whitespace()
                .filter_map(|t| t.parse().ok())
                .collect();
            if let (&[start, len, offset, loop_on, loop_len], Some(list)) =
                (v.as_slice(), oclips.last_mut())
                && len > 0.0
            {
                list.push(ObjClip {
                    start: start.max(0.0),
                    len,
                    offset: offset.max(0.0),
                    loop_on: loop_on >= 0.5,
                    loop_len: loop_len.max(0.05),
                    anim: ShapeAnim::default(),
                });
            }
            continue;
        }
        if let Some(rest) = line.strip_prefix("fx ") {
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
            // Attaches to the last clip of the shape above it; a stray line
            // (a v1 file's comp-time keys) is dropped.
            if let (Some(track), Some(clip)) = (
                parse_track(rest),
                oclips.last_mut().and_then(|l| l.last_mut()),
            ) {
                clip.anim.tracks.push(track);
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
        if let Some(rest) = line.strip_prefix("folder ") {
            if let (Ok(f), Some(last)) = (rest.trim().parse(), folder.last_mut()) {
                *last = f;
            }
            continue;
        }
        if let Some(rest) = line.strip_prefix("react ") {
            // `react <setting> <trigger> <intensity>`: one setting riding
            // one curve, attached to the object above. The v1 line was
            // three bare amounts (every object wobbling from birth) —
            // its first token is a number, and it is dropped on load.
            let mut tok = rest.split_whitespace();
            if let (Some(target), Some(source), Some(Ok(amount)), Some(stack)) = (
                tok.next().and_then(Target::parse),
                tok.next().and_then(crate::fx::Source::from_tag),
                tok.next().map(str::parse::<f32>),
                fx.last_mut(),
            ) {
                stack.set_reaction(crate::fx::Reaction {
                    target,
                    source,
                    amount,
                });
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
        // The line has grown four times — every past width still reads.
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
        ids.push(0);
        names.push(name.to_string());
        oclips.push(Vec::new());
        fx.push(Stack::default());
        groups.push(0);
        hidden.push(false);
        folder.push(0);
    }
    // Identity fix-up: unassigned or duplicated ids get fresh ones, so
    // every object leaves the parser uniquely named whatever the file
    // held (a .sparkshape has no id lines at all).
    let mut seen: Vec<u32> = Vec::new();
    let mut next = ids.iter().copied().max().unwrap_or(0) + 1;
    for id in &mut ids {
        if *id == 0 || seen.contains(id) {
            *id = next;
            next += 1;
        }
        seen.push(*id);
    }
    // Clips sorted by start — the invariant every lookup leans on.
    for list in &mut oclips {
        list.sort_by(|a, b| a.start.total_cmp(&b.start));
    }
    // A `folderdef` whose members all vanished would be a ghost row.
    folders.retain(|f| folder.contains(&f.id));
    audio::finish(audio.as_deref(), &mut aclips);
    Doc {
        shapes,
        ids,
        paths,
        names,
        oclips,
        fx,
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
        sounds,
        aclips,
        volumes,
        duration,
        loop_region,
        playhead,
        snap,
        wave,
        grid,
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
