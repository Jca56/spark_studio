//! The arrangement's layout for one frame: every row's cell and
//! controls down the sidebar, every clip's bar on the axis. Pure —
//! the studio hands it the editor, the audio tracks resolved, the
//! selection and the drag, and draws what comes back.

use std::collections::HashMap;

use spark_render::Viewport;
use spark_ui::{ICON_IMAGE, theme};

use super::{
    ArrangeScene, AudioTrack, ClipRef, ClipRow, ROW_H, ROW_STEP, RowDragView, RowKind, TRACK_TEXT,
    TrackRow, VOL_H, VOL_W, head_px, row_kinds, row_step,
};
use crate::comps::PlacedComp;
use crate::editor::Editor;
use crate::timeline::{Panel, TimeView};

#[allow(clippy::too_many_arguments)]
pub fn build(
    panel: &Panel,
    view: &TimeView,
    scale: f32,
    ed: &Editor,
    subcomps: &HashMap<u32, PlacedComp>,
    selected: &[ClipRef],
    scroll: f32,
    audio: &[AudioTrack],
    missing_meshes: &[u32],
    drag: Option<RowDragView>,
) -> ArrangeScene {
    let kinds = row_kinds(ed, audio);
    let (ax, aw) = panel.axis;
    let mut rows = Vec::new();
    let mut clips = Vec::new();
    let line = spark_text::Text::line_height(TRACK_TEXT * scale);
    let dragged = drag.and_then(|d| kinds.iter().position(|k| *k == d.kind));
    let head = head_px(audio, scale);
    let drop_y = drag.map(|d| panel.lanes.y - scroll + head + d.slot as f32 * ROW_STEP * scale);
    let teal = theme().wave;
    let teal = [teal[0], teal[1], teal[2]];
    let mut y = panel.lanes.y - scroll;
    for kind in kinds.iter().copied() {
        let step = row_step(kind) * scale;
        let row_h = step - (ROW_STEP - ROW_H) * scale;
        // The dragged row rides the cursor; the rest hold their slots.
        let lift = match drag {
            Some(d) if d.kind == kind => d.dy,
            _ => 0.0,
        };
        let top = y + lift;
        y += step;
        let cell = Viewport {
            x: panel.names_box.x + 6.0 * scale,
            y: top + 2.0 * scale,
            w: panel.names_box.w - 12.0 * scale,
            h: row_h - 4.0 * scale,
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
        let audio_track = match kind {
            RowKind::Audio(asset) => audio.iter().find(|a| a.asset == asset),
            _ => None,
        };
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
                // A mesh whose file couldn't be read says so in its name.
                let name = if mesh_lost(ed, i, missing_meshes) {
                    format!("! {}", ed.display_name(i))
                } else {
                    ed.display_name(i)
                };
                (
                    Some((g, icon, s.rgb())),
                    name,
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
            RowKind::Audio(_) => {
                let a = audio_track;
                let name = a.map(|a| a.name.clone()).unwrap_or_else(|| "sound".into());
                (
                    None,
                    if a.is_some_and(|a| a.missing) {
                        format!("! {name}")
                    } else {
                        name
                    },
                    false,
                    false,
                    false,
                )
            }
        };
        // The eye sits at the row's right end, clear of the name.
        let eye = matches!(kind, RowKind::Object(_) | RowKind::Folder(_)).then(|| Viewport {
            x: cell.x + cell.w - side - 8.0 * scale,
            y: mid(side),
            w: side,
            h: side,
        });
        let label_max_w = (cell.x + cell.w - x - (side + 16.0) * scale).max(1.0);
        // An audio row: the name along the top, the volume box under it.
        let (label_y, volume) = match audio_track {
            Some(a) => (
                cell.y + 8.0 * scale,
                Some((
                    Viewport {
                        x: cell.x + 10.0 * scale,
                        y: cell.y + cell.h - VOL_H * scale - 8.0 * scale,
                        w: (VOL_W * scale).min(cell.w - 20.0 * scale),
                        h: VOL_H * scale,
                    },
                    a.volume.clone(),
                )),
            ),
            None => (cell.y + (cell.h - line) * 0.5, None),
        };
        rows.push(TrackRow {
            kind,
            cell,
            disclose,
            eye,
            hidden,
            glyph,
            label,
            label_pos: [x, label_y],
            label_max_w,
            selected: selected_row,
            dim,
            volume,
        });
        // The row's clips on the axis.
        let bar_y = top + 4.0 * scale;
        let bar_h = (row_h - 8.0 * scale).max(1.0);
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
        let label_at = |bar: &Viewport| {
            let lx = (bar.x + 10.0 * scale).max(ax + 6.0 * scale);
            ([lx, bar.y + (bar.h - line) * 0.5], (bar.x + bar.w - lx - 8.0 * scale).max(1.0))
        };
        match kind {
            RowKind::Object(i) => {
                let obj = ed.shape_id(i);
                let lost = mesh_lost(ed, i, missing_meshes);
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
                    let (label_pos, label_max_w) = label_at(&bar);
                    clips.push(ClipRow {
                        r,
                        bar,
                        label: if lost {
                            format!("! missing: {}", ed.display_name(i))
                        } else {
                            ed.display_name(i)
                        },
                        label_pos,
                        label_max_w,
                        selected: selected.contains(&r),
                        missing: lost,
                        loop_xs,
                        color: Some(ed.shapes()[i].rgb()),
                        audio: None,
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
                    let (label_pos, label_max_w) = label_at(&bar);
                    clips.push(ClipRow {
                        r,
                        bar,
                        label: name,
                        label_pos,
                        label_max_w,
                        selected: selected.contains(&r),
                        missing,
                        loop_xs,
                        color: None,
                        audio: None,
                    });
                }
            }
            RowKind::Audio(asset) => {
                let Some(a) = audio_track else {
                    continue;
                };
                for ab in &a.clips {
                    let Some(bar) = clip_bar(ab.start, ab.span) else {
                        continue;
                    };
                    let r = ClipRef::Audio(ab.k);
                    let (label_pos, label_max_w) = label_at(&bar);
                    clips.push(ClipRow {
                        r,
                        bar,
                        label: if a.missing {
                            format!("! missing: {}", a.name)
                        } else {
                            a.name.clone()
                        },
                        label_pos,
                        label_max_w,
                        selected: selected.contains(&r),
                        missing: a.missing,
                        loop_xs: Vec::new(),
                        color: Some(teal),
                        audio: Some(asset),
                    });
                }
            }
            RowKind::Folder(_) => {}
        }
    }
    ArrangeScene {
        rows,
        clips,
        dragged,
        drop_y,
    }
}

/// Whether object `i` is a mesh whose file couldn't be read.
fn mesh_lost(ed: &Editor, i: usize, missing: &[u32]) -> bool {
    ed.shapes()
        .get(i)
        .and_then(|s| s.mesh_asset())
        .is_some_and(|a| missing.contains(&a))
}
