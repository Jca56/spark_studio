//! The context menu: right-click opens a floating panel at the cursor.
//! More than a menu — controls can sit *outside* the panel body as
//! satellites, and the first is the shape-tool rail down its left flank.
//!
//! A tool click **selects** that tool and the panel becomes its
//! draw-defaults page (what the shape will look like the moment it is
//! drawn — configurable before drawing, which the old design never
//! allowed); clicking the active tool again unselects back to Move and
//! the home panel. Tool clicks never close the menu; a click anywhere
//! else does, swallowed either way — the app menus' rule. The defaults
//! themselves land next; the page names itself honestly meanwhile.
//!
//! It draws in the overlay submit beside the app menus — one `UiPass`
//! owns one instance buffer, so a floating panel must be a second full
//! stack or the words underneath print straight through it.

use spark_render::Viewport;
use spark_ui::{
    ICON_ARROW, ICON_CIRCLE, ICON_LINE, ICON_PENTAGON, ICON_SQUARE, ICON_STARS, UiRect, surfaces,
    theme,
};

use crate::Studio;
use crate::editor::Tool;

/// Panel body size, logical px — an empty page until content lands.
const PANEL_W: f32 = 340.0;
const PANEL_H: f32 = 420.0;
/// Air between the rail and the panel's left edge.
const RAIL_GAP: f32 = 12.0;
/// Air between rail buttons.
const BTN_GAP: f32 = 10.0;

/// The drawing tools the rail offers, top to bottom — the same order the
/// number keys pick them (`1` move … `6` stars) — and the name the
/// panel's title calls each.
const RAIL: [(Tool, f32, &str); 6] = [
    (Tool::Select, ICON_ARROW, "Move"),
    (Tool::Circle, ICON_CIRCLE, "Circle"),
    (Tool::Box, ICON_SQUARE, "Box"),
    (Tool::Polygon, ICON_PENTAGON, "Polygon"),
    (Tool::Line, ICON_LINE, "Line"),
    (Tool::Stars, ICON_STARS, "Star Field"),
];

/// The panel's title for the active tool — the home panel for Move.
pub fn tool_title(tool: Tool) -> Option<&'static str> {
    RAIL.iter()
        .find(|(t, _, _)| *t == tool && *t != Tool::Select)
        .map(|(_, _, name)| *name)
}

/// Solved context-menu geometry: the panel and its satellites.
pub struct Ctx {
    /// The panel body.
    pub panel: Viewport,
    /// The tool rail outside the panel's left edge: square buttons stacked
    /// to exactly the panel's height.
    pub rail: [(Tool, f32, Viewport); 6],
}

/// Lay the menu out with the panel's top-left at `anchor`, pulled back
/// inside `win` when the click lands near an edge. The rail is part of
/// the footprint: a menu opened at the window's far left keeps its
/// satellites on screen, not just its body.
pub fn build(anchor: [f32; 2], scale: f32, win: Viewport) -> Ctx {
    let (w, h) = (PANEL_W * scale, PANEL_H * scale);
    let gap = RAIL_GAP * scale;
    let btn_gap = BTN_GAP * scale;
    // Six squares and five gaps span exactly the panel's height, so the
    // rail grows with whatever the panel becomes.
    let side = (h - btn_gap * 5.0) / 6.0;
    let rail_w = side + gap;
    let x = anchor[0].min(win.x + win.w - w).max(win.x + rail_w);
    let y = anchor[1].min(win.y + win.h - h).max(win.y);
    let panel = Viewport { x, y, w, h };
    let rail = std::array::from_fn(|i| {
        let (tool, icon, _) = RAIL[i];
        let b = Viewport {
            x: panel.x - rail_w,
            y: panel.y + (side + btn_gap) * i as f32,
            w: side,
            h: side,
        };
        (tool, icon, b)
    });
    Ctx { panel, rail }
}

/// The panel, then each rail button as its own floating plate — the
/// active tool in Spark's two accents, the way every lit toggle reads.
pub fn rects(ctx: &Ctx, scale: f32, active: Tool, hover: Option<usize>) -> Vec<UiRect> {
    let t = theme();
    let mut out = vec![surfaces().float.rect(ctx.panel, scale)];
    for (i, &(tool, icon, b)) in ctx.rail.iter().enumerate() {
        // Every state is the same raised plate — the armed tool wears the
        // purple face under a gold edge instead of going flat.
        let plate = surfaces().plate;
        out.push(if tool == active {
            plate.filled(t.accent_alt_bg).edge(2.0, t.accent).rect(b, scale)
        } else if hover == Some(i) {
            plate.filled(t.button_hover).rect(b, scale)
        } else {
            plate.rect(b, scale)
        });
        let fg = if tool == active {
            // Gold glyph on the purple highlight — Spark's two accents.
            t.accent
        } else if hover == Some(i) {
            t.icon_hover
        } else {
            t.icon
        };
        out.push(UiRect::icon_sized(b, icon, 2.0 * scale, fg, 0.34));
    }
    out
}

impl Studio {
    /// A right press lands: open the menu at the cursor if this is a
    /// region the menu owns — the viewport (comp viewer; the fly view's
    /// right button pans) and the empty side panels. The timeline's
    /// right-clicks keep their jobs (delete key, clear loop).
    pub(crate) fn context_press(&mut self) -> bool {
        let (cx, cy) = (self.cursor_px.0 as f32, self.cursor_px.1 as f32);
        let Some(layout) = self.layout() else {
            return false;
        };
        let opens = layout.viewport.contains(cx, cy)
            || layout.left.contains(cx, cy)
            || layout.right.contains(cx, cy);
        if !opens {
            return false;
        }
        self.ctx_menu = Some([cx, cy]);
        self.ctx_hover = None;
        self.request_redraw();
        true
    }

    /// The open menu's geometry, from the same inputs the frame draws
    /// with — hit tests and rects must never disagree.
    pub(crate) fn context(&self) -> Option<Ctx> {
        let anchor = self.ctx_menu?;
        let (w, h) = self.gpu.as_ref()?.size();
        Some(build(
            anchor,
            self.scale(),
            Viewport {
                x: 0.0,
                y: 0.0,
                w: w as f32,
                h: h as f32,
            },
        ))
    }

    /// A left press while the menu is up. A rail button toggles its tool
    /// — select it, or unselect the active one back to Move — and the
    /// menu stays open; a press on the panel body is the menu's own
    /// (options land there next); anywhere else closes it. Always true:
    /// an open menu owns the click.
    pub(crate) fn context_press_left(&mut self, cx: f32, cy: f32) -> bool {
        if self.ctx_menu.is_none() {
            return false;
        }
        let Some(ctx) = self.context() else {
            return self.context_close();
        };
        if let Some(&(tool, _, _)) = ctx.rail.iter().find(|(_, _, b)| b.contains(cx, cy)) {
            let next = if self.editor.tool() == tool {
                // The active tool unselects back to the hand.
                Tool::Select
            } else {
                tool
            };
            self.editor.choose_tool(next);
            self.request_redraw();
            return true;
        }
        if ctx.panel.contains(cx, cy) {
            // The panel's own real estate — the defaults page will take
            // these clicks; until then they are simply not a dismissal.
            return true;
        }
        self.context_close();
        true
    }

    /// Close the menu if it is open; true when the gesture was spent
    /// doing it — the app menus' rule: an open menu owns the click.
    pub(crate) fn context_close(&mut self) -> bool {
        if self.ctx_menu.take().is_some() {
            self.ctx_hover = None;
            self.request_redraw();
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn win() -> Viewport {
        Viewport {
            x: 0.0,
            y: 0.0,
            w: 3840.0,
            h: 2160.0,
        }
    }

    /// The rail rides the panel's left flank: every button fully outside
    /// the body, square, one column, and the stack spanning exactly the
    /// panel's height — top button flush with the top edge, bottom
    /// button flush with the bottom.
    #[test]
    fn the_rail_spans_exactly_the_panels_height() {
        for scale in [1.0f32, 1.4] {
            let c = build([1000.0, 600.0], scale, win());
            let first = c.rail[0].2;
            let last = c.rail[5].2;
            assert!(
                (first.y - c.panel.y).abs() < 0.5,
                "scale {scale}: the rail doesn't start at the panel's top"
            );
            assert!(
                (last.y + last.h - (c.panel.y + c.panel.h)).abs() < 0.5,
                "scale {scale}: the rail misses the panel's bottom by {}",
                (c.panel.y + c.panel.h) - (last.y + last.h)
            );
            for (_, _, b) in &c.rail {
                assert!(
                    b.x + b.w < c.panel.x,
                    "scale {scale}: a button leaks into the panel"
                );
                assert!((b.w - b.h).abs() < 0.5, "scale {scale}: not square");
                assert!((b.x - first.x).abs() < 0.5, "scale {scale}: not a column");
            }
        }
    }

    /// A click in any corner keeps the whole assembly on screen — the
    /// rail is part of the footprint, so a far-left click must not push
    /// the satellites off the window.
    #[test]
    fn a_corner_click_keeps_the_whole_assembly_on_screen() {
        for scale in [1.0f32, 1.4] {
            for corner in [
                [0.0, 0.0],
                [3840.0, 0.0],
                [0.0, 2160.0],
                [3840.0, 2160.0],
            ] {
                let c = build(corner, scale, win());
                assert!(
                    c.rail[0].2.x >= 0.0,
                    "scale {scale} corner {corner:?}: the rail fell off the left"
                );
                assert!(c.panel.x + c.panel.w <= 3840.0 + 0.5);
                assert!(c.panel.y >= 0.0 && c.panel.y + c.panel.h <= 2160.0 + 0.5);
            }
        }
    }

    /// Six tools, each once, `1` first — the same order the number keys
    /// pick them.
    #[test]
    fn the_rail_lists_each_tool_once_in_key_order() {
        assert_eq!(RAIL[0].0, Tool::Select, "Move leads, and `1` picks it");
        for (i, (tool, _, _)) in RAIL.iter().enumerate() {
            assert!(
                !RAIL[..i].iter().any(|(t, _, _)| t == tool),
                "{tool:?} is on the rail twice"
            );
        }
        // Every drawing tool titles its defaults page; Move is the home.
        assert_eq!(tool_title(Tool::Select), None);
        assert_eq!(tool_title(Tool::Stars), Some("Star Field"));
    }
}
