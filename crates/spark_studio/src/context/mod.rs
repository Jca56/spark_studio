//! The context menu: right-click opens a floating panel at the cursor —
//! **the tool home** (Alva's spec, 2026-08-31). A rail of shape tools
//! rides the panel's left flank; the body is the armed tool's
//! **draw-defaults page** — what the shape will look like the moment it
//! is drawn, configurable *before* drawing, which nothing in Spark
//! allowed before — or, with Move armed, **Home**: what the menu offers
//! for the thing that was under the cursor when it opened (`home`).
//! A right-click on a shape selects it first, so Home opens on the thing
//! you pointed at.
//!
//! One shape, always: a tall rectangle, [`PANEL_W`] × [`PANEL_H`], with
//! the rail's small fixed squares top-aligned beside it. A tool click
//! selects that tool and the panel becomes its page; clicking the active
//! tool again unselects back to Move and Home. Tool clicks and slider
//! drags never close the menu; a verb acts and closes it; a click
//! anywhere else closes it, swallowed either way — the app menus' rule.
//!
//! The pages' numbers are sliders (the dial is kept for elsewhere):
//! press or drag anywhere on a slider's band, the wheel steps. Colour is
//! not here: the permanent colour home goes in the right panel.
//!
//! It draws in the overlay submit beside the app menus — one `UiPass`
//! owns one instance buffer, so a floating panel must be a second full
//! stack or the words underneath print straight through it.

mod home;
mod page;
#[cfg(test)]
mod tests;

pub use home::Target;
pub use page::{Align, Hit, Label, Page};

use spark_render::Viewport;
use spark_ui::{
    ICON_ARROW, ICON_CIRCLE, ICON_LINE, ICON_PENTAGON, ICON_SQUARE, ICON_STARS, Slider, UiRect,
    surfaces, theme,
};

use crate::Studio;
use crate::editor::Tool;

/// Panel body size, logical px — one tall rectangle for every page.
pub const PANEL_W: f32 = 420.0;
pub const PANEL_H: f32 = 680.0;
/// A rail button's side, logical px — the transport's squares, not a
/// sixth of the panel (which at 680 px made them "way too big").
const RAIL_SIDE: f32 = 52.0;
/// Air between rail buttons, and between the rail and the panel.
const BTN_GAP: f32 = 8.0;
const RAIL_GAP: f32 = 12.0;

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
    /// The tool rail outside the panel's left edge: fixed-size squares in
    /// a column, top-aligned with the panel.
    pub rail: [(Tool, f32, Viewport); 6],
}

/// Lay the menu out with the panel's top-left at `anchor`, pulled back
/// inside `win` when the click lands near an edge. The rail is part of
/// the footprint: a menu opened at the window's far left keeps its
/// satellites on screen, not just its body.
pub fn build(anchor: [f32; 2], scale: f32, win: Viewport) -> Ctx {
    let (w, h) = (PANEL_W * scale, PANEL_H * scale);
    let side = RAIL_SIDE * scale;
    let rail_w = side + RAIL_GAP * scale;
    let x = anchor[0].min(win.x + win.w - w).max(win.x + rail_w);
    let y = anchor[1].min(win.y + win.h - h).max(win.y);
    let panel = Viewport { x, y, w, h };
    let rail = std::array::from_fn(|i| {
        let (tool, icon, _) = RAIL[i];
        let b = Viewport {
            x: panel.x - rail_w,
            y: panel.y + (side + BTN_GAP * scale) * i as f32,
            w: side,
            h: side,
        };
        (tool, icon, b)
    });
    Ctx { panel, rail }
}

/// The panel, then each rail button as its own floating plate — the
/// active tool in Spark's two accents, the way every lit toggle reads.
pub fn rail_rects(ctx: &Ctx, scale: f32, active: Tool, hover: Option<usize>) -> Vec<UiRect> {
    let t = theme();
    let mut out = vec![surfaces().float.rect(ctx.panel, scale)];
    for (i, &(tool, icon, b)) in ctx.rail.iter().enumerate() {
        // Every state is the same raised plate — the armed tool wears the
        // purple face under a gold edge instead of going flat.
        let plate = surfaces().plate;
        out.push(if tool == active {
            plate
                .filled(t.accent_alt_bg)
                .edge(2.0, t.accent)
                .rect(b, scale)
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

/// A drag on the page: a slider's thumb following the cursor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Drag {
    Slider(usize),
}

/// A wheel notch on a slider: a fiftieth of the range, a fine
/// two-hundredth with Shift.
pub fn slider_step(v: f32, notches: f32, fine: bool) -> f32 {
    let step = if fine { 0.005 } else { 0.02 };
    (v + notches * step).clamp(0.0, 1.0)
}

impl Studio {
    /// A right press lands: open the menu at the cursor if this is a
    /// region the menu owns — the viewport (comp viewer; the fly view's
    /// right button pans) and the empty side panels. The timeline's
    /// right-clicks keep their jobs (delete key, clear loop). The
    /// menu's subject is whatever was under the cursor: over the canvas
    /// with Move armed, an object there is selected and becomes the
    /// target; anything else is empty space. Where the press landed, in
    /// canvas units, is kept too — a paste lands there.
    pub(crate) fn context_press(&mut self) -> bool {
        let (cx, cy) = (self.cursor_px.0 as f32, self.cursor_px.1 as f32);
        let Some(layout) = self.layout() else {
            return false;
        };
        let in_viewport = layout.viewport.contains(cx, cy);
        let opens = in_viewport || layout.left.contains(cx, cy) || layout.right.contains(cx, cy);
        if !opens {
            return false;
        }
        self.ctx_target = Target::Empty;
        if in_viewport
            && self.editor.tool() == Tool::Select
            && let Some(id) = self.editor.id_under_cursor()
        {
            self.editor.select_under_cursor();
            self.ctx_target = Target::Object(id);
        }
        self.ctx_at = self.editor.cursor();
        self.ctx_menu = Some([cx, cy]);
        self.ctx_hover = None;
        self.ctx_over = None;
        self.ctx_drag = None;
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

    /// The panel body's page for the current tool, target and state.
    fn context_page(&self, panel: Viewport) -> Page {
        let scale = self.scale();
        let tool = self.editor.tool();
        match tool_title(tool) {
            Some(title) => Page::tool(
                panel,
                scale,
                tool,
                title,
                self.editor.defaults.get(tool),
                self.editor.canvas(),
            ),
            None => Page::home(
                panel,
                scale,
                &home::title(self.ctx_target, &self.editor),
                &home::rows(self.ctx_target, &self.editor),
            ),
        }
    }

    /// Everything the frame draws for the menu: the panel with its rail,
    /// the page's rects, and the page's words for the text pass.
    pub(crate) fn context_frame(&self) -> Option<(Vec<UiRect>, Vec<Label>)> {
        let ctx = self.context()?;
        let scale = self.scale();
        let mut rects = rail_rects(&ctx, scale, self.editor.tool(), self.ctx_hover);
        let page = self.context_page(ctx.panel);
        rects.extend(page.rects(self.ctx_over, self.ctx_drag));
        Some((rects, page.labels(self.ctx_over, self.ctx_drag)))
    }

    /// A left press while the menu is up. A rail button toggles its tool
    /// — select it, or unselect the active one back to Move — and the
    /// menu stays open; a press on the panel body goes to the page: a
    /// slider jumps to the cursor and follows it, a switch flips, a verb
    /// acts and closes; anywhere else closes it. Always true: an open
    /// menu owns the click.
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
            self.ctx_over = None;
            self.ctx_drag = None;
            self.request_redraw();
            return true;
        }
        if !ctx.panel.contains(cx, cy) {
            self.context_close();
            return true;
        }
        let page = self.context_page(ctx.panel);
        match page.hit(cx, cy) {
            Some(Hit::Slider(k)) => {
                if let Some(slot) = page.sliders.get(k) {
                    self.context_slider_to(k, Slider::t_at(slot.track, cx));
                    self.ctx_drag = Some(Drag::Slider(k));
                }
            }
            Some(Hit::Segment(i)) => {
                let tool = self.editor.tool();
                if let Some((sw, _, _)) = &page.switch {
                    // Fill has no thickness to move; that slider dims.
                    sw.pick(self.editor.defaults.get_mut(tool), i);
                }
            }
            Some(Hit::Verb(i)) => {
                if let Some(v) = page.verbs.get(i) {
                    let (verb, at) = (v.row.verb, self.ctx_at);
                    self.context_close();
                    self.context_verb(verb, at);
                }
            }
            // The panel's own real estate: not a dismissal.
            None => {}
        }
        self.request_redraw();
        true
    }

    /// The cursor moved with the menu up: a held slider follows it,
    /// otherwise what is under the cursor lights. True when the frame
    /// needs redrawing.
    pub(crate) fn context_moved(&mut self, mx: f32, my: f32) -> bool {
        if self.ctx_menu.is_none() {
            return false;
        }
        let Some(ctx) = self.context() else {
            return false;
        };
        if let Some(Drag::Slider(k)) = self.ctx_drag {
            let page = self.context_page(ctx.panel);
            if let Some(slot) = page.sliders.get(k) {
                self.context_slider_to(k, Slider::t_at(slot.track, mx));
            }
            return true;
        }
        // Which rail button and which page widget are under the cursor —
        // the same geometry the frame draws, so nothing lights where it
        // isn't clickable.
        let rail = ctx.rail.iter().position(|(_, _, b)| b.contains(mx, my));
        let over = if ctx.panel.contains(mx, my) {
            self.context_page(ctx.panel).hit(mx, my)
        } else {
            None
        };
        let dirty = rail != self.ctx_hover || over != self.ctx_over;
        self.ctx_hover = rail;
        self.ctx_over = over;
        dirty
    }

    /// The button came up: a slider drag ends, the menu stays. True when
    /// a drag was spent — the release is the menu's.
    pub(crate) fn context_release(&mut self) -> bool {
        self.ctx_drag.take().is_some()
    }

    /// The wheel over an open menu: a notch on a slider steps it;
    /// anywhere else on the panel or rail is swallowed, so a stray notch
    /// never zooms the canvas underneath. False when the menu isn't
    /// there.
    pub(crate) fn context_wheel(&mut self, cx: f32, cy: f32, dy: f32) -> bool {
        if self.ctx_menu.is_none() {
            return false;
        }
        let Some(ctx) = self.context() else {
            return false;
        };
        let on_rail = ctx.rail.iter().any(|(_, _, b)| b.contains(cx, cy));
        if !ctx.panel.contains(cx, cy) && !on_rail {
            return false;
        }
        let page = self.context_page(ctx.panel);
        if let Some(Hit::Slider(k)) = page.hit(cx, cy)
            && let Some(slot) = page.sliders.get(k)
        {
            let v = slider_step(slot.v, dy, self.modifiers.shift_key());
            self.context_slider_to(k, v);
        }
        true
    }

    /// Close the menu if it is open; true when the gesture was spent
    /// doing it — the app menus' rule: an open menu owns the click.
    pub(crate) fn context_close(&mut self) -> bool {
        if self.ctx_menu.take().is_some() {
            self.ctx_hover = None;
            self.ctx_over = None;
            self.ctx_drag = None;
            self.request_redraw();
            true
        } else {
            false
        }
    }

    /// Move slider `slot` of the current tool's page to normalized `v`.
    fn context_slider_to(&mut self, slot: usize, v: f32) {
        let tool = self.editor.tool();
        let Some(spec) = crate::defaults::sliders(tool).get(slot) else {
            return;
        };
        let canvas = self.editor.canvas();
        let (lo, hi) = crate::props::range(spec.prop, canvas);
        self.editor
            .defaults
            .get_mut(tool)
            .set(spec.prop, lo + v.clamp(0.0, 1.0) * (hi - lo), canvas);
    }
}
