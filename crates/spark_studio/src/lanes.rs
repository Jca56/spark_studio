//! Keyframe lanes (the Keys tab): one row per keyed-or-selected shape in
//! stack order, key markers mapped through the timeline's zoomable view.
//! Diamonds are smooth keys, squares are linear. Pure layout — clicks live
//! in input.

use spark_render::Viewport;
use spark_ui::{ICON_KEY, UiRect, theme};

use crate::anim::{Ease, ShapeAnim};
use crate::editor::Prop;
use crate::layers::kind_parts;
use crate::timeline::{Panel, TimeView};

/// Lane label font size in logical px (a step under the body text so the
/// 42px rows breathe).
pub const LANE_TEXT: f32 = 20.0;

/// Row pitch in logical px (row height + gap).
const ROW_STEP: f32 = 48.0;
const ROW_H: f32 = 42.0;
/// Key marker side in logical px.
const KEY_SIDE: f32 = 30.0;

pub struct LaneRow {
    /// Index into the editor's shape list.
    pub index: usize,
    pub row: Viewport,
    /// The name card inside the sidebar's name box.
    pub cell: Viewport,
    pub chip: Viewport,
    /// Top-left of the label text (physical px), and the width it may fill
    /// (the gutter left of the time axis).
    pub label_pos: [f32; 2],
    pub label_max_w: f32,
    pub label: String,
    pub rgb: [f32; 3],
    pub selected: bool,
    /// Key markers: (time s, center x px, linear?).
    pub keys: Vec<(f32, f32, bool)>,
}

/// Whether a shape earns a lane: it has keys, or it's selected (so the row
/// is there to watch the first stamp land). Everything else stays out of
/// the way.
pub fn visible(anims: &[ShapeAnim], selection: &[usize], index: usize) -> bool {
    selection.contains(&index) || anims.get(index).is_some_and(|a| a.has_keys())
}

#[allow(clippy::too_many_arguments)]
pub fn rows(
    panel: &Panel,
    view: &TimeView,
    scale: f32,
    shapes: &[spark_render::Shape],
    names: &[String],
    anims: &[ShapeAnim],
    selection: &[usize],
    scroll: f32,
) -> Vec<LaneRow> {
    let area = panel.lanes;
    let pad = 12.0 * scale;
    let mut y = area.y - scroll;
    let mut out = Vec::new();
    for (index, shape) in shapes
        .iter()
        .enumerate()
        .rev()
        .filter(|(i, _)| visible(anims, selection, *i))
    {
        // Rows span from the name box across the whole axis.
        let row = Viewport {
            x: panel.names_box.x,
            y,
            w: (area.x + area.w - pad - panel.names_box.x).max(1.0),
            h: ROW_H * scale,
        };
        let cell = Viewport {
            x: panel.names_box.x + 6.0 * scale,
            y: row.y + 2.0 * scale,
            w: panel.names_box.w - 12.0 * scale,
            h: row.h - 4.0 * scale,
        };
        let chip_side = 24.0 * scale;
        let chip = Viewport {
            x: cell.x + 8.0 * scale,
            y: row.y + (row.h - chip_side) * 0.5,
            w: chip_side,
            h: chip_side,
        };
        let (_, kind_name) = kind_parts(shape.kind());
        let keys = anims
            .get(index)
            .map(|a| {
                a.key_times()
                    .iter()
                    .map(|&(t, e)| (t, view.x_of(t, panel.axis), e == Ease::Linear))
                    .collect()
            })
            .unwrap_or_default();
        let label_x = chip.x + chip.w + 10.0 * scale;
        out.push(LaneRow {
            index,
            row,
            cell,
            chip,
            label_pos: [label_x, row.y + (row.h - LANE_TEXT * 1.2 * scale) * 0.5],
            label_max_w: (cell.x + cell.w - label_x - 6.0 * scale).max(40.0),
            label: match names.get(index).filter(|n| !n.is_empty()) {
                Some(given) => given.clone(),
                None => format!("{kind_name} {}", index + 1),
            },
            rgb: shape.rgb(),
            selected: selection.contains(&index),
            keys,
        });
        y += ROW_STEP * scale;
    }
    out
}

/// Total lane content height at this scale, for scroll clamping.
pub fn content_height(count: usize, scale: f32) -> f32 {
    count as f32 * ROW_STEP * scale
}

/// Row furniture over the bar shading: a translucent accent tint on the
/// selected row's axis span (the alternation reads through), a hairline
/// separator under each row, and the name card + color chip in the
/// sidebar's name box.
pub fn rects(rows: &[LaneRow], panel: &Panel, scale: f32) -> Vec<UiRect> {
    let t = theme();
    let mut out = Vec::new();
    for lr in rows {
        if lr.selected {
            out.push(UiRect::region_rounded(
                lr.row,
                [t.accent_bg[0], t.accent_bg[1], t.accent_bg[2], 0.55],
                8.0 * scale,
            ));
        }
        out.push(UiRect::region(
            Viewport {
                x: panel.axis.0,
                y: lr.row.y + lr.row.h + 2.0 * scale,
                w: panel.axis.1,
                h: 1.5 * scale,
            },
            [1.0, 1.0, 1.0, 0.14],
        ));
        // Each shape reads as its own track: a card per name.
        out.push(UiRect::region_rounded(
            lr.cell,
            if lr.selected { t.accent_bg } else { t.card },
            8.0 * scale,
        ));
        out.push(UiRect::region_rounded(
            lr.chip,
            [lr.rgb[0], lr.rgb[1], lr.rgb[2], 1.0],
            lr.chip.w * 0.3,
        ));
    }
    out
}

/// Key markers (drawn over the bar shading): gold diamonds, squares for
/// linear; the selected key draws white and a step larger. Markers
/// scrolled or zoomed out of the axis are culled.
pub fn key_rects(
    rows: &[LaneRow],
    panel: &Panel,
    scale: f32,
    selected: &[(usize, f32)],
) -> Vec<UiRect> {
    let th = theme();
    let side = KEY_SIDE * scale;
    let (ax, aw) = panel.axis;
    let mut out = Vec::new();
    for lr in rows {
        for &(t, x, linear) in &lr.keys {
            if x < ax - side || x > ax + aw + side {
                continue;
            }
            let sel = crate::anim::key_list_has(selected, lr.index, t);
            let color = if sel { th.icon_hover } else { th.playhead };
            let grow = if sel { 1.25 } else { 1.0 };
            if linear {
                // Linear keys draw as squares — hold-to-lerp reads blocky.
                let s = side * 0.52 * grow;
                out.push(UiRect::region_rounded(
                    Viewport {
                        x: x - s * 0.5,
                        y: lr.row.y + (lr.row.h - s) * 0.5,
                        w: s,
                        h: s,
                    },
                    color,
                    2.0 * scale,
                ));
            } else {
                let s = side * grow;
                out.push(UiRect::icon_sized(
                    Viewport {
                        x: x - s * 0.5,
                        y: lr.row.y + (lr.row.h - s) * 0.5,
                        w: s,
                        h: s,
                    },
                    ICON_KEY,
                    0.0,
                    color,
                    0.42,
                ));
            }
        }
    }
    out
}

pub enum LaneHit {
    /// A key marker: (shape index, key time).
    Key(usize, f32),
    /// The name gutter of a row — select that shape.
    Gutter(usize),
    /// Empty lane space on the time axis — scrub.
    Scrub,
}

pub fn hit(rows: &[LaneRow], panel: &Panel, scale: f32, px: f32, py: f32) -> Option<LaneHit> {
    if !panel.lanes.contains(px, py) {
        return None;
    }
    if let Some(lr) = rows.iter().find(|lr| lr.row.contains(px, py)) {
        let grab = KEY_SIDE * 0.6 * scale;
        if let Some(&(t, _, _)) = lr
            .keys
            .iter()
            .filter(|(_, x, _)| (x - px).abs() <= grab)
            .min_by(|a, b| (a.1 - px).abs().total_cmp(&(b.1 - px).abs()))
        {
            return Some(LaneHit::Key(lr.index, t));
        }
        if px < panel.axis.0 {
            return Some(LaneHit::Gutter(lr.index));
        }
    }
    (px >= panel.axis.0).then_some(LaneHit::Scrub)
}

/// Retimed keys land on the nearest 16th note — choreography sits on the
/// grid; free micro-placement can come back later if it's ever missed.
pub fn quantize(t: f32, beat: &spark_audio::BeatGrid) -> f32 {
    let step = 60.0 / beat.bpm.max(1.0) / 4.0;
    let base = beat.first_bar;
    base + ((t - base) / step).round() * step
}

/// One React slider docked in the Keys tab's sidebar.
pub struct ReactRow {
    pub prop: Prop,
    pub label: &'static str,
    pub label_pos: [f32; 2],
    pub track: Viewport,
    pub t: f32,
    pub value: String,
}

/// The primary selection's audio-reaction amounts, at the bottom of the
/// lane-name box — audio behavior lives with the track.
pub fn react_rows(panel: &Panel, scale: f32, react: [f32; 3]) -> Vec<ReactRow> {
    let pad = 12.0 * scale;
    let x = panel.names_box.x + pad;
    let w = (panel.names_box.w - pad * 2.0).max(1.0);
    let row_h = 48.0 * scale;
    let mut y = panel.names_box.y + panel.names_box.h - row_h * 3.0 - 10.0 * scale;
    let specs = [
        (Prop::ReactScale, "React Scale", react[0]),
        (Prop::ReactGlow, "React Glow", react[1]),
        (Prop::ReactBright, "React Bright", react[2]),
    ];
    specs
        .into_iter()
        .map(|(prop, label, v)| {
            let r = ReactRow {
                prop,
                label,
                label_pos: [x, y],
                track: Viewport {
                    x,
                    y: y + 26.0 * scale,
                    w,
                    h: 8.0 * scale,
                },
                t: (v / 2.0).clamp(0.0, 1.0),
                value: format!("{v:.2}x"),
            };
            y += row_h;
            r
        })
        .collect()
}
