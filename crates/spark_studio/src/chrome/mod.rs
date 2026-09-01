//! Frame text: every label the editor queues each redraw. Split from main
//! so the event plumbing stays readable.

use spark_render::Viewport;
use spark_text::Text;
use spark_ui::{Layout, Menu, TitleBar, theme};

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

/// The context menu's words, drawn in the overlay pass with the menu's.
pub struct CtxScene {
    pub panel: Viewport,
    /// The active tool's name — `None` is the home panel.
    pub title: Option<&'static str>,
}

/// Everything labels() needs beyond the layout itself.
pub struct Scene<'a> {
    /// The arrangement: track rows and clip bars.
    pub arrange: &'a crate::arrange::ArrangeScene,
    /// Never optional: the timeline keeps its own clock with or without a
    /// track, so the ruler always has bar numbers to draw.
    pub timeline: &'a TlScene,
    pub menus: &'a [Menu; 4],
    pub menu_open: Option<usize>,
    /// The context menu, while it's up.
    pub ctx: Option<CtxScene>,
    /// [black bg, snap grid, smart guides, spark cursor, spark cursor II,
    /// half-res, fly, floor] — active View items draw accented.
    pub view_flags: [bool; 8],
    /// The Canvas menu's row for the comp's current size, if it is a
    /// preset — drawn accented like an active View toggle.
    pub canvas_pick: Option<usize>,
    /// The toolbar's zoom readout button, and the percentage it shows
    /// (100 = exact fit).
    pub zoom: Viewport,
    pub zoom_pct: u16,
    /// Status/hint text centered in the timeline panel, if any.
    pub audio_note: Option<&'a str>,
    /// The transport's tempo field: where it is, what it reads, and whether
    /// it's being typed into. Always present — a comp keeps a tempo before
    /// it has a track to detect one from.
    pub bpm: (Viewport, String, bool),
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
/// The context panel's words: the active tool's name in gold over its
/// draw-defaults page, or the home panel naming itself. Same overlay
/// pass as the menu's labels, for the same z-order reason.
pub fn context_labels(text: &mut Text, scale: f32, scene: &Scene, res: (u32, u32)) {
    let Some(ctx) = &scene.ctx else { return };
    let th = theme();
    let pad = 18.0 * scale;
    let title_size = MENU_TEXT * scale;
    let note_size = 19.0 * scale;
    match ctx.title {
        Some(name) => {
            text.label(
                name,
                title_size,
                ctx.panel.x + pad,
                ctx.panel.y + 14.0 * scale,
                th.accent,
                ctx.panel.w - pad * 2.0,
                res,
            );
            text.label(
                "Draw defaults land here.",
                note_size,
                ctx.panel.x + pad,
                ctx.panel.y + 14.0 * scale + Text::line_height(title_size) + 8.0 * scale,
                th.text_dim,
                ctx.panel.w - pad * 2.0,
                res,
            );
            text.label(
                "Drag on the canvas to draw.",
                note_size,
                ctx.panel.x + pad,
                ctx.panel.y + 14.0 * scale
                    + Text::line_height(title_size)
                    + Text::line_height(note_size)
                    + 12.0 * scale,
                th.text_dim,
                ctx.panel.w - pad * 2.0,
                res,
            );
        }
        None => {
            text.label(
                "Home — filling in soon.",
                note_size,
                ctx.panel.x + pad,
                ctx.panel.y + 14.0 * scale,
                th.text_dim,
                ctx.panel.w - pad * 2.0,
                res,
            );
        }
    }
}

pub fn menu_labels(text: &mut Text, scale: f32, scene: &Scene, res: (u32, u32)) {
    let Some(mi) = scene.menu_open else { return };
    let th = theme();
    let size = UI_TEXT * scale;
    let m = &scene.menus[mi];
    let items = crate::menu::items(mi);
    for (i, (row, label)) in m.items.iter().zip(items).enumerate() {
        // View toggles light up in the accent while enabled.
        // The secondary accent, as it always was — a checked View toggle
        // is not the same kind of "active" as a selection.
        let lit = (mi == crate::menu::VIEW && scene.view_flags[i])
            || (mi == crate::menu::CANVAS && scene.canvas_pick == Some(i));
        let col = if lit { th.accent_alt } else { th.text };
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
    for (m, label) in scene.menus.iter().zip(crate::menu::LABELS) {
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
    {
        // The arrangement's text: track names down the sidebar (dimmed
        // when no clip covers the playhead), a name on every clip bar,
        // and the hint that tells an empty timeline what it's for.
        let size = crate::arrange::TRACK_TEXT * scale;
        let clip = (layout.timeline.y, layout.timeline.y + layout.timeline.h);
        let line = Text::line_height(size);
        let fits = |y: f32| y >= clip.0 && y + line <= clip.1;
        let ar = scene.arrange;
        for tr in &ar.rows {
            if fits(tr.label_pos[1]) {
                let col = if tr.selected {
                    title_col
                } else if tr.dim {
                    th.text_off
                } else {
                    header_col
                };
                text.label(
                    &tr.label,
                    size,
                    tr.label_pos[0],
                    tr.label_pos[1],
                    col,
                    tr.label_max_w,
                    res,
                );
            }
        }
        for cr in &ar.clips {
            if fits(cr.label_pos[1]) {
                let col = if cr.missing { th.red } else { title_col };
                text.label(
                    &cr.label,
                    size,
                    cr.label_pos[0],
                    cr.label_pos[1],
                    col,
                    cr.label_max_w,
                    res,
                );
            }
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
        // The zoom button's live readout — clicking it refits to 100%.
        let zb = scene.zoom;
        let pct = format!("{}%", scene.zoom_pct);
        let w = text.measure(&pct, size);
        text.label(
            &pct,
            size,
            zb.x + (zb.w - w) * 0.5,
            zb.y + (zb.h - Text::line_height(size)) * 0.5,
            // Gold off-100% so a zoomed view is unmissable.
            if scene.zoom_pct == 100 {
                title_col
            } else {
                theme().accent
            },
            zb.w,
            res,
        );
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
}
