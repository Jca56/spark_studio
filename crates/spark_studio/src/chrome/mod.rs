//! Frame text: every label the editor queues each redraw. Split from main
//! so the event plumbing stays readable.
//!
//! The right panel's cards live in [`cards`] — the one region of this pass
//! with a clip boundary of its own, and the piece that pushed the file past
//! its size budget.

mod cards;

use spark_render::Viewport;
use spark_text::Text;
use spark_ui::{Layout, Menu, Segmented, TextField, TitleBar, theme};

use crate::lanes::{LaneRow, ReactRow};
use crate::layers::{ChoiceRow, LayerRow, ToggleRow};
use crate::menu::{FILE_ITEMS, VIEW_ITEMS};

/// Wordmark font size in logical px (multiply by the output scale).
pub const WM_SIZE: f32 = 30.0;
/// Body label font size in logical px — Alva reads from a distance, keep big.
pub const UI_TEXT: f32 = 23.0;
/// Title-bar menu anchor font size ("File") — a step up from body text.
pub const MENU_TEXT: f32 = 26.0;

/// Timeline text: the ruler's bar numbers.
pub struct TlScene {
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
    pub editing: Option<crate::layers::EditField>,
    pub edit_buf: Option<&'a str>,
    /// React sliders docked in the Keys tab, if any.
    pub react: &'a [ReactRow],
    pub layers: &'a [LayerRow],
    pub folders: &'a [crate::layers::FolderRow],
    /// The folder whose name is being renamed.
    pub renaming_folder: Option<u32>,
    pub lanes: &'a [LaneRow],
    /// Never optional: the timeline keeps its own clock with or without a
    /// track, so the ruler always has bar numbers to draw.
    pub timeline: &'a TlScene,
    /// The effects browser filling the left panel.
    pub browser: &'a crate::browser::Browser,
    pub menus: &'a [Menu; 2],
    pub menu_open: Option<usize>,
    /// [black bg, snap grid, smart guides, spark cursor, spark cursor II,
    /// materials] — active View items draw accented.
    pub view_flags: [bool; 7],
    /// The material playground's rows, when it's open.
    pub materials: Option<&'a crate::materials::Panel>,
    /// Canvas zoom for the zoom bar readout (100 = exact fit).
    pub zoom_pct: u16,
    /// The comp file Save writes to, shown in the title bar.
    pub file: &'a str,
    /// Status/hint text centered in the timeline panel, if any.
    pub audio_note: Option<&'a str>,
    /// An in-progress layer rename: the buffer and its field.
    pub rename: Option<(&'a str, &'a TextField)>,
    /// The transport's tempo field: where it is, what it reads, and whether
    /// it's being typed into. Always present — a comp keeps a tempo before
    /// it has a track to detect one from.
    pub bpm: (Viewport, String, bool),
}

/// One number box's two labels: its name to the left, its value inside.
///
/// Layer cards and folder headers carry the same fields, so they draw
/// through the same routine — they used to diverge, and the folder's
/// version never showed the buffer you were typing.
#[allow(clippy::too_many_arguments)]
fn scrub_labels(
    text: &mut Text,
    f: &crate::layers::ScrubField,
    editing: bool,
    buf: Option<&str>,
    (size, y, scale): (f32, f32, f32),
    (label_col, value_col): ([f32; 4], [f32; 4]),
    res: (u32, u32),
) {
    text.label(
        f.label,
        size,
        f.label_pos[0],
        y,
        label_col,
        crate::layers::SCRUB_LABEL_W * scale,
        res,
    );
    // A field under text edit shows the live buffer instead of the value.
    let shown: &str = if editing { buf.unwrap_or("") } else { &f.value };
    // Right-aligned whether or not it's being typed into: a number that
    // jumps sides on click reads as a different box appearing rather than
    // the same one waking up.
    let w = text.measure(shown, size);
    text.label(
        shown,
        size,
        f.rect.x + f.rect.w - w - crate::layers::FIELD_PAD * scale,
        y,
        if !editing && f.keyed {
            theme().accent
        } else {
            value_col
        },
        f.rect.w,
        res,
    );
}

/// An open menu's item labels, drawn in their own pass *after* the panel
/// that floats them.
///
/// Text is a separate pass from the rects, and one pass has no z-order
/// against another: every label in the editor used to land on top of every
/// rect, so an open File menu covered the layer browser's boxes and the
/// browser's own words then printed straight back through the menu. Nothing
/// about the menu was wrong — it was drawn in the right order — the words
/// underneath it simply were not in the same ordering at all.
pub fn menu_labels(text: &mut Text, scale: f32, scene: &Scene, res: (u32, u32)) {
    let Some(mi) = scene.menu_open else { return };
    let th = theme();
    let size = UI_TEXT * scale;
    let m = &scene.menus[mi];
    let items: &[&str] = if mi == 0 { &FILE_ITEMS } else { &VIEW_ITEMS };
    for (i, (row, label)) in m.items.iter().zip(items).enumerate() {
        // View toggles light up in the accent while enabled.
        // The secondary accent, as it always was — a checked View toggle
        // is not the same kind of "active" as a selection.
        let col = if mi == 1 && scene.view_flags[i] {
            th.accent_alt
        } else {
            th.text
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

pub fn labels(
    text: &mut Text,
    layout: &Layout,
    scale: f32,
    tb: &TitleBar,
    scene: &Scene,
    res: (u32, u32),
) {
    let th = theme();
    let title_col = th.text;
    let header_col = th.text_dim;
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
    // What the picker has hold of, when it is not the selection. The same
    // square paints a shape one moment and the side panels the next, so it
    // says which — an unlabelled control that changes meaning is a trap.
    if let Some(name) = &scene.color.caption {
        text.label(
            name,
            size,
            scene.color.region.x + 14.0 * scale,
            scene.color.region.y + 12.0 * scale,
            th.accent,
            scene.color.region.w - 28.0 * scale,
            res,
        );
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
    cards::labels(text, scale, size, scene, res);
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
                r.value_right - w,
                r.track.y + (r.track.h - Text::line_height(react_size)) * 0.5,
                title_col,
                w + 2.0,
                res,
            );
        }
    }
    {
        let tl = scene.timeline;
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
        // The effects browser: a caption and one name per kind.
        text.label(
            "EFFECTS",
            crate::layers::CARD_TEXT * scale,
            scene.browser.caption_pos[0],
            scene.browser.caption_pos[1],
            header_col,
            layout.left.w,
            res,
        );
        for r in &scene.browser.rows {
            text.label(
                r.kind.label(),
                size,
                r.label_pos[0],
                r.label_pos[1],
                title_col,
                r.row.w,
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
                theme().accent
            },
            zb.pct.w,
            res,
        );
    }
    if let Some(mp) = scene.materials {
        crate::materials::labels(text, mp, layout.timeline, scale, res);
    }
    // Tempo: the number big and centred, "BPM" small beside it so the field
    // says what it is without a separate caption row.
    {
        let (rect, reading, editing) = &scene.bpm;
        let num_size = 30.0 * scale;
        let cap_size = 17.0 * scale;
        let gap = 7.0 * scale;
        let nw = text.measure(reading, num_size);
        let cw = text.measure("BPM", cap_size);
        let x = rect.x + (rect.w - (nw + gap + cw)) * 0.5;
        text.label(
            reading,
            num_size,
            x,
            rect.y + (rect.h - Text::line_height(num_size)) * 0.5,
            if *editing { th.accent } else { title_col },
            rect.w,
            res,
        );
        text.label(
            "BPM",
            cap_size,
            x + nw + gap,
            rect.y + (rect.h - Text::line_height(cap_size)) * 0.5 + 3.0 * scale,
            header_col,
            rect.w,
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
fn toggle_labels(
    text: &mut Text,
    row: &ToggleRow,
    title: &str,
    options: [&str; 2],
    size: f32,
    clip: (f32, f32),
    res: (u32, u32),
) {
    segment_labels(
        text,
        &row.seg,
        row.label_pos,
        title,
        &options,
        row.on as usize,
        size,
        clip,
        res,
    );
}

/// The same, for a row with more than two options.
fn choice_labels(
    text: &mut Text,
    row: &ChoiceRow,
    title: &str,
    size: f32,
    clip: (f32, f32),
    res: (u32, u32),
) {
    segment_labels(
        text,
        &row.seg,
        row.label_pos,
        title,
        row.options,
        row.active,
        size,
        clip,
        res,
    );
}

/// Grey title above, one label per segment, the active one in gold. Labels
/// vertically outside `clip` are skipped (scrolled away).
#[allow(clippy::too_many_arguments)]
fn segment_labels(
    text: &mut Text,
    seg: &Segmented,
    label_pos: [f32; 2],
    title: &str,
    options: &[&str],
    active: usize,
    size: f32,
    clip: (f32, f32),
    res: (u32, u32),
) {
    let title_grey = theme().text_dim;
    // Gold carries active state — purple stays a secondary accent.
    let accent = theme().accent;
    let line = Text::line_height(size);
    let vis = |y: f32| y >= clip.0 && y + line <= clip.1;
    if vis(label_pos[1]) {
        text.label(
            title,
            size,
            label_pos[0],
            label_pos[1],
            title_grey,
            4000.0,
            res,
        );
    }
    for (i, name) in options.iter().enumerate() {
        let Some(&slot) = seg.segments.get(i) else {
            continue;
        };
        let y = slot.y + (slot.h - line) * 0.5;
        if !vis(y) {
            continue;
        }
        let w = text.measure(name, size);
        text.label(
            name,
            size,
            slot.x + (slot.w - w) * 0.5,
            y,
            if i == active { accent } else { title_grey },
            slot.w,
            res,
        );
    }
}
