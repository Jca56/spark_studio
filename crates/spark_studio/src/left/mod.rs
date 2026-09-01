//! The left panel: a tab strip — `Effects` today, a Browser and whatever
//! else later — over the active tab's page (Alva's spec, 2026-08-31).
//!
//! **Effects** is where effects come from: a card of rows, one per kind,
//! and you **drag a row onto an object** — a shape on the canvas or its
//! row on the timeline — to add it; the object is selected and its new
//! section appears in the inspector. A ghost of the row's name rides the
//! cursor while it's held. A click without a drag says so in the status
//! strip rather than doing something else.

mod effects;
#[cfg(test)]
mod tests;

pub use effects::{Hit, Page};

use spark_render::Viewport;
use spark_ui::{UiRect, surfaces, theme};

use crate::Studio;
use crate::arrange::{ArrHit, ClipRef, RowKind};
use crate::chrome::{Align, Label, UI_TEXT};
use crate::fx::EffectKind;

/// The panel's tabs, in strip order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tab {
    Effects,
}

impl Tab {
    pub const ALL: [Tab; 1] = [Tab::Effects];

    pub fn title(self) -> &'static str {
        match self {
            Tab::Effects => "Effects",
        }
    }
}

/// An effect row being dragged: which, where the press was, where the
/// cursor is, and whether it has travelled — a press that never does is
/// a click.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Drag {
    pub kind: EffectKind,
    pub from: [f32; 2],
    pub at: [f32; 2],
    pub moved: bool,
}

/// The left panel's own state, on the studio.
pub struct State {
    pub tab: Tab,
    pub over: Option<Hit>,
    pub drag: Option<Drag>,
}

impl State {
    pub fn new() -> Self {
        Self {
            tab: Tab::Effects,
            over: None,
            drag: None,
        }
    }
}

/// What the frame draws for the panel, and the drag's ghost, which
/// floats over everything.
pub struct Frame {
    pub rects: Vec<UiRect>,
    pub labels: Vec<Label>,
    pub ghost: Option<(Vec<UiRect>, Vec<Label>)>,
}

/// The ghost's size, logical px.
const GHOST_W: f32 = 200.0;
const GHOST_H: f32 = 44.0;

/// Cursor travel before a press becomes a drag, logical px.
const DRAG_START: f32 = 4.0;

impl Studio {
    fn left_page(&self, panel: Viewport) -> Page {
        Page::build(panel, self.scale(), self.left.tab)
    }

    /// Everything the frame draws for the left panel.
    pub(crate) fn left_frame(&self, panel: Viewport) -> Frame {
        let page = self.left_page(panel);
        let held = self.left.drag.filter(|d| d.moved).and_then(|d| {
            page.rows.iter().position(|r| r.kind == d.kind)
        });
        let ghost = self.left.drag.filter(|d| d.moved).map(|d| {
            let s = self.scale();
            let t = theme();
            let r = Viewport {
                x: d.at[0] + 16.0 * s,
                y: d.at[1] + 16.0 * s,
                w: GHOST_W * s,
                h: GHOST_H * s,
            };
            let size = UI_TEXT * s;
            (
                vec![surfaces().plate.edge(2.0, t.accent).rect(r, s)],
                vec![Label {
                    text: d.kind.label().to_string(),
                    size,
                    pos: [
                        r.x + r.w * 0.5,
                        r.y + (r.h - spark_text::Text::line_height(size)) * 0.5,
                    ],
                    color: t.text,
                    max_w: r.w,
                    align: Align::Center,
                }],
            )
        });
        Frame {
            rects: page.rects(self.left.over, held),
            labels: page.labels(),
            ghost,
        }
    }

    /// A left press on the panel: a tab picks itself, a row starts a
    /// drag. False on the panel's air, which stays the neutral surface
    /// it was (a click there drops the selection).
    pub(crate) fn left_press(&mut self, panel: Viewport, cx: f32, cy: f32) -> bool {
        let page = self.left_page(panel);
        match page.hit(cx, cy) {
            Some(Hit::Tab(i)) => {
                if let Some((tab, _)) = page.tabs.get(i) {
                    self.left.tab = *tab;
                }
                true
            }
            Some(Hit::Row(k)) => {
                if let Some(row) = page.rows.get(k) {
                    self.left.drag = Some(Drag {
                        kind: row.kind,
                        from: [cx, cy],
                        at: [cx, cy],
                        moved: false,
                    });
                }
                true
            }
            None => false,
        }
    }

    /// The cursor moved: a held row's ghost follows it; otherwise what
    /// is under the cursor lights. True when the frame needs redrawing.
    pub(crate) fn left_moved(&mut self, panel: Viewport, mx: f32, my: f32) -> bool {
        let start = DRAG_START * self.scale();
        if let Some(d) = &mut self.left.drag {
            d.at = [mx, my];
            let travel = ((mx - d.from[0]).powi(2) + (my - d.from[1]).powi(2)).sqrt();
            if travel >= start {
                d.moved = true;
            }
            return d.moved;
        }
        let over = if panel.contains(mx, my) {
            self.left_page(panel).hit(mx, my)
        } else {
            None
        };
        let dirty = over != self.left.over;
        self.left.over = over;
        dirty
    }

    /// The button came up with a row held: dropped on an object — under
    /// the cursor on the canvas, or its row on the timeline — the effect
    /// is added to it and it is selected, so the inspector shows the new
    /// section. Dropped on nothing, or never dragged, the status strip
    /// says what to do instead. True when a press on a row ended here.
    pub(crate) fn left_release(&mut self, cx: f32, cy: f32) -> bool {
        let Some(d) = self.left.drag.take() else {
            return false;
        };
        let name = d.kind.label();
        if !d.moved {
            self.export_note = Some(format!("drag {name} onto an object to add it"));
            return true;
        }
        let Some(layout) = self.layout() else {
            return true;
        };
        let scale = self.scale();
        let target: Option<usize> = if layout.viewport.contains(cx, cy) {
            self.editor
                .id_under_cursor()
                .and_then(|id| self.editor.index_of(id))
        } else if layout.timeline.contains(cx, cy) {
            let panel = crate::timeline::panel(layout.timeline, scale);
            let sc = self.arrange_scene(&panel, scale);
            match crate::arrange::hit(&sc, cx, cy, scale) {
                Some(ArrHit::Head(RowKind::Object(i))) | Some(ArrHit::Eye(RowKind::Object(i))) => {
                    Some(i)
                }
                Some(ArrHit::Clip(ClipRef::Obj { obj, .. }, _)) => self.editor.index_of(obj),
                _ => None,
            }
        } else {
            None
        };
        match target {
            Some(i) => {
                self.editor.select(Some(i));
                self.editor.add_effect_to(i, d.kind);
                self.export_note = Some(format!(
                    "added {name} to {}",
                    self.editor.display_name(i)
                ));
            }
            None => {
                self.export_note = Some(format!("{name} dropped on nothing — aim at an object"));
            }
        }
        true
    }
}
