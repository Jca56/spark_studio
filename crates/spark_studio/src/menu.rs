//! The menu bar: File and View at the far left of the title bar (the logo
//! block owns the right). Dispatch stays in `input`, which matches on
//! (menu index, item index).

use spark_render::Viewport;
use spark_ui::{Layout, Menu};

/// File menu rows, in display order.
pub const FILE_ITEMS: [&str; 5] = ["Open...", "Save", "Save As...", "Import Audio...", "Exit"];
/// View menu rows — all toggles; active ones draw in the accent color.
pub const VIEW_ITEMS: [&str; 3] = ["Black Background", "Snap to Grid", "Smart Guides"];

/// Anchor label widths are measured by the caller and cached between
/// frames; `item_w` is the widest item label across both menus.
pub fn build(layout: &Layout, scale: f32, anchor_ws: [f32; 2], item_w: f32) -> [Menu; 2] {
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
    let view = Menu::new(anchor(anchor_ws[1]), VIEW_ITEMS.len(), item_w, scale);
    [file, view]
}
