//! The arrangement — THE timeline: one track per object, the track
//! sidebar doubling as the outliner (there is no separate scene list:
//! every object gets a track, and that *is* the list). Object clips say
//! when a thing exists and carry its motion; comp tracks play placed
//! comps; the song rides an audio row with its waveform. Pure layout and
//! hit-testing; clicks live in input, evaluation in `keys`/`comps`.
//!
//! A clip bar is tinted its object's own colour and shows a faint tick
//! at every loop seam, so "how many times does this play" is visible.
//! Dragging the body moves a clip along its own track (an object can't
//! change owner); either edge trims — the left edge eating content, the
//! Ableton way. A comp clip whose file can't be read stays on the
//! arrangement in red saying so.
//!
//! Rows run in **stack order**: the first object drawn is the top row,
//! a new one lands at the bottom (Alva, 2026-08-31: "new tracks get
//! added to the bottom of the list not the top"), and lower in the
//! list draws in front — the DAW's track order and the picture's draw
//! order are the same list. **Drag a row's head up or down** to reorder
//! it; a gold line says where it will land. A folder header drags its
//! whole run; a row dropped inside a folder's run lands after it (a
//! folder's members are its own — join one with Ctrl+Shift+N).

mod draw;
mod input;
#[cfg(test)]
mod tests;

pub use draw::rects;

use std::collections::HashMap;

use spark_render::Viewport;
use spark_ui::ICON_IMAGE;

use crate::comps::PlacedComp;
use crate::editor::Editor;
use crate::timeline::{Panel, TimeView};

/// Track label / clip label size, logical px.
pub const TRACK_TEXT: f32 = 20.0;

/// Row pitch and height, logical px.
pub const ROW_STEP: f32 = 60.0;
const ROW_H: f32 = 52.0;
/// How close to a clip's edge (logical px) a press becomes a trim.
const EDGE: f32 = 12.0;

/// What a track row stands for.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RowKind {
    /// A folder header: a group track. Collapsing hides member rows.
    Folder(u32),
    /// An object, by stack index.
    Object(usize),
    /// A comp-clip track, by its track number.
    CompTrack(u32),
    /// The song.
    Audio,
}

/// A clip on the arrangement, addressed stably.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum ClipRef {
    /// An object's clip: (object id, clip index within its sorted list).
    Obj { obj: u32, c: usize },
    /// A comp clip, by editor index.
    Comp(usize),
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
}

/// Everything the arrangement draws this frame.
pub struct ArrangeScene {
    pub rows: Vec<TrackRow>,
    pub clips: Vec<ClipRow>,
    /// The audio row's band on the axis (y0, y1), for the waveform.
    pub wave_band: Option<(f32, f32)>,
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

/// A clip drag in progress: which clip, which grip, and how far into the
/// clip the cursor grabbed it (so a move doesn't jump to the cursor).
#[derive(Clone, Copy)]
pub struct ClipDrag {
    pub r: ClipRef,
    pub zone: Zone,
    pub grab_dt: f32,
}

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

/// Every row: the objects and folders in stack order, then the comp
/// tracks, then the song.
fn row_kinds(ed: &Editor, has_audio: bool) -> Vec<RowKind> {
    let mut out = object_rows(ed);
    let mut tracks: Vec<u32> = ed.comp_clips().iter().map(|c| c.track).collect();
    tracks.sort_unstable();
    tracks.dedup();
    out.extend(tracks.into_iter().map(RowKind::CompTrack));
    if has_audio {
        out.push(RowKind::Audio);
    }
    out
}

/// Content height for scroll clamping.
pub fn content_height(ed: &Editor, has_audio: bool, scale: f32) -> f32 {
    row_count(ed, has_audio).max(3) as f32 * ROW_STEP * scale
}

/// How many rows the sidebar lists.
pub fn row_count(ed: &Editor, has_audio: bool) -> usize {
    row_kinds(ed, has_audio).len()
}

/// The slot a dragged row would drop into for a cursor at `y`: the
/// seam between rows nearest the cursor, counted among the object and
/// folder rows only (comp tracks and the song stay put at the bottom).
pub fn drop_slot(panel: &Panel, scale: f32, scroll: f32, y: f32, n_top: usize) -> usize {
    let pitch = ROW_STEP * scale;
    let f = (y - (panel.lanes.y - scroll)) / pitch.max(1.0);
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

#[allow(clippy::too_many_arguments)]
pub fn build(
    panel: &Panel,
    view: &TimeView,
    scale: f32,
    ed: &Editor,
    subcomps: &HashMap<u32, PlacedComp>,
    selected: Option<ClipRef>,
    scroll: f32,
    audio_name: Option<&str>,
    drag: Option<RowDragView>,
) -> ArrangeScene {
    let kinds = row_kinds(ed, audio_name.is_some());
    let (ax, aw) = panel.axis;
    let mut rows = Vec::new();
    let mut clips = Vec::new();
    let mut wave_band = None;
    let line = spark_text::Text::line_height(TRACK_TEXT * scale);
    let dragged = drag.and_then(|d| kinds.iter().position(|k| *k == d.kind));
    let drop_y = drag.map(|d| panel.lanes.y - scroll + d.slot as f32 * ROW_STEP * scale);
    for (k, kind) in kinds.iter().copied().enumerate() {
        // The dragged row rides the cursor; the rest hold their slots.
        let lift = match drag {
            Some(d) if d.kind == kind => d.dy,
            _ => 0.0,
        };
        let y = panel.lanes.y - scroll + k as f32 * ROW_STEP * scale + lift;
        let cell = Viewport {
            x: panel.names_box.x + 6.0 * scale,
            y: y + 2.0 * scale,
            w: panel.names_box.w - 12.0 * scale,
            h: ROW_H * scale - 4.0 * scale,
        };
        let side = 26.0 * scale;
        let mid = |h: f32| cell.y + (cell.h - h) * 0.5;
        let mut x = cell.x + 8.0 * scale;
        // Members of a folder indent under their header.
        let indented = matches!(kind, RowKind::Object(i)
            if ed.folder_of(i) != 0);
        if indented {
            x += 22.0 * scale;
        }
        let disclose = matches!(kind, RowKind::Folder(_)).then(|| {
            let v = Viewport {
                x,
                y: mid(side),
                w: side,
                h: side,
            };
            x += side + 4.0 * scale;
            v
        });
        let (glyph, label, hidden, selected_row, dim) = match kind {
            RowKind::Object(i) => {
                let s = &ed.shapes()[i];
                let (icon, _) = crate::props::kind_parts(s.kind());
                let g = Viewport {
                    x,
                    y: mid(side),
                    w: side,
                    h: side,
                };
                x += side + 6.0 * scale;
                (
                    Some((g, icon, s.rgb())),
                    ed.display_name(i),
                    ed.is_hidden(i),
                    ed.selection().contains(&i),
                    !ed.exists_now(i),
                )
            }
            RowKind::Folder(id) => {
                let f = ed.folder(id);
                (
                    None,
                    f.map(|f| {
                        if f.name.is_empty() {
                            format!("folder ({})", ed.folder_members(id).len())
                        } else {
                            f.name.clone()
                        }
                    })
                    .unwrap_or_default(),
                    f.is_some_and(|f| f.hidden),
                    false,
                    false,
                )
            }
            RowKind::CompTrack(t) => {
                let g = Viewport {
                    x,
                    y: mid(side),
                    w: side,
                    h: side,
                };
                x += side + 6.0 * scale;
                (
                    Some((g, ICON_IMAGE, [0.8, 0.25, 0.25])),
                    format!("Comps {}", t + 1),
                    false,
                    false,
                    false,
                )
            }
            RowKind::Audio => (
                None,
                audio_name.unwrap_or("song").to_string(),
                false,
                false,
                false,
            ),
        };
        // The eye sits at the row's right end, clear of the name.
        let eye = matches!(kind, RowKind::Object(_) | RowKind::Folder(_)).then(|| Viewport {
            x: cell.x + cell.w - side - 8.0 * scale,
            y: mid(side),
            w: side,
            h: side,
        });
        let label_max_w = (cell.x + cell.w - x - (side + 16.0) * scale).max(1.0);
        rows.push(TrackRow {
            kind,
            cell,
            disclose,
            eye,
            hidden,
            glyph,
            label,
            label_pos: [x, cell.y + (cell.h - line) * 0.5],
            label_max_w,
            selected: selected_row,
            dim,
        });
        // The row's clips on the axis.
        let bar_y = y + 4.0 * scale;
        let bar_h = (ROW_H - 8.0) * scale;
        let clip_bar = |start: f32, len: f32| -> Option<Viewport> {
            let x0 = view.x_of(start, panel.axis);
            let x1 = view.x_of(start + len, panel.axis);
            if x1 < ax || x0 > ax + aw {
                return None;
            }
            Some(Viewport {
                x: x0,
                y: bar_y,
                w: (x1 - x0).max(2.0),
                h: bar_h,
            })
        };
        match kind {
            RowKind::Object(i) => {
                let obj = ed.shape_id(i);
                for (c, clip) in ed.obj_clips(i).iter().enumerate() {
                    let Some(bar) = clip_bar(clip.start, clip.len) else {
                        continue;
                    };
                    // A tick at every loop seam inside the bar.
                    let mut loop_xs = Vec::new();
                    if clip.loop_on {
                        let period = clip.loop_len.max(0.05);
                        let mut t = clip.start + period - clip.offset.rem_euclid(period);
                        let mut n = 0;
                        while t < clip.end() - 1e-4 && n < 512 {
                            let x = view.x_of(t, panel.axis);
                            if x > ax && x < ax + aw {
                                loop_xs.push(x);
                            }
                            t += period;
                            n += 1;
                        }
                    }
                    let r = ClipRef::Obj { obj, c };
                    let lx = (bar.x + 10.0 * scale).max(ax + 6.0 * scale);
                    clips.push(ClipRow {
                        r,
                        bar,
                        label: ed.display_name(i),
                        label_pos: [lx, bar.y + (bar.h - line) * 0.5],
                        label_max_w: (bar.x + bar.w - lx - 8.0 * scale).max(1.0),
                        selected: selected == Some(r),
                        missing: false,
                        loop_xs,
                        color: Some(ed.shapes()[i].rgb()),
                    });
                }
            }
            RowKind::CompTrack(t) => {
                for (ci, c) in ed.comp_clips().iter().enumerate() {
                    if c.track != t {
                        continue;
                    }
                    let Some(bar) = clip_bar(c.start, c.len) else {
                        continue;
                    };
                    let (name, period, missing) = match subcomps.get(&c.comp) {
                        Some(pc) if pc.missing => {
                            (format!("! missing: {}", pc.name()), pc.period, true)
                        }
                        Some(pc) => (pc.name(), pc.period, false),
                        None => ("loading...".to_string(), f32::MAX, false),
                    };
                    let mut loop_xs = Vec::new();
                    let mut k = 1;
                    while c.start + k as f32 * period < c.start + c.len && k < 512 {
                        let x = view.x_of(c.start + k as f32 * period, panel.axis);
                        if x > ax && x < ax + aw {
                            loop_xs.push(x);
                        }
                        k += 1;
                    }
                    let r = ClipRef::Comp(ci);
                    let lx = (bar.x + 10.0 * scale).max(ax + 6.0 * scale);
                    clips.push(ClipRow {
                        r,
                        bar,
                        label: name,
                        label_pos: [lx, bar.y + (bar.h - line) * 0.5],
                        label_max_w: (bar.x + bar.w - lx - 8.0 * scale).max(1.0),
                        selected: selected == Some(r),
                        missing,
                        loop_xs,
                        color: None,
                    });
                }
            }
            RowKind::Audio => {
                wave_band = Some((bar_y, bar_y + bar_h));
            }
            RowKind::Folder(_) => {}
        }
    }
    ArrangeScene {
        rows,
        clips,
        wave_band,
        dragged,
        drop_y,
    }
}

/// What a press lands on — clip bars first (later clips draw over), then
/// the sidebar's controls, then row heads.
pub fn hit(sc: &ArrangeScene, x: f32, y: f32, scale: f32) -> Option<ArrHit> {
    for c in sc.clips.iter().rev() {
        if !c.bar.contains(x, y) {
            continue;
        }
        let m = (EDGE * scale).min(c.bar.w * 0.33);
        let zone = if x < c.bar.x + m {
            Zone::Left
        } else if x > c.bar.x + c.bar.w - m {
            Zone::Right
        } else {
            Zone::Move
        };
        return Some(ArrHit::Clip(c.r, zone));
    }
    for tr in &sc.rows {
        if !tr.cell.contains(x, y) {
            continue;
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

