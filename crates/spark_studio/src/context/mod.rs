//! The context menu: right-click opens a floating panel at the cursor —
//! **the tool home** (Alva's spec, 2026-08-31). A rail of shape tools
//! rides the panel's left flank; the body is the armed tool's
//! **draw-defaults page** — what the shape will look like the moment it
//! is drawn, configurable *before* drawing, which nothing in Spark
//! allowed before — or, with Move armed, **Home**: the verbs for what is
//! selected. A right-click on a shape selects it first, so the menu
//! opens on the thing you pointed at.
//!
//! A tool click selects that tool and the panel becomes its page;
//! clicking the active tool again unselects back to Move and Home. Tool
//! clicks, knob drags and colour picks never close the menu; a verb acts
//! and closes it; a click anywhere else closes it, swallowed either way —
//! the app menus' rule.
//!
//! Knobs are Lantern Mix's dial (`spark_ui::knob`), on its interaction:
//! drag up to turn up, [`DRAG_PX`] of travel for the whole range, Shift
//! for fine, the wheel steps. The colour is the one current colour every
//! tool draws with — palette chips and an HSV picker on every page.
//!
//! It draws in the overlay submit beside the app menus — one `UiPass`
//! owns one instance buffer, so a floating panel must be a second full
//! stack or the words underneath print straight through it.

mod home;
mod page;
#[cfg(test)]
mod tests;

pub use page::{Align, Hit, Label, Page};

use spark_render::Viewport;
use spark_ui::picker::{hsv_to_rgb, linear_to_srgb, rgb_to_hsv, srgb_to_linear};
use spark_ui::{
    ICON_ARROW, ICON_CIRCLE, ICON_LINE, ICON_PENTAGON, ICON_SQUARE, ICON_STARS, UiRect, knob,
    surfaces, theme,
};

use crate::Studio;
use crate::editor::Tool;
use crate::props::PALETTE;

/// Panel body size, logical px — sized for the tallest page (the star
/// field's six knobs); shorter pages give the air to the colour picker.
pub const PANEL_W: f32 = 420.0;
pub const PANEL_H: f32 = 680.0;
/// Air between the rail and the panel's left edge.
const RAIL_GAP: f32 = 12.0;
/// Air between rail buttons.
const BTN_GAP: f32 = 10.0;
/// A knob's full range is this much vertical drag, logical px — Lantern
/// Mix's feel, carried over with the dial.
pub const DRAG_PX: f32 = knob::DRAG_PX;

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

/// A drag on the page: a knob (where it started, and the value it held
/// then — the drag is relative, so the cap never jumps under the cursor),
/// or the colour picker's square or hue bar.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Drag {
    Knob {
        slot: usize,
        start_y: f32,
        start_v: f32,
    },
    Sv,
    Hue,
}

/// Where a knob drag has turned to: `dy` physical px down from the press
/// (up is negative and turns *up*), `fine` a tenth of the travel.
pub fn knob_drag(start_v: f32, dy: f32, scale: f32, fine: bool) -> f32 {
    let k = if fine { 0.1 } else { 1.0 };
    (start_v - dy / (DRAG_PX * scale) * k).clamp(0.0, 1.0)
}

/// A wheel notch on a knob: a fiftieth of the range, a fine two-hundredth.
pub fn knob_step(v: f32, notches: f32, fine: bool) -> f32 {
    let step = if fine { 0.005 } else { 0.02 };
    (v + notches * step).clamp(0.0, 1.0)
}

/// The picker's HSV for a colour, which every shape holds in linear light
/// and the picker speaks in display space.
pub fn hsv_of(rgb: [f32; 3]) -> [f32; 3] {
    rgb_to_hsv([
        linear_to_srgb(rgb[0]),
        linear_to_srgb(rgb[1]),
        linear_to_srgb(rgb[2]),
    ])
}

/// The colour a picker position means, back in linear light.
pub fn rgb_of(hsv: [f32; 3]) -> [f32; 3] {
    let s = hsv_to_rgb(hsv[0], hsv[1], hsv[2]);
    [
        srgb_to_linear(s[0]),
        srgb_to_linear(s[1]),
        srgb_to_linear(s[2]),
    ]
}

/// Whether two colours are the same to the eye — the picker's HSV keeps
/// its hue through a grey only while it is the one that wrote the colour.
fn same_colour(a: [f32; 3], b: [f32; 3]) -> bool {
    a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-4)
}

impl Studio {
    /// A right press lands: open the menu at the cursor if this is a
    /// region the menu owns — the viewport (comp viewer; the fly view's
    /// right button pans) and the empty side panels. The timeline's
    /// right-clicks keep their jobs (delete key, clear loop). Over the
    /// canvas with Move armed, whatever is under the cursor is selected
    /// first, so Home opens on the thing you pointed at.
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
        if in_viewport && self.editor.tool() == Tool::Select {
            self.editor.select_under_cursor();
        }
        self.ctx_menu = Some([cx, cy]);
        self.ctx_hover = None;
        self.ctx_over = None;
        self.ctx_drag = None;
        self.ctx_fade = [0.0; crate::defaults::MAX_KNOBS];
        self.ctx_hsv = hsv_of(self.editor.color());
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

    /// The panel body's page for the current tool and state.
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
                self.editor.palette_match(),
                self.ctx_hsv,
            ),
            None => Page::home(panel, scale, &home::state(&self.editor)),
        }
    }

    /// Everything the frame draws for the menu: the panel with its rail,
    /// the page's rects, and the page's words for the text pass.
    pub(crate) fn context_frame(&self) -> Option<(Vec<UiRect>, Vec<Label>)> {
        let ctx = self.context()?;
        let scale = self.scale();
        let mut rects = rail_rects(&ctx, scale, self.editor.tool(), self.ctx_hover);
        let page = self.context_page(ctx.panel);
        rects.extend(page.rects(self.ctx_over, self.ctx_drag, &self.ctx_fade));
        Some((rects, page.labels(&self.ctx_fade)))
    }

    /// Step the knobs' hover crossfades toward where the cursor is — the
    /// readout fading in and the pointer retracting, Lantern Mix's
    /// quarter-per-frame ease. Also re-reads the picker's HSV from the
    /// colour whenever something else moved it (`C`, the eyedropper), so
    /// the two can't disagree. True while a fade is still moving, which
    /// the frame answers with another frame.
    pub(crate) fn context_animate(&mut self) -> bool {
        if self.ctx_menu.is_none() {
            return false;
        }
        if !same_colour(rgb_of(self.ctx_hsv), self.editor.color()) {
            self.ctx_hsv = hsv_of(self.editor.color());
        }
        let mut moving = false;
        for (k, f) in self.ctx_fade.iter_mut().enumerate() {
            let engaged = matches!(self.ctx_over, Some(Hit::Knob(o)) if o == k)
                || matches!(self.ctx_drag, Some(Drag::Knob { slot, .. }) if slot == k);
            let target = if engaged { 1.0 } else { 0.0 };
            *f += (target - *f) * 0.25;
            if (*f - target).abs() < 0.004 {
                *f = target;
            } else {
                moving = true;
            }
        }
        moving
    }

    /// A left press while the menu is up. A rail button toggles its tool
    /// — select it, or unselect the active one back to Move — and the
    /// menu stays open; a press on the panel body goes to the page: a
    /// knob starts turning, a switch flips, a chip or the picker picks the
    /// colour, a verb acts and closes; anywhere else closes it. Always
    /// true: an open menu owns the click.
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
            Some(Hit::Knob(k)) => {
                if let Some(slot) = page.knobs.get(k) {
                    self.ctx_drag = Some(Drag::Knob {
                        slot: k,
                        start_y: cy,
                        start_v: slot.v,
                    });
                }
            }
            Some(Hit::Segment(i)) => {
                let tool = self.editor.tool();
                if let Some((sw, _, _)) = &page.switch {
                    // Fill has no thickness to turn; that knob dims.
                    sw.pick(self.editor.defaults.get_mut(tool), i);
                }
            }
            Some(Hit::Chip(i)) => {
                if let Some(&rgb) = PALETTE.get(i) {
                    self.editor.load_color(rgb);
                    self.ctx_hsv = hsv_of(rgb);
                }
            }
            Some(Hit::Sv) => {
                if let Some((p, _)) = &page.picker {
                    let (s, v) = p.sv_at(cx, cy);
                    self.context_set_hsv([self.ctx_hsv[0], s, v]);
                    self.ctx_drag = Some(Drag::Sv);
                }
            }
            Some(Hit::Hue) => {
                if let Some((p, _)) = &page.picker {
                    let h = p.hue_at(cy);
                    self.context_set_hsv([h, self.ctx_hsv[1], self.ctx_hsv[2]]);
                    self.ctx_drag = Some(Drag::Hue);
                }
            }
            Some(Hit::Verb(i)) => {
                if let Some(row) = page.verbs.get(i) {
                    let verb = row.verb;
                    self.context_close();
                    self.context_verb(verb);
                }
            }
            // The panel's own real estate: not a dismissal.
            None => {}
        }
        self.request_redraw();
        true
    }

    /// The cursor moved with the menu up: turn a held knob or drag the
    /// picker, otherwise track what is under the cursor. True when the
    /// frame needs redrawing.
    pub(crate) fn context_moved(&mut self, mx: f32, my: f32) -> bool {
        if self.ctx_menu.is_none() {
            return false;
        }
        let Some(ctx) = self.context() else {
            return false;
        };
        match self.ctx_drag {
            Some(Drag::Knob {
                slot,
                start_y,
                start_v,
            }) => {
                let v = knob_drag(
                    start_v,
                    my - start_y,
                    self.scale(),
                    self.modifiers.shift_key(),
                );
                self.context_knob_to(slot, v);
                return true;
            }
            Some(Drag::Sv) => {
                let page = self.context_page(ctx.panel);
                if let Some((p, _)) = &page.picker {
                    let (s, v) = p.sv_at(mx, my);
                    self.context_set_hsv([self.ctx_hsv[0], s, v]);
                }
                return true;
            }
            Some(Drag::Hue) => {
                let page = self.context_page(ctx.panel);
                if let Some((p, _)) = &page.picker {
                    let h = p.hue_at(my);
                    self.context_set_hsv([h, self.ctx_hsv[1], self.ctx_hsv[2]]);
                }
                return true;
            }
            None => {}
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

    /// The button came up: a knob or picker drag ends, the menu stays.
    /// True when a drag was spent — the release is the menu's.
    pub(crate) fn context_release(&mut self) -> bool {
        self.ctx_drag.take().is_some()
    }

    /// The wheel over an open menu: a notch on a knob steps it; anywhere
    /// else on the panel or rail is swallowed, so a stray notch never
    /// zooms the canvas underneath. False when the menu isn't there.
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
        if let Some(Hit::Knob(k)) = page.hit(cx, cy)
            && let Some(slot) = page.knobs.get(k)
        {
            let v = knob_step(slot.v, dy, self.modifiers.shift_key());
            self.context_knob_to(k, v);
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

    /// Turn knob `slot` of the current tool's page to normalized `v`.
    fn context_knob_to(&mut self, slot: usize, v: f32) {
        let tool = self.editor.tool();
        let Some(spec) = crate::defaults::knobs(tool).get(slot) else {
            return;
        };
        let canvas = self.editor.canvas();
        let (lo, hi) = crate::props::range(spec.prop, canvas);
        self.editor.defaults.get_mut(tool).set(
            spec.prop,
            lo + v.clamp(0.0, 1.0) * (hi - lo),
            canvas,
        );
    }

    /// The picker moved: the current colour follows, without painting the
    /// selection — a tool page sets what the *next* shape draws with.
    fn context_set_hsv(&mut self, hsv: [f32; 3]) {
        self.ctx_hsv = hsv;
        self.editor.load_color(rgb_of(hsv));
    }
}
