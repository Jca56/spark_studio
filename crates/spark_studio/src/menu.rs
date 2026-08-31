//! The menu bar: File, Add, Canvas and View at the far left of the title
//! bar (the logo block owns the right). Dispatch stays in `input`, which
//! matches on (menu index, item index).

use spark_render::Viewport;
use spark_ui::{Layout, Menu};

/// The menus, left to right, and their indices.
pub const LABELS: [&str; 4] = ["File", "Add", "Canvas", "View"];
pub const FILE: usize = 0;
pub const ADD: usize = 1;
pub const CANVAS: usize = 2;
pub const VIEW: usize = 3;

/// File menu rows, in display order.
pub const FILE_ITEMS: [&str; 12] = [
    "New",
    "Open...",
    "Save",
    "Save As...",
    "Import Audio...",
    "Save Shape...",
    "Import Shape...",
    "Import Mesh...",
    "New Comp...",
    "Place Comp...",
    "Export Video...",
    "Exit",
];
/// Where the fixed rows sit in `FILE_ITEMS`.
pub const FILE_NEW_COMP: usize = 8;
pub const FILE_PLACE_COMP: usize = 9;
pub const FILE_EXPORT: usize = 10;
pub const FILE_EXIT: usize = 11;

/// Canvas menu rows: the comp's size, one preset per row, the current
/// one drawn accented. The video is the canvas, so this is the export
/// resolution too: a phone's screen is the portrait one. The format
/// takes any even size (`canvas <w> <h>`); these are the ones with names.
pub const CANVAS_PRESETS: [(&str, [f32; 2]); 5] = [
    ("Landscape 1920 × 1080", [1920.0, 1080.0]),
    ("Portrait 1080 × 1920", [1080.0, 1920.0]),
    ("Square 1080 × 1080", [1080.0, 1080.0]),
    ("4K Landscape 3840 × 2160", [3840.0, 2160.0]),
    ("4K Portrait 2160 × 3840", [2160.0, 3840.0]),
];
pub const CANVAS_ITEMS: [&str; 5] = [
    CANVAS_PRESETS[0].0,
    CANVAS_PRESETS[1].0,
    CANVAS_PRESETS[2].0,
    CANVAS_PRESETS[3].0,
    CANVAS_PRESETS[4].0,
];

/// Which preset row `canvas` is, if it is one — a hand-edited size
/// lights no row.
pub fn preset_index(canvas: [f32; 2]) -> Option<usize> {
    CANVAS_PRESETS.iter().position(|(_, c)| *c == canvas)
}
/// Add menu rows: objects that aren't drawn with a tool — the lights, in
/// `LightKind::from_index` order, then the built-in meshes in
/// `primitives::PATHS` order.
pub const ADD_ITEMS: [&str; 7] = [
    "Sun",
    "Point Light",
    "Spot Light",
    "Ambient",
    "Plane",
    "Cube",
    "Sphere",
];
/// View menu rows — all toggles; active ones draw in the accent color.
/// The two cursor rows pick one (or neither) of the Spark cursors.
/// Half-Res Playback renders the stage at half size while the song runs —
/// preview quality for a quiet GPU; the paused picture and export are
/// untouched.
/// Fly View flies an editor-only camera around the scene (`Tab`); 3D
/// Floor draws the floor grid in the comp viewer too.
pub const VIEW_ITEMS: [&str; 9] = [
    "Black Background",
    "Snap to Grid",
    "Smart Guides",
    "Spark Cursor",
    "Spark Cursor II",
    "Materials",
    "Half-Res Playback",
    "Fly View",
    "3D Floor",
];

/// A menu's rows.
pub fn items(mi: usize) -> &'static [&'static str] {
    match mi {
        FILE => &FILE_ITEMS,
        ADD => &ADD_ITEMS,
        CANVAS => &CANVAS_ITEMS,
        _ => &VIEW_ITEMS,
    }
}

/// Every item label on every menu, for measuring the widest.
pub fn all_items() -> impl Iterator<Item = &'static str> {
    (0..LABELS.len()).flat_map(|mi| items(mi).iter().copied())
}

/// Anchor label widths are measured by the caller and cached between
/// frames; `item_w` is the widest item label across every menu.
pub fn build(layout: &Layout, scale: f32, anchor_ws: [f32; 4], item_w: f32) -> [Menu; 4] {
    let mut x = layout.title.x + 10.0 * scale;
    let mut anchor = |label_w: f32| {
        let v = Viewport {
            x,
            y: layout.title.y + 5.0 * scale,
            w: label_w + 32.0 * scale,
            h: layout.title.h - 10.0 * scale,
        };
        x += v.w + 6.0 * scale;
        v
    };
    let file = Menu::new(anchor(anchor_ws[0]), FILE_ITEMS.len(), item_w, scale);
    let add = Menu::new(anchor(anchor_ws[1]), ADD_ITEMS.len(), item_w, scale);
    let canvas = Menu::new(anchor(anchor_ws[2]), CANVAS_ITEMS.len(), item_w, scale);
    let view = Menu::new(anchor(anchor_ws[3]), VIEW_ITEMS.len(), item_w, scale);
    [file, add, canvas, view]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every preset is a size a video encoder takes: even on both sides.
    /// And the default canvas is the first row, so a new comp lights one.
    #[test]
    fn presets_are_even_and_the_default_is_one() {
        for (name, [w, h]) in CANVAS_PRESETS {
            assert!(w % 2.0 == 0.0 && h % 2.0 == 0.0, "{name} is odd-sized");
            assert!(w >= 2.0 && h >= 2.0);
        }
        assert_eq!(preset_index(spark_render::CANVAS), Some(0));
        assert_eq!(preset_index([1080.0, 1920.0]), Some(1));
        assert_eq!(preset_index([1234.0, 1234.0]), None);
    }

    /// The File menu's fixed rows are where dispatch thinks they are.
    #[test]
    fn the_file_menus_named_rows_line_up() {
        assert_eq!(FILE_ITEMS[FILE_NEW_COMP], "New Comp...");
        assert_eq!(FILE_ITEMS[FILE_PLACE_COMP], "Place Comp...");
        assert_eq!(FILE_ITEMS[FILE_EXPORT], "Export Video...");
        assert_eq!(FILE_ITEMS[FILE_EXIT], "Exit");
        assert_eq!(LABELS[CANVAS], "Canvas");
        assert_eq!(items(CANVAS).len(), CANVAS_PRESETS.len());
    }
}
