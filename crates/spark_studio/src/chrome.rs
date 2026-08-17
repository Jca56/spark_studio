//! Frame text: every label the editor queues each redraw. Split from main
//! so the event plumbing stays readable.

use spark_render::Viewport;
use spark_text::Text;
use spark_ui::{Layout, Menu, TextField, TitleBar, srgb, theme};

use crate::lanes::{LaneRow, ReactRow};
use crate::layers::{LayerRow, ToggleRow};
use crate::menu::{FILE_ITEMS, VIEW_ITEMS};

/// Wordmark font size in logical px (multiply by the output scale).
pub const WM_SIZE: f32 = 30.0;
/// Body label font size in logical px — Alva reads from a distance, keep big.
pub const UI_TEXT: f32 = 23.0;
/// Title-bar menu anchor font size ("File") — a step up from body text.
pub const MENU_TEXT: f32 = 26.0;

/// Timeline text: the Arrange/Keys toggle's label and the ruler's bar
/// numbers.
pub struct TlScene {
    /// The rectangular Arrange/Keys toggle and the tab name it shows.
    pub tk: Viewport,
    pub tk_label: &'static str,
    /// The shown tab is the active one — its label goes gold.
    pub tk_active: bool,
    /// Bar-number labels: (x, text), on the ruler row.
    pub marks: Vec<(f32, String)>,
    pub ruler: Viewport,
}

/// Everything labels() needs beyond the layout itself.
pub struct Scene<'a> {
    pub color: &'a crate::colorhome::ColorHome,
    /// The layer-cards region — card text clips to it.
    pub cards: Viewport,
    /// The card whose name is being renamed (its label hides under the
    /// rename field).
    pub renaming: Option<usize>,
    /// The scrub field being text-edited and its live buffer.
    pub editing: Option<(usize, crate::editor::Prop)>,
    pub edit_buf: Option<&'a str>,
    /// React sliders docked in the Keys tab, if any.
    pub react: &'a [ReactRow],
    pub layers: &'a [LayerRow],
    pub lanes: &'a [LaneRow],
    pub timeline: Option<&'a TlScene>,
    pub menus: &'a [Menu; 2],
    pub menu_open: Option<usize>,
    /// [black bg, snap grid, smart guides, spark cursor, spark cursor II]
    /// — active View items draw accented.
    pub view_flags: [bool; 5],
    /// Canvas zoom for the zoom bar readout (100 = exact fit).
    pub zoom_pct: u16,
    /// The comp file Save writes to, shown in the title bar.
    pub file: &'a str,
    /// Status/hint text centered in the timeline panel, if any.
    pub audio_note: Option<&'a str>,
    /// An in-progress layer rename: the buffer and its field.
    pub rename: Option<(&'a str, &'a TextField)>,
}

pub fn labels(
    text: &mut Text,
    layout: &Layout,
    scale: f32,
    tb: &TitleBar,
    scene: &Scene,
    res: (u32, u32),
) {
    let title_col = srgb(0xf2f2f2);
    let header_col = srgb(0xb2b2b2);
    let accent = theme().accent;
    let size = UI_TEXT * scale;
    let wm_size = WM_SIZE * scale;
    text.label_bold(
        "SPARK STUDIO",
        wm_size,
        tb.wordmark_x(),
        layout.title.y + (layout.title.h - Text::line_height(wm_size)) * 0.5,
        title_col,
        layout.title.w,
        res,
    );
    let menu_size = MENU_TEXT * scale;
    for (m, label) in scene.menus.iter().zip(["File", "View"]) {
        let w = text.measure(label, menu_size);
        text.label(
            label,
            menu_size,
            m.anchor.x + (m.anchor.w - w) * 0.5,
            m.anchor.y + (m.anchor.h - Text::line_height(menu_size)) * 0.5,
            title_col,
            m.anchor.w,
            res,
        );
    }
    let name_w = text.measure(scene.file, size);
    text.label(
        scene.file,
        size,
        layout.title.x + (layout.title.w - name_w) * 0.5,
        layout.title.y + (layout.title.h - Text::line_height(size)) * 0.5,
        header_col,
        layout.title.w,
        res,
    );
    if let Some(mi) = scene.menu_open {
        let m = &scene.menus[mi];
        let items: &[&str] = if mi == 0 { &FILE_ITEMS } else { &VIEW_ITEMS };
        for (i, (row, label)) in m.items.iter().zip(items).enumerate() {
            // View toggles light up in the accent while enabled.
            let col = if mi == 1 && scene.view_flags[i] {
                accent
            } else {
                title_col
            };
            text.label(
                label,
                size,
                row.x + 16.0 * scale,
                row.y + (row.h - Text::line_height(size)) * 0.5,
                col,
                row.w,
                res,
            );
        }
    }
    if let Some((_, hsv, hex_pos)) = &scene.color.picker {
        text.label(
            &crate::colorhome::hex_of(*hsv),
            size,
            hex_pos[0],
            hex_pos[1],
            title_col,
            layout.right.w,
            res,
        );
    }
    {
        // Layer cards: name, scrub fields, and expanded detail — all
        // clipped to the cards region under the pinned color home.
        let clip = (scene.cards.y, scene.cards.y + scene.cards.h);
        let line = Text::line_height(size);
        let card_size = crate::layers::CARD_TEXT * scale;
        let card_line = Text::line_height(card_size);
        let vis = |y: f32, l: f32| y >= clip.0 && y + l <= clip.1;
        for lr in scene.layers {
            if vis(lr.label_pos[1], line) && scene.renaming != Some(lr.index) {
                text.label(
                    &lr.label,
                    size,
                    lr.label_pos[0],
                    lr.label_pos[1],
                    if lr.hidden {
                        srgb(0x6a6a6a)
                    } else if lr.selected {
                        title_col
                    } else {
                        header_col
                    },
                    (lr.row.x + lr.row.w - lr.label_pos[0] - 50.0 * scale).max(40.0),
                    res,
                );
            }
            for f in &lr.scrubs {
                let y = f.rect.y + (f.rect.h - card_line) * 0.5;
                if !vis(y, card_line) {
                    continue;
                }
                text.label(
                    f.label,
                    card_size,
                    f.rect.x + 7.0 * scale,
                    y,
                    header_col,
                    f.rect.w,
                    res,
                );
                // A field under text edit shows the live buffer instead.
                let editing_this = scene.editing == Some((lr.index, f.prop));
                let shown: &str = if editing_this {
                    scene.edit_buf.unwrap_or("")
                } else {
                    &f.value
                };
                let w = text.measure(shown, card_size);
                text.label(
                    shown,
                    card_size,
                    f.rect.x + f.rect.w - w - 7.0 * scale,
                    y,
                    if editing_this {
                        title_col
                    } else if f.keyed {
                        theme().playhead
                    } else {
                        title_col
                    },
                    f.rect.w,
                    res,
                );
            }
            if let Some(d) = &lr.detail {
                for row in &d.sliders {
                    if !vis(row.label_pos[1], card_line) {
                        continue;
                    }
                    text.label(
                        row.label,
                        card_size,
                        row.label_pos[0],
                        row.label_pos[1],
                        header_col,
                        row.track.w,
                        res,
                    );
                    let w = text.measure(&row.value, card_size);
                    text.label(
                        &row.value,
                        card_size,
                        row.track.x + row.track.w - w,
                        row.label_pos[1],
                        if row.keyed {
                            theme().playhead
                        } else {
                            title_col
                        },
                        row.track.w,
                        res,
                    );
                }
                if let Some(st) = &d.style {
                    toggle_labels(text, st, "Style", ["Fill", "Outline"], card_size, clip, res);
                }
                toggle_labels(
                    text,
                    &d.blend,
                    "Blend",
                    ["Solid", "Add"],
                    card_size,
                    clip,
                    res,
                );
                toggle_labels(
                    text,
                    &d.grad,
                    "Gradient",
                    ["Off", "On"],
                    card_size,
                    clip,
                    res,
                );
            }
        }
    }
    {
        // Lane names clip to the timeline panel like the other lists. A
        // slightly smaller face keeps the 42px rows breathing.
        let lane_size = crate::lanes::LANE_TEXT * scale;
        let clip = (layout.timeline.y, layout.timeline.y + layout.timeline.h);
        let line = Text::line_height(lane_size);
        for lr in scene.lanes {
            let y = lr.label_pos[1];
            if y < clip.0 || y + line > clip.1 {
                continue;
            }
            text.label(
                &lr.label,
                lane_size,
                lr.label_pos[0],
                y,
                if lr.selected { title_col } else { header_col },
                lr.label_max_w,
                res,
            );
        }
    }
    {
        // React sliders in the Keys sidebar: label left, value right.
        let react_size = crate::lanes::LANE_TEXT * scale;
        for r in scene.react {
            text.label(
                r.label,
                react_size,
                r.label_pos[0],
                r.label_pos[1],
                header_col,
                r.track.w,
                res,
            );
            let w = text.measure(&r.value, react_size);
            text.label(
                &r.value,
                react_size,
                r.track.x + r.track.w - w,
                r.label_pos[1],
                title_col,
                r.track.w,
                res,
            );
        }
    }
    if let Some(tl) = scene.timeline {
        let seg = tl.tk;
        let w = text.measure(tl.tk_label, size);
        text.label(
            tl.tk_label,
            size,
            seg.x + (seg.w - w) * 0.5,
            seg.y + (seg.h - Text::line_height(size)) * 0.5,
            // Active tab: gold on the purple card, the toolbar recipe.
            if tl.tk_active {
                theme().grad_gold
            } else {
                header_col
            },
            seg.w,
            res,
        );
        let mark_size = 17.0 * scale;
        for (x, label) in &tl.marks {
            text.label(
                label,
                mark_size,
                *x,
                tl.ruler.y + 2.0 * scale,
                header_col,
                80.0 * scale,
                res,
            );
        }
    }
    {
        // The zoom button's live readout — clicking it refits to 100%.
        let zb = crate::view::zoom_bar(layout.zoom, scale);
        let pct = format!("{}%", scene.zoom_pct);
        let w = text.measure(&pct, size);
        text.label(
            &pct,
            size,
            zb.pct.x + (zb.pct.w - w) * 0.5,
            zb.pct.y + (zb.pct.h - Text::line_height(size)) * 0.5,
            // Gold off-100% so a zoomed view is unmissable.
            if scene.zoom_pct == 100 {
                title_col
            } else {
                theme().grad_gold
            },
            zb.pct.w,
            res,
        );
    }
    if let Some(note) = scene.audio_note {
        let w = text.measure(note, size);
        let tl = layout.timeline;
        text.label(
            note,
            size,
            tl.x + (tl.w - w) * 0.5,
            tl.y + (tl.h - Text::line_height(size)) * 0.5,
            header_col,
            tl.w,
            res,
        );
    }
    if let Some((buf, field)) = scene.rename {
        text.label(
            buf,
            size,
            field.text_x(),
            field.rect.y + (field.rect.h - Text::line_height(size)) * 0.5,
            title_col,
            field.rect.w,
            res,
        );
    }
}

/// A labeled two-option segmented row: grey title, accented active option.
/// Labels vertically outside `clip` are skipped (scrolled away).
fn toggle_labels(
    text: &mut Text,
    row: &ToggleRow,
    title: &str,
    options: [&str; 2],
    size: f32,
    clip: (f32, f32),
    res: (u32, u32),
) {
    let title_grey = srgb(0xb2b2b2);
    // Gold carries active state — purple stays a secondary accent.
    let accent = theme().grad_gold;
    let line = Text::line_height(size);
    let vis = |y: f32| y >= clip.0 && y + line <= clip.1;
    if vis(row.label_pos[1]) {
        text.label(
            title,
            size,
            row.label_pos[0],
            row.label_pos[1],
            title_grey,
            4000.0,
            res,
        );
    }
    for (i, name) in options.iter().enumerate() {
        let seg = row.seg.segments[i];
        let y = seg.y + (seg.h - line) * 0.5;
        if !vis(y) {
            continue;
        }
        let active = (i == 1) == row.on;
        let w = text.measure(name, size);
        text.label(
            name,
            size,
            seg.x + (seg.w - w) * 0.5,
            y,
            if active { accent } else { title_grey },
            seg.w,
            res,
        );
    }
}
