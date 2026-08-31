//! The menu bar: File, Add and View at the far left of the title bar (the
//! logo block owns the right). Dispatch stays in `input`, which matches on
//! (menu index, item index).

use spark_render::Viewport;
use spark_ui::{Layout, Menu};

/// The menus, left to right, and their indices.
pub const LABELS: [&str; 3] = ["File", "Add", "View"];
pub const FILE: usize = 0;
pub const ADD: usize = 1;
pub const VIEW: usize = 2;

/// File menu rows, in display order.
pub const FILE_ITEMS: [&str; 9] = [
    "New",
    "Open...",
    "Save",
    "Save As...",
    "Import Audio...",
    "Save Shape...",
    "Import Shape...",
    "Import Mesh...",
    "Exit",
];
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
        _ => &VIEW_ITEMS,
    }
}

/// Anchor label widths are measured by the caller and cached between
/// frames; `item_w` is the widest item label across every menu.
pub fn build(layout: &Layout, scale: f32, anchor_ws: [f32; 3], item_w: f32) -> [Menu; 3] {
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
    let view = Menu::new(anchor(anchor_ws[2]), VIEW_ITEMS.len(), item_w, scale);
    [file, add, view]
}
