//! The material playground: the editor's own look, editable from inside it.
//!
//! Spark's chrome kept being restyled by the one participant who can't see
//! the screen, so every attempt cost a build-look-describe-revert round trip
//! and twice ended in a revert. This hands the controls over.
//!
//! It lives in the **bottom panel** — full window width, and already
//! resizable by dragging its top edge. The first version squeezed into the
//! left panel and there was nowhere near enough room: the sliders ran off
//! the edge and half the controls needed scrolling.
//!
//! Two tabs, because they answer different questions:
//!
//! - **Colors** — every shade the editor draws with, by the name of the
//!   thing you see rather than the field in the code. Click a swatch and
//!   type a hex code. This is the one that was missing entirely in v1.
//! - **Depth** — the per-material knobs: rounding, borders, edge light,
//!   shadows.
//!
//! Rules it lives by: it never styles *itself* from the values it edits, so
//! a color tuned into oblivion can't take the panel that would undo it down
//! too; and every control is one number or one color, so nothing here needs
//! explaining twice.

mod draw;
pub(super) mod input;
mod recipe;
mod slots;

pub use draw::{labels, rects};
pub use recipe::recipe;
pub use slots::{KNOBS, Knob, MATERIALS, SLOTS, format_value, get, set};

use spark_render::Viewport;
use spark_ui::{Surface, Surfaces, hex_of, surfaces, theme};

/// Label size inside the panel — Alva reads from a distance.
pub const TEXT: f32 = 20.0;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Tab {
    #[default]
    Colors,
    Depth,
}

pub const TABS: [(Tab, &str); 2] = [(Tab::Colors, "Colors"), (Tab::Depth, "Depth")];

/// What a typed hex code is being typed *into*. Two tabs now carry colour
/// fields and they address different things: a palette entry belongs to the
/// theme, a gradient's far end belongs to one material.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Edit {
    /// A palette colour, by index into [`SLOTS`].
    Slot(usize),
    /// The far end of the picked material's gradient.
    GradEnd,
}

/// The material at position `i` in [`MATERIALS`]. The two stay in step by
/// hand; a test walks every index and checks the pair agrees.
fn nth(m: &Surfaces, i: usize) -> Surface {
    match i {
        0 => m.panel,
        1 => m.bar,
        2 => m.timeline,
        3 => m.status,
        4 => m.card,
        5 => m.card_inner,
        6 => m.fx_card,
        7 => m.header,
        8 => m.plate,
        9 => m.float,
        10 => m.well,
        11 => m.field,
        _ => m.hover,
    }
}

fn nth_mut(m: &mut Surfaces, i: usize) -> &mut Surface {
    match i {
        0 => &mut m.panel,
        1 => &mut m.bar,
        2 => &mut m.timeline,
        3 => &mut m.status,
        4 => &mut m.card,
        5 => &mut m.card_inner,
        6 => &mut m.fx_card,
        7 => &mut m.header,
        8 => &mut m.plate,
        9 => &mut m.float,
        10 => &mut m.well,
        11 => &mut m.field,
        _ => &mut m.hover,
    }
}

/// The colour an edit target currently reads.
///
/// The right panel's picker paints through this, so the playground never
/// grew a second colour picker of its own — Spark already had a good one,
/// and two would have drifted apart.
pub fn color_of(edit: Edit, pick: usize) -> [f32; 4] {
    match edit {
        Edit::Slot(i) => (SLOTS[i].get)(&theme()),
        Edit::GradEnd => nth(&surfaces(), pick).fill_to,
    }
}

/// Write a colour to an edit target. The single place a playground colour
/// changes, whether it came from a typed code or from the picker.
pub fn set_color(edit: Edit, pick: usize, color: [f32; 4]) {
    match edit {
        Edit::Slot(i) => {
            // `set_theme` rederives every material from the palette, which
            // is what carries a recolour into the borders — but it would
            // also throw away any depth already dialled in on the other
            // tab. Keep it.
            let mut t = theme();
            (SLOTS[i].set)(&mut t, color);
            let depth = surfaces();
            spark_ui::set_theme(t);
            input::carry_depth(&depth);
        }
        Edit::GradEnd => {
            let mut live = surfaces();
            nth_mut(&mut live, pick).fill_to = color;
            spark_ui::set_surfaces(live);
        }
    }
}

/// What the picker says it is painting. Never left to be guessed at: the
/// same square paints a shape one moment and the side panels the next.
pub fn label_of(edit: Edit, pick: usize) -> String {
    match edit {
        Edit::Slot(i) => SLOTS[i].label.to_string(),
        Edit::GradEnd => format!("{} fades to", MATERIALS[pick].0),
    }
}

/// One color in the grid: its swatch, its name, and the code it reads as.
pub struct Cell {
    /// What typing here changes.
    pub edit: Edit,
    /// What it is called on screen. Carried on the cell rather than looked
    /// up, because the two tabs' cells come from different tables.
    pub name: &'static str,
    pub rect: Viewport,
    pub swatch: Viewport,
    /// The box the code sits in. A bare string of hex digits gave no sign
    /// it could be typed into, which is exactly how a whole tab of live
    /// controls read as a list of colours nobody could do anything with.
    pub field: Viewport,
    pub color: [f32; 4],
    pub hex: String,
    pub label_pos: [f32; 2],
    pub hex_pos: [f32; 2],
    /// How wide the name may run before it hits the code.
    pub label_w: f32,
    /// Set while this cell is being typed into.
    pub editing: bool,
}

/// A knob row: its label, the track it rides, and where the text goes.
pub struct Row {
    pub knob: Knob,
    pub label: &'static str,
    pub track: Viewport,
    pub t: f32,
    pub value: String,
    pub label_pos: [f32; 2],
}

/// A group heading in either grid.
pub struct Head {
    pub label: &'static str,
    pub pos: [f32; 2],
}

pub struct Panel {
    pub tabs: [Viewport; 2],
    pub tab: Tab,
    pub heads: Vec<Head>,
    /// Colors tab.
    pub cells: Vec<Cell>,
    /// Depth tab: one row per material, then the knobs.
    pub picks: Vec<Viewport>,
    pub pick_labels: Vec<[f32; 2]>,
    pub rows: Vec<Row>,
    pub print: Viewport,
    pub reset: Viewport,
}

/// What the panel needs to know about the session to lay itself out.
pub struct State {
    pub tab: Tab,
    pub pick: usize,
    /// What is being typed into, and the buffer so far.
    pub editing: Option<(Edit, String)>,
}

/// Lay the panel out across `area` — the whole bottom panel.
pub fn build(area: Viewport, scale: f32, st: &State) -> Panel {
    let pad = 12.0 * scale;
    let line = spark_text::Text::line_height(TEXT * scale);

    // A strip along the top: the two tabs on the left, the two buttons on
    // the right. Everything else scrolls under it.
    let strip_h = line + pad * 2.0;
    let tab_w = 140.0 * scale;
    let tabs = [
        Viewport {
            x: area.x + pad,
            y: area.y + pad * 0.5,
            w: tab_w,
            h: strip_h - pad,
        },
        Viewport {
            x: area.x + pad + tab_w + 8.0 * scale,
            y: area.y + pad * 0.5,
            w: tab_w,
            h: strip_h - pad,
        },
    ];
    let btn_w = 130.0 * scale;
    let print = Viewport {
        x: area.x + area.w - pad - btn_w * 2.0 - 8.0 * scale,
        y: tabs[0].y,
        w: btn_w,
        h: tabs[0].h,
    };
    let reset = Viewport {
        x: area.x + area.w - pad - btn_w,
        y: tabs[0].y,
        w: btn_w,
        h: tabs[0].h,
    };

    let body = Viewport {
        x: area.x + pad,
        y: area.y + strip_h,
        w: (area.w - pad * 2.0).max(1.0),
        h: (area.h - strip_h - pad).max(1.0),
    };
    let mut p = Panel {
        tabs,
        tab: st.tab,
        heads: Vec::new(),
        cells: Vec::new(),
        picks: Vec::new(),
        pick_labels: Vec::new(),
        rows: Vec::new(),
        print,
        reset,
    };
    match st.tab {
        Tab::Colors => colors_grid(&mut p, body, scale, line, st),
        Tab::Depth => depth_grid(&mut p, body, scale, line, st),
    }
    p
}

/// Fill `body` column by column, wrapping when a column runs out of height.
/// Cell width shrinks to fit however many columns that takes, so the grid
/// always lands inside the panel instead of running off the end of it.
struct Flow {
    x: f32,
    y: f32,
    top: f32,
    bottom: f32,
    col_w: f32,
    row_h: f32,
}

impl Flow {
    /// `items` is the sequence about to be laid out, `true` for a heading.
    /// The column count is found by *simulating* that exact sequence rather
    /// than dividing by rows-per-column: a heading that would be orphaned at
    /// the foot of a column pushes an early wrap, so the naive count came up
    /// short and the last column ran off the end of the panel.
    fn new(body: Viewport, items: &[bool], row_h: f32, max_col_w: f32) -> Self {
        let mut y = 0.0f32;
        let mut cols = 1usize;
        for &is_head in items {
            if is_head && y > 0.0 && y + row_h * 2.0 > body.h {
                cols += 1;
                y = 0.0;
            }
            if y + row_h > body.h {
                cols += 1;
                y = 0.0;
            }
            y += row_h;
        }
        let col_w = (body.w / cols as f32).min(max_col_w);
        Self {
            x: body.x,
            y: body.y,
            top: body.y,
            bottom: body.y + body.h,
            col_w,
            row_h,
        }
    }

    /// Reserve the next row, wrapping to a new column at the bottom.
    fn next(&mut self) -> Viewport {
        if self.y + self.row_h > self.bottom {
            self.y = self.top;
            self.x += self.col_w;
        }
        let v = Viewport {
            x: self.x,
            y: self.y,
            w: self.col_w,
            h: self.row_h,
        };
        self.y += self.row_h;
        v
    }

    /// Keep a heading and its first entry in the same column.
    fn keep_together(&mut self, rows: f32) {
        if self.y > self.top && self.y + self.row_h * rows > self.bottom {
            self.y = self.top;
            self.x += self.col_w;
        }
    }
}

fn colors_grid(p: &mut Panel, body: Viewport, scale: f32, line: f32, st: &State) {
    let row_h = line + 14.0 * scale;
    let mut plan = Vec::with_capacity(SLOTS.len() * 2);
    let mut seen = "";
    for s in SLOTS {
        if s.group != seen {
            seen = s.group;
            plan.push(true);
        }
        plan.push(false);
    }
    // 340 was starving the labels — "Toggle, active half" ran off the end
    // of its own cell — while a 4K bottom panel had over a thousand logical
    // px per column going spare. The cap only ever binds when there is room
    // to spare; a narrow window still divides the width evenly.
    let mut flow = Flow::new(body, &plan, row_h, 620.0 * scale);
    let t = theme();
    let mut group = "";
    for (i, s) in SLOTS.iter().enumerate() {
        if s.group != group {
            group = s.group;
            flow.keep_together(2.0);
            let v = flow.next();
            p.heads.push(Head {
                label: s.group,
                pos: [v.x, v.y + (v.h - line) * 0.5],
            });
        }
        let v = flow.next();
        p.cells.push(cell(
            v,
            Edit::Slot(i),
            s.label,
            (s.get)(&t),
            st,
            scale,
            line,
        ));
    }
}

/// One colour row: swatch, name, and the code in a box you can type into.
///
/// A **gutter** on the right is not decoration. Without it the code sat
/// flush against the next column's swatch and read as that swatch's value.
fn cell(
    v: Viewport,
    edit: Edit,
    name: &'static str,
    color: [f32; 4],
    st: &State,
    scale: f32,
    line: f32,
) -> Cell {
    let editing = matches!(&st.editing, Some((e, _)) if *e == edit);
    // While it is being typed into the field shows the buffer, so a
    // half-typed code reads as what you typed rather than as the colour it
    // hasn't become yet.
    let hex = match &st.editing {
        Some((e, buf)) if *e == edit => buf.clone(),
        _ => hex_of(color),
    };
    let gutter = 20.0 * scale;
    let sw = v.h * 0.68;
    let fw = 150.0 * scale;
    let fh = line + 10.0 * scale;
    let field = Viewport {
        x: v.x + v.w - gutter - fw,
        y: v.y + (v.h - fh) * 0.5,
        w: fw,
        h: fh,
    };
    let label_x = v.x + sw + 14.0 * scale;
    Cell {
        edit,
        name,
        rect: v,
        swatch: Viewport {
            x: v.x,
            y: v.y + (v.h - sw) * 0.5,
            w: sw,
            h: sw,
        },
        field,
        color,
        hex,
        label_pos: [label_x, v.y + (v.h - line) * 0.5],
        hex_pos: [field.x + 12.0 * scale, field.y + (field.h - line) * 0.5],
        label_w: (field.x - label_x - 10.0 * scale).max(20.0),
        editing,
    }
}

/// The heading the end-colour field files under, named once so the table
/// and the layout can't drift apart.
const GRADIENT: &str = "Gradient";

fn depth_grid(p: &mut Panel, body: Viewport, scale: f32, line: f32, st: &State) {
    // Left column: which material. Full names, one per row, always visible
    // so it never scrolls away from the knob it belongs to.
    // The list wraps into as many columns as the panel's height needs.
    // One column was fine for seven materials and overflowed the moment the
    // window regions joined them — and a control that has run off the
    // bottom of the panel is a control that does not exist.
    let gap = 4.0 * scale;
    let row = line + 16.0 * scale;
    let per_col = (((body.h + gap) / (row + gap)).floor() as usize).max(1);
    let cols = MATERIALS.len().div_ceil(per_col);
    // Never more than half the panel: the knobs have to stay reachable too.
    let col_w = (300.0 * scale).min(body.w * 0.5 / cols as f32);
    let pick_w = col_w * cols as f32;
    for i in 0..MATERIALS.len() {
        let v = Viewport {
            x: body.x + (i / per_col) as f32 * col_w,
            y: body.y + (i % per_col) as f32 * (row + gap),
            w: col_w - 8.0 * scale,
            h: row,
        };
        p.pick_labels
            .push([v.x + 12.0 * scale, v.y + (v.h - line) * 0.5]);
        p.picks.push(v);
    }

    // The knobs flow through whatever is left.
    let rest = Viewport {
        x: body.x + pick_w + 20.0 * scale,
        y: body.y,
        w: (body.w - pick_w - 20.0 * scale).max(1.0),
        h: body.h,
    };
    let row_h = line + 30.0 * scale;
    let mut plan = Vec::with_capacity(KNOBS.len() * 2);
    let mut seen = "";
    for (_, head, ..) in KNOBS {
        if head != seen {
            seen = head;
            plan.push(true);
            // The Gradient heading is followed by its end-colour field
            // before any of its knobs, so the column simulation has to
            // count that row too or the last column runs off the panel.
            if head == GRADIENT {
                plan.push(false);
            }
        }
        plan.push(false);
    }
    let mut flow = Flow::new(rest, &plan, row_h, 520.0 * scale);
    let live = nth(&surfaces(), st.pick);
    let mut group = "";
    for (knob, head, label, max) in KNOBS {
        if head != group {
            group = head;
            flow.keep_together(2.0);
            let v = flow.next();
            p.heads.push(Head {
                label: head,
                pos: [v.x, v.y + (v.h - line) * 0.5],
            });
            if head == GRADIENT {
                // The far colour itself. "Darken" below it derives this
                // from the fill; typing here overrides it, which is the
                // only way to a gradient that tints or lightens rather
                // than only fading toward black.
                let v = flow.next();
                p.cells.push(cell(
                    v,
                    Edit::GradEnd,
                    // Not "End color": there is no start colour to pair it
                    // with — the surface's own fill is where it begins — and
                    // the pair reads as a span, which the two knobs below
                    // now genuinely are.
                    "Fade to",
                    live.fill_to,
                    st,
                    scale,
                    line,
                ));
            }
        }
        let v = flow.next();
        let value = get(&live, knob);
        p.rows.push(Row {
            knob,
            label,
            track: Viewport {
                x: v.x,
                y: v.y + line + 4.0 * scale,
                w: (v.w - 16.0 * scale).max(1.0),
                h: 10.0 * scale,
            },
            t: (value / max).clamp(0.0, 1.0),
            value: format_value(knob, value),
            label_pos: [v.x, v.y],
        });
    }
}

#[cfg(test)]
pub mod tests;
