//! The document's types: assets, clips, and the parallel-array `Doc`
//! itself. The text format that round-trips them lives in the parent
//! module.

use spark_render::Shape;

use super::{AudioClip, SoundAsset};
use crate::anim::ShapeAnim;
use crate::editor::Folder;
use crate::fx::Stack;

/// An imported model the comp draws: a file, named by a small id that
/// mesh shapes carry in their `extra[0]`.
#[derive(Clone, Debug, PartialEq)]
pub struct MeshAsset {
    pub id: u32,
    pub path: String,
}

/// Another comp this one places: a .spark file, named by a small id that
/// comp clips carry. The path is stored as given — moving the file breaks
/// the reference, and the arrangement says so rather than showing nothing.
#[derive(Clone, Debug, PartialEq)]
pub struct CompAsset {
    pub id: u32,
    pub path: String,
}

/// One comp clip on the arrangement: comp asset `comp` plays on `track`
/// from `start` for `len` seconds of the host's time, looping its comp's
/// own period the whole way.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Clip {
    pub track: u32,
    pub comp: u32,
    pub start: f32,
    pub len: f32,
}

/// One span of an object's existence on the arrangement, carrying its
/// motion: keyframes in clip-local time. The object's base state lives on
/// the object; the clip says when it is there and how it moves — the
/// instrument/notes split, Ableton's own.
/// How far before a clip's start still counts as inside it: a few
/// audio frames' worth, well under a pixel of timeline.
pub const EDGE: f32 = 2.0e-4;

#[derive(Clone, Debug, PartialEq, Default)]
pub struct ObjClip {
    /// Where the clip sits in the comp's time, seconds.
    pub start: f32,
    pub len: f32,
    /// Content offset: how far into the motion the clip's left edge sits.
    /// A left-trim eats content rather than shifting it, so the surviving
    /// motion keeps its grid position.
    pub offset: f32,
    /// While on, content time wraps by `loop_len`; off, the motion plays
    /// once and holds its last pose to the clip's end (curves clamp).
    pub loop_on: bool,
    pub loop_len: f32,
    /// The motion: keyframe curves in clip-local time.
    pub anim: ShapeAnim,
}

impl ObjClip {
    /// A fresh clip: `len` long, looping its own initial length.
    pub fn new(start: f32, len: f32) -> Self {
        Self {
            start,
            len: len.max(0.05),
            offset: 0.0,
            loop_on: true,
            loop_len: len.max(0.05),
            anim: ShapeAnim::default(),
        }
    }

    /// Whether host time `t` is inside the clip. The start forgives a
    /// hair: a clock quantised to audio frames lands a few microseconds
    /// off the time it was asked for, and a hair before the start must
    /// not read as "no clip here".
    pub fn contains(&self, t: f32) -> bool {
        t >= self.start - EDGE && t < self.start + self.len
    }

    /// Clip-local content time for host time `t` — never before the
    /// content's start, so the hair before a looping clip can't wrap
    /// round to its loop's end.
    pub fn local(&self, t: f32) -> f32 {
        let lt = (t - self.start).max(0.0) + self.offset;
        if self.loop_on {
            lt.rem_euclid(self.loop_len.max(0.001))
        } else {
            lt
        }
    }

    pub fn end(&self) -> f32 {
        self.start + self.len
    }
}

/// Where work left off, handed back by a load for the studio to apply.
#[derive(Default)]
pub struct Session {
    pub loop_region: Option<(f32, f32, bool)>,
    pub playhead: Option<f32>,
    /// The snap toggle, the waveform overlay, and the grid (steps to
    /// the bar) — the timeline's modes, kept so a reopened project
    /// works the way it was left (Alva, 2026-09-01).
    pub snap: Option<bool>,
    pub wave: Option<bool>,
    pub grid: Option<u32>,
}

/// One comp's worth of document: the parallel per-object arrays plus the
/// document-level bits. Everything that round-trips through the format.
#[derive(Default)]
pub struct Doc {
    pub shapes: Vec<Shape>,
    /// Persistent object identity, parallel to `shapes` — what clips and
    /// anything else that outlives a session refer to. 0 = unassigned
    /// (an imported .sparkshape); `parse` fills those in.
    pub ids: Vec<u32>,
    pub paths: Vec<Vec<[f32; 2]>>,
    pub names: Vec<String>,
    /// Each object's clips, parallel to `shapes`, sorted by start.
    pub oclips: Vec<Vec<ObjClip>>,
    /// Effect stacks, parallel to `shapes`. Audio reaction lives here
    /// too, as a React effect — the per-object `react` line is gone.
    pub fx: Vec<Stack>,
    pub groups: Vec<u32>,
    pub hidden: Vec<bool>,
    /// Folder id per shape (0 = loose).
    pub folder: Vec<u32>,
    /// Folder definitions, in stack order. Static parent transforms —
    /// group automation went with v1 and returns properly later.
    pub folders: Vec<Folder>,
    pub audio: Option<String>,
    /// A tempo the user typed, overriding what analysis guessed.
    pub bpm: Option<f32>,
    /// The models mesh shapes draw, as `asset <id> mesh <path>` lines.
    pub assets: Vec<MeshAsset>,
    /// The comp's size, as a `canvas <w> <h>` line.
    pub canvas: [f32; 2],
    /// The comps this one places, as `asset <id> comp <path>` lines.
    pub comps: Vec<CompAsset>,
    /// Comp clips, one `clip <track> <comp> <start> <len>` line each.
    pub clips: Vec<Clip>,
    /// The other audio files this comp plays, as `asset <id> sound
    /// <path>` lines (the song is `audio`, asset 0 — see `doc/audio.rs`).
    pub sounds: Vec<SoundAsset>,
    /// Every audio clip on the arrangement, the song's included, one
    /// `sclip <asset> <start> <len> <offset>` line each.
    pub aclips: Vec<AudioClip>,
    /// Per-track volume, `volume <asset> <gain>`; absent is unity.
    pub volumes: Vec<(u32, f32)>,
    /// An explicit length in seconds (`duration <s>`) — the loop period
    /// when this comp is placed as a clip. `None` derives it from the
    /// last clip's end.
    pub duration: Option<f32>,
    /// Session state riding the file: none of it is part of what the
    /// comp *is* (dirty tracking ignores it).
    pub loop_region: Option<(f32, f32, bool)>,
    pub playhead: Option<f32>,
    /// The timeline's modes, riding the file like the loop and the
    /// playhead — see `Session`.
    pub snap: Option<bool>,
    pub wave: Option<bool>,
    pub grid: Option<u32>,
}

