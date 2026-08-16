//! Frame text: every label the editor queues each redraw. Split from main
//! so the event plumbing stays readable.

use spark_text::Text;
use spark_ui::{Layout, Menu, TextField, TitleBar, srgb, theme};

use crate::inspector::{Inspector, ToggleRow};
use crate::layers::LayerRow;
use crate::menu::{FILE_ITEMS, VIEW_ITEMS};

/// Wordmark font size in logical px (multiply by the output scale).
pub const WM_SIZE: f32 = 30.0;
/// Body label font size in logical px — Alva reads from a distance, keep big.
pub const UI_TEXT: f32 = 23.0;
/// Title-bar menu anchor font size ("File") — a step up from body text.
pub const MENU_TEXT: f32 = 26.0;

/// Everything labels() needs beyond the layout itself.
pub struct Scene<'a> {
    pub insp: Option<&'a Inspector>,
    pub layers: &'a [LayerRow],
    pub menus: &'a [Menu; 2],
    pub menu_open: Option<usize>,
    /// [black bg, snap grid, smart guides] — active View items draw accented.
    pub view_flags: [bool; 3],
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
    if let Some(insp) = scene.insp {
        for row in &insp.rows {
            text.label(
                row.label,
                size,
                row.label_pos[0],
                row.label_pos[1],
                header_col,
                layout.left.w,
                res,
            );
            let value_w = text.measure(&row.value, size);
            text.label(
                &row.value,
                size,
                row.track.x + row.track.w - value_w,
                row.label_pos[1],
                title_col,
                layout.left.w,
                res,
            );
        }
        text.label(
            "Color",
            size,
            insp.color_label_pos[0],
            insp.color_label_pos[1],
            header_col,
            layout.left.w,
            res,
        );
        if let Some(mode) = &insp.mode {
            toggle_labels(text, mode, "Style", ["Fill", "Outline"], size, res);
        }
        toggle_labels(text, &insp.blend, "Blend", ["Solid", "Add"], size, res);
    }
    for lr in scene.layers {
        text.label(
            &lr.label,
            size,
            lr.label_pos[0],
            lr.label_pos[1],
            if lr.selected { title_col } else { header_col },
            layout.right.w,
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
    res: (u32, u32),
) {
    let title_grey = srgb(0xb2b2b2);
    let accent = theme().accent;
    text.label(
        title,
        size,
        row.label_pos[0],
        row.label_pos[1],
        title_grey,
        4000.0,
        res,
    );
    for (i, name) in options.iter().enumerate() {
        let seg = row.seg.segments[i];
        let active = (i == 1) == row.on;
        let w = text.measure(name, size);
        text.label(
            name,
            size,
            seg.x + (seg.w - w) * 0.5,
            seg.y + (seg.h - Text::line_height(size)) * 0.5,
            if active { accent } else { title_grey },
            seg.w,
            res,
        );
    }
}
