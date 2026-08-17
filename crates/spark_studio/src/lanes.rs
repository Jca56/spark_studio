//! Keyframe lanes (the Keys tab): one row per keyed-or-selected shape in
//! stack order, key markers mapped through the timeline's zoomable view.
//! Diamonds are smooth keys, squares are linear. Pure layout — clicks live
//! in input.

use spark_render::Viewport;
use spark_ui::{ICON_KEY, UiRect, theme};

use crate::anim::{Ease, Owner};
use crate::editor::{Editor, Prop};
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
    /// Whose curves this lane shows — a shape or a folder transform.
    pub owner: Owner,
    /// The whole lane, header plus any expanded settings.
    pub row: Viewport,
    /// The name/keys strip — key markers center on this, not the expansion.
    pub head: Viewport,
    /// The cog that expands this lane's settings (shapes only; folders have
    /// no per-shape settings to show yet).
    pub cog: Option<Viewport>,
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
    /// Folder lanes indent their members and tint gold.
    pub is_folder: bool,
    /// Cog-expanded settings — the React trio today, more later.
    pub detail: Vec<ReactRow>,
    /// Key markers: (time s, center x px, linear?).
    pub keys: Vec<(f32, f32, bool)>,
}

/// Whether an owner earns a lane: it has keys, or it's selected (so the row
/// is there to watch the first stamp land). Everything else stays out of
/// the way — but anything *keyed* always shows, which is what makes stray
/// keys findable and deletable instead of invisibly animating.
pub fn visible(ed: &Editor, o: Owner) -> bool {
    if ed.owner_anim(o).is_some_and(|a| a.has_keys()) {
        return true;
    }
    match o {
        Owner::Shape(i) => ed.selection().contains(&i),
        Owner::Folder(id) => {
            let m = ed.folder_members(id);
            !m.is_empty() && m.iter().all(|i| ed.selection().contains(i))
        }
    }
}

/// How many lanes the list will show, for scroll clamping.
pub fn count(ed: &Editor) -> usize {
    ed.key_owners().into_iter().filter(|&o| visible(ed, o)).count()
}

pub fn rows(
    panel: &Panel,
    view: &TimeView,
    scale: f32,
    ed: &Editor,
    open: Option<Owner>,
    scroll: f32,
) -> Vec<LaneRow> {
    let area = panel.lanes;
    let pad = 12.0 * scale;
    let mut y = area.y - scroll;
    let mut out = Vec::new();
    for owner in ed.key_owners().into_iter().filter(|&o| visible(ed, o)) {
        let is_folder = matches!(owner, Owner::Folder(_));
        let indent = if matches!(owner, Owner::Shape(i) if ed.folder_of(i) != 0) {
            14.0 * scale
        } else {
            0.0
        };
        // Rows span from the name box across the whole axis.
        let head = Viewport {
            x: panel.names_box.x,
            y,
            w: (area.x + area.w - pad - panel.names_box.x).max(1.0),
            h: ROW_H * scale,
        };
        let cell = Viewport {
            x: panel.names_box.x + 6.0 * scale + indent,
            y: head.y + 2.0 * scale,
            w: panel.names_box.w - 12.0 * scale - indent,
            h: head.h - 4.0 * scale,
        };
        let chip_side = 24.0 * scale;
        let chip = Viewport {
            x: cell.x + 8.0 * scale,
            y: head.y + (head.h - chip_side) * 0.5,
            w: chip_side,
            h: chip_side,
        };
        // Shapes carry React amounts; a folder transform has none yet.
        let cog_side = 26.0 * scale;
        let cog = matches!(owner, Owner::Shape(_)).then_some(Viewport {
            x: cell.x + cell.w - cog_side - 6.0 * scale,
            y: head.y + (head.h - cog_side) * 0.5,
            w: cog_side,
            h: cog_side,
        });
        let expanded = open == Some(owner) && cog.is_some();
        let detail = if expanded {
            match owner {
                Owner::Shape(i) => {
                    react_rows(cell.x, cell.x + cell.w, head.y + head.h, scale, ed.react(i))
                }
                Owner::Folder(_) => Vec::new(),
            }
        } else {
            Vec::new()
        };
        let extra = if detail.is_empty() {
            0.0
        } else {
            REACT_H * 3.0 * scale + 8.0 * scale
        };
        let row = Viewport {
            h: head.h + extra,
            ..head
        };
        let keys: Vec<_> = ed
            .owner_anim(owner)
            .map(|a| {
                a.key_times()
                    .iter()
                    .map(|&(t, e)| (t, view.x_of(t, panel.axis), e == Ease::Linear))
                    .collect()
            })
            .unwrap_or_default();
        let (label, rgb, selected) = match owner {
            Owner::Shape(i) => {
                let shape = &ed.shapes()[i];
                let (_, kind_name) = kind_parts(shape.kind());
                let name = ed.name(i);
                (
                    if name.is_empty() {
                        format!("{kind_name} {}", i + 1)
                    } else {
                        name.to_string()
                    },
                    shape.rgb(),
                    ed.selection().contains(&i),
                )
            }
            Owner::Folder(id) => {
                let f = ed.folder(id);
                (
                    f.map(|f| f.name.clone()).unwrap_or_default(),
                    // Folder lanes wear the accent, not a shape color.
                    [1.0, 0.78, 0.09],
                    visible(ed, owner) && ed.folder_members(id).iter().all(|i| ed.selection().contains(i)),
                )
            }
        };
        let label_x = chip.x + chip.w + 10.0 * scale;
        out.push(LaneRow {
            owner,
            row,
            head,
            cog,
            cell,
            chip,
            label_pos: [label_x, head.y + (head.h - LANE_TEXT * 1.2 * scale) * 0.5],
            label_max_w: (cell.x + cell.w - label_x - 6.0 * scale).max(40.0),
            label,
            rgb,
            selected,
            is_folder,
            detail,
            keys,
        });
        y += row.h + (ROW_STEP - ROW_H) * scale;
    }
    out
}

/// Total lane content height at this scale, for scroll clamping — the
/// expanded lane, if any, is taller than the rest.
pub fn content_height(ed: &Editor, open: Option<Owner>, scale: f32) -> f32 {
    let n = count(ed);
    let expanded = open
        .filter(|&o| matches!(o, Owner::Shape(_)) && visible(ed, o))
        .is_some();
    let extra = if expanded {
        REACT_H * 3.0 * scale + 8.0 * scale
    } else {
        0.0
    };
    n as f32 * ROW_STEP * scale + extra
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
                lr.head,
                [t.accent_bg[0], t.accent_bg[1], t.accent_bg[2], 0.55],
                8.0 * scale,
            ));
        }
        out.push(UiRect::region(
            Viewport {
                x: panel.axis.0,
                y: lr.head.y + lr.head.h + 2.0 * scale,
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
        if let Some(cog) = lr.cog {
            out.push(UiRect::icon_sized(
                cog,
                spark_ui::ICON_GEAR,
                0.0,
                if lr.detail.is_empty() {
                    t.icon
                } else {
                    t.playhead
                },
                0.40,
            ));
        }
        out.push(UiRect::region_rounded(
            lr.chip,
            [lr.rgb[0], lr.rgb[1], lr.rgb[2], 1.0],
            // Folder chips are square-ish so they read as containers.
            if lr.is_folder {
                lr.chip.w * 0.18
            } else {
                lr.chip.w * 0.3
            },
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
    selected: &[(Owner, f32)],
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
            let sel = crate::anim::key_list_has(selected, lr.owner, t);
            let color = if sel { th.icon_hover } else { th.playhead };
            let grow = if sel { 1.25 } else { 1.0 };
            if linear {
                // Linear keys draw as squares — hold-to-lerp reads blocky.
                let s = side * 0.52 * grow;
                out.push(UiRect::region_rounded(
                    Viewport {
                        x: x - s * 0.5,
                        y: lr.head.y + (lr.head.h - s) * 0.5,
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
                        y: lr.head.y + (lr.head.h - s) * 0.5,
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
    /// A key marker: (owner, key time).
    Key(Owner, f32),
    /// The name gutter of a row — select that shape or folder.
    Gutter(Owner),
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
            return Some(LaneHit::Key(lr.owner, t));
        }
        if px < panel.axis.0 {
            return Some(LaneHit::Gutter(lr.owner));
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

/// One React slider inside an expanded lane.
#[derive(Clone)]
pub struct ReactRow {
    pub prop: Prop,
    pub label: &'static str,
    pub label_pos: [f32; 2],
    pub track: Viewport,
    pub t: f32,
    pub value: String,
}

/// Row pitch for one React slider inside an expanded lane.
const REACT_H: f32 = 44.0;

/// A shape's audio-reaction amounts, laid out inside its expanded lane —
/// audio behavior lives with the track, and now with the layer too.
pub fn react_rows(x0: f32, x1: f32, top: f32, scale: f32, react: [f32; 3]) -> Vec<ReactRow> {
    let pad = 10.0 * scale;
    let x = x0 + pad;
    let w = (x1 - x0 - pad * 2.0).max(1.0);
    let mut y = top + 2.0 * scale;
    [
        (Prop::ReactScale, "React Scale", react[0]),
        (Prop::ReactGlow, "React Glow", react[1]),
        (Prop::ReactBright, "React Bright", react[2]),
    ]
    .into_iter()
    .map(|(prop, label, v)| {
        let r = ReactRow {
            prop,
            label,
            label_pos: [x, y],
            track: Viewport {
                x,
                y: y + 24.0 * scale,
                w,
                h: 8.0 * scale,
            },
            t: (v / 2.0).clamp(0.0, 1.0),
            value: format!("{v:.2}x"),
        };
        y += REACT_H * scale;
        r
    })
    .collect()
}
