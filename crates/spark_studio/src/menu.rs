//! The File menu: item list and geometry. Lives at the far left of the
//! title bar (the logo block owns the right). Dispatch stays in main, which
//! matches on the clicked item index.

use spark_render::Viewport;
use spark_ui::{Layout, Menu};

/// Menu rows, in display order. `press` dispatches by index.
pub const FILE_ITEMS: [&str; 5] = ["Open...", "Save", "Save As...", "Import Audio...", "Exit"];

/// `file_w` / `item_w` are the measured widths of the anchor label and the
/// widest item label (physical px), cached by the caller between frames.
pub fn build(layout: &Layout, scale: f32, file_w: f32, item_w: f32) -> Menu {
    let anchor = Viewport {
        x: layout.title.x + 10.0 * scale,
        y: layout.title.y + 5.0 * scale,
        w: file_w + 32.0 * scale,
        h: layout.title.h - 10.0 * scale,
    };
    Menu::new(anchor, FILE_ITEMS.len(), item_w, scale)
}
