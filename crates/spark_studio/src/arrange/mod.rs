//! The arrangement — THE timeline: one track per object, the track
//! sidebar doubling as the outliner (there is no separate scene list:
//! every object gets a track, and that *is* the list). Object clips say
//! when a thing exists and carry its motion; comp tracks play placed
//! comps; **audio rides tracks too** — the song and every other sound
//! the comp names, each a row of clips drawing its waveform, with a
//! volume box in the row's head. Pure layout and hit-testing; clicks
//! live in `input`, drags in `group`, evaluation in `keys`/`comps`.
//!
//! A clip bar is tinted its object's own colour (audio is teal) and
//! shows a faint tick at every loop seam, so "how many times does this
//! play" is visible. Dragging the body moves a clip along its own track
//! (an object can't change owner) — and every other selected clip with
//! it, so Ctrl+A and a drag shoves the whole arrangement over to make
//! room for an intro; either edge trims — the left edge eating content,
//! the Ableton way. A clip whose file can't be read stays on the
//! arrangement in red saying so.
//!
//! Rows run in **stack order**: the audio rows first (the song on top:
//! it can't be reordered, so it sits where it is always in view), then
//! the first object drawn is the top row and a new one lands at the
//! bottom (Alva, 2026-08-31: "new tracks get added to the bottom of the
//! list not the top"), and lower in the list draws in front — the
//! DAW's track order and the picture's draw order are the same list.
//! **Drag a row's head up or down** to reorder it; a gold line says
//! where it will land. A folder header drags its whole run; a row
//! dropped inside a folder's run lands after it.

mod build;
mod draw;
mod group;
mod input;
#[cfg(test)]
mod tests;
mod waves;

pub use build::build;
pub use draw::rects;
pub use waves::clip_waves;

use spark_render::Viewport;

use crate::editor::Editor;
use crate::timeline::Panel;

/// Track label / clip label size, logical px.
pub const TRACK_TEXT: f32 = 20.0;

/// Row pitch and height, logical px — and the audio rows' taller pair:
/// the name on top and the volume box under it (Alva, 2026-09-02:
/// "they'd have to be taller to fit more options").
pub const ROW_STEP: f32 = 60.0;
pub(super) const ROW_H: f32 = 52.0;
pub const AUDIO_ROW_STEP: f32 = 104.0;
/// The volume box: a well in the audio row's head, dragged up and down.
pub(super) const VOL_W: f32 = 150.0;
pub(super) const VOL_H: f32 = 38.0;
/// How far in from a clip's edge (logical px) a press is a trim — the
/// grip, drawn on every bar so it can be aimed at (Alva, 2026-09-02:
/// "it takes me like 5-10 tries") — and how far *past* the edge the
/// same grip still answers, so an overshoot lands.
pub const GRIP: f32 = 26.0;
const GRIP_OUT: f32 = 10.0;

/// What a track row stands for.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RowKind {
    /// A folder header: a group track. Collapsing hides member rows.
    Folder(u32),
    /// An object, by stack index.
    Object(usize),
    /// A comp-clip track, by its track number.
    CompTrack(u32),
    /// An audio track, by asset — the song is `doc::SONG`.
    Audio(u32),
}

/// A clip on the arrangement, addressed stably.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum ClipRef {
    /// An object's clip: (object id, clip index within its sorted list).
    Obj { obj: u32, c: usize },
    /// A comp clip, by editor index.
    Comp(usize),
    /// An audio clip, by its index in the editor's list.
    Audio(usize),
}

/// An audio track as the arrangement lists it — the song first, then
/// each sound the comp names — with its clips resolved to spans (the
/// studio knows the files' lengths; the arrangement doesn't).
pub struct AudioTrack {
    pub asset: u32,
    pub name: String,
    /// The file couldn't be read: its clips draw red.
    pub missing: bool,
    /// What the volume box reads.
    pub volume: String,
    pub clips: Vec<AudioBar>,
}

/// One audio clip, resolved: its index in the editor's list, and its
/// place and length on the timeline.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AudioBar {
    pub k: usize,
    pub start: f32,
    pub span: f32,
}

pub struct TrackRow {
    pub kind: RowKind,
    pub cell: Viewport,
    /// The disclose triangle (folders only).
    pub disclose: Option<Viewport>,
    /// The visibility eye (objects and folders).
    pub eye: Option<Viewport>,
    /// Whether the eye shows hidden.
    pub hidden: bool,
    /// The kind glyph, and the colour it tints (an object's own).
    pub glyph: Option<(Viewport, f32, [f32; 3])>,
    pub label: String,
    pub label_pos: [f32; 2],
    pub label_max_w: f32,
    pub selected: bool,
    /// No clip under the playhead: listed, but not there right now.
    pub dim: bool,
    /// The volume box and its reading (audio rows).
    pub volume: Option<(Viewport, String)>,
}

pub struct ClipRow {
    pub r: ClipRef,
    pub bar: Viewport,
    pub label: String,
    pub label_pos: [f32; 2],
    pub label_max_w: f32,
    pub selected: bool,
    pub missing: bool,
    /// x of every loop seam inside the bar.
    pub loop_xs: Vec<f32>,
    /// The bar's tint (the object's colour; comps stay red).
    pub color: Option<[f32; 3]>,
    /// An audio clip: which file's waveform fills the bar.
    pub audio: Option<u32>,
}

/// Everything the arrangement draws this frame.
pub struct ArrangeScene {
    pub rows: Vec<TrackRow>,
    pub clips: Vec<ClipRow>,
    /// The row being dragged, by index into `rows`, drawn over the rest.
    pub dragged: Option<usize>,
    /// Where a dragged row will land: the y of the gold line.
    pub drop_y: Option<f32>,
}

/// Which part of a clip a press grabs.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Zone {
    Move,
    Left,
    Right,
}

/// A clip drag in progress: which clip was grabbed and by which grip,
/// how far into it the cursor grabbed (so a move doesn't jump to the
/// cursor), where the press was and whether the cursor has travelled
/// since — a press that never does is a click, and a click in a clip
/// is a seek. `group` is every selected clip with where it started,
/// the grabbed one included: a move carries them all by the same
/// amount (`orig` is the grabbed clip's own start).
#[derive(Clone)]
pub struct ClipDrag {
    pub r: ClipRef,
    pub zone: Zone,
    pub grab_dt: f32,
    pub press_x: f32,
    pub moved: bool,
    pub group: Vec<(ClipRef, f32)>,
    pub orig: f32,
}

/// Cursor travel before a press on a clip becomes a drag, logical px.
pub const CLIP_DRAG_START: f32 = 4.0;

/// A row being dragged up or down the sidebar: which, how far the
/// cursor has travelled from the press, and whether it has travelled
/// enough to count — a press that never does is a click.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RowDrag {
    pub kind: RowKind,
    pub from_y: f32,
    pub dy: f32,
    pub moved: bool,
}

/// A row drag as the frame draws it: the row's kind and offset, and
/// the slot the gold line marks.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RowDragView {
    pub kind: RowKind,
    pub dy: f32,
    pub slot: usize,
}

/// What a sidebar press hit.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum ArrHit {
    Clip(ClipRef, Zone),
    Head(RowKind),
    Eye(RowKind),
    Disclose(u32),
    /// An audio track's volume box, by asset.
    Volume(u32),
}

/// The object and folder rows, in stack order — bottom of the stack
/// first, so the first thing drawn is the top row and a newborn lands
/// at the bottom. Folder members hide under a collapsed header.
pub fn object_rows(ed: &Editor) -> Vec<RowKind> {
    let mut out = Vec::new();
    let n = ed.shapes().len();
    let mut i = 0;
    while i < n {
        let f = ed.folder_of(i);
        if f == 0 {
            out.push(RowKind::Object(i));
            i += 1;
            continue;
        }
        let members = ed.folder_members(f);
        out.push(RowKind::Folder(f));
        if !ed.folder(f).is_some_and(|fo| fo.collapsed) {
            out.extend(members.iter().map(|&m| RowKind::Object(m)));
        }
        i = members.last().copied().unwrap_or(i) + 1;
    }
    out
}

/// Every row: the audio tracks first — the song on top, then the
/// sounds; none of them reorder, so they sit where they are always in
/// view (Alva: "at least put it at the top") — then the objects and
/// folders in stack order, then the comp tracks.
pub(super) fn row_kinds(ed: &Editor, audio: &[AudioTrack]) -> Vec<RowKind> {
    let mut out: Vec<RowKind> = audio.iter().map(|a| RowKind::Audio(a.asset)).collect();
    out.extend(object_rows(ed));
    let mut tracks: Vec<u32> = ed.comp_clips().iter().map(|c| c.track).collect();
    tracks.sort_unstable();
    tracks.dedup();
    out.extend(tracks.into_iter().map(RowKind::CompTrack));
    out
}

/// A row's pitch, logical px: audio rows are the tall ones.
pub fn row_step(kind: RowKind) -> f32 {
    match kind {
        RowKind::Audio(_) => AUDIO_ROW_STEP,
        _ => ROW_STEP,
    }
}

/// How tall the rows above the object rows are — the audio tracks —
/// in physical px, so a drop slot counts from the first object.
pub fn head_px(audio: &[AudioTrack], scale: f32) -> f32 {
    audio.len() as f32 * AUDIO_ROW_STEP * scale
}

/// Content height for scroll clamping.
pub fn content_height(ed: &Editor, audio: &[AudioTrack], scale: f32) -> f32 {
    let rows = row_kinds(ed, audio);
    let h: f32 = rows.iter().map(|k| row_step(*k)).sum();
    h.max(3.0 * ROW_STEP) * scale
}

/// How many rows the sidebar lists.
pub fn row_count(ed: &Editor, audio: &[AudioTrack]) -> usize {
    row_kinds(ed, audio).len()
}

/// The slot a dragged row would drop into for a cursor at `y`: the
/// seam between rows nearest the cursor, counted among the object and
/// folder rows only — the `head` px of audio rows above them and the
/// comp tracks below stay put.
pub fn drop_slot(panel: &Panel, scale: f32, scroll: f32, y: f32, n_top: usize, head: f32) -> usize {
    let pitch = ROW_STEP * scale;
    let f = (y - (panel.lanes.y - scroll) - head) / pitch.max(1.0);
    (f.round().max(0.0) as usize).min(n_top)
}

/// The stack index a drop `slot` sits before: the object at that row,
/// a folder's first member, or the end of the stack past the last row.
pub fn drop_dest(ed: &Editor, slot: usize) -> usize {
    match object_rows(ed).get(slot) {
        Some(RowKind::Object(i)) => *i,
        Some(RowKind::Folder(f)) => ed.folder_members(*f).first().copied().unwrap_or(0),
        _ => ed.shapes().len(),
    }
}

/// The grip's width on a bar: the full [`GRIP`] once the bar has room,
/// a third of the bar when it doesn't, so a short clip keeps a body.
pub fn grip_w(bar_w: f32, scale: f32) -> f32 {
    (GRIP * scale).min(bar_w * 0.33)
}

/// What a press lands on — clip bars first (later clips draw over; a
/// press inside a bar beats one just outside another's edge), then
/// the sidebar's controls, then row heads.
pub fn hit(sc: &ArrangeScene, x: f32, y: f32, scale: f32) -> Option<ArrHit> {
    for c in sc.clips.iter().rev() {
        if !c.bar.contains(x, y) {
            continue;
        }
        let m = grip_w(c.bar.w, scale);
        let zone = if x < c.bar.x + m {
            Zone::Left
        } else if x > c.bar.x + c.bar.w - m {
            Zone::Right
        } else {
            Zone::Move
        };
        return Some(ArrHit::Clip(c.r, zone));
    }
    // Just past an edge, on the bar's row: still that edge.
    let out = GRIP_OUT * scale;
    for c in sc.clips.iter().rev() {
        if y < c.bar.y || y > c.bar.y + c.bar.h {
            continue;
        }
        if x >= c.bar.x - out && x < c.bar.x {
            return Some(ArrHit::Clip(c.r, Zone::Left));
        }
        if x > c.bar.x + c.bar.w && x <= c.bar.x + c.bar.w + out {
            return Some(ArrHit::Clip(c.r, Zone::Right));
        }
    }
    for tr in &sc.rows {
        if !tr.cell.contains(x, y) {
            continue;
        }
        if let (Some((v, _)), RowKind::Audio(asset)) = (&tr.volume, tr.kind)
            && v.contains(x, y)
        {
            return Some(ArrHit::Volume(asset));
        }
        if let Some(d) = tr.disclose
            && d.contains(x, y)
            && let RowKind::Folder(id) = tr.kind
        {
            return Some(ArrHit::Disclose(id));
        }
        if tr.eye.is_some_and(|e| e.contains(x, y)) {
            return Some(ArrHit::Eye(tr.kind));
        }
        return Some(ArrHit::Head(tr.kind));
    }
    None
}
