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
mod slots;

pub use draw::{labels, rects};
pub use slots::{KNOBS, Knob, MATERIALS, SLOTS, format_value, get, set, shade_of};

use std::fmt::Write as _;

use spark_render::Viewport;
use spark_ui::{Surface, Surfaces, Theme, hex_of, surfaces, theme};

/// Label size inside the panel — Alva reads from a distance.
pub const TEXT: f32 = 20.0;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Tab {
    #[default]
    Colors,
    Depth,
}

pub const TABS: [(Tab, &str); 2] = [(Tab::Colors, "Colors"), (Tab::Depth, "Depth")];

fn nth(m: &Surfaces, i: usize) -> Surface {
    match i {
        1 => m.header,
        2 => m.plate,
        3 => m.well,
        4 => m.float,
        5 => m.field,
        6 => m.hover,
        _ => m.card,
    }
}

fn nth_mut(m: &mut Surfaces, i: usize) -> &mut Surface {
    match i {
        1 => &mut m.header,
        2 => &mut m.plate,
        3 => &mut m.well,
        4 => &mut m.float,
        5 => &mut m.field,
        6 => &mut m.hover,
        _ => &mut m.card,
    }
}

/// One color in the grid: its swatch, its name, and the code it reads as.
pub struct Cell {
    pub slot: usize,
    pub rect: Viewport,
    pub swatch: Viewport,
    pub color: [f32; 4],
    pub hex: String,
    pub label_pos: [f32; 2],
    pub hex_pos: [f32; 2],
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
    /// The slot being typed into, and the buffer so far.
    pub editing: Option<(usize, String)>,
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
    let mut flow = Flow::new(body, &plan, row_h, 340.0 * scale);
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
        let sw = row_h * 0.62;
        let color = (s.get)(&t);
        let editing = matches!(&st.editing, Some((k, _)) if *k == i);
        let hex = match &st.editing {
            Some((k, buf)) if *k == i => buf.clone(),
            _ => hex_of(color),
        };
        p.cells.push(Cell {
            slot: i,
            rect: v,
            swatch: Viewport {
                x: v.x,
                y: v.y + (v.h - sw) * 0.5,
                w: sw,
                h: sw,
            },
            color,
            hex,
            label_pos: [v.x + sw + 10.0 * scale, v.y + (v.h - line) * 0.5],
            hex_pos: [v.x + v.w - 92.0 * scale, v.y + (v.h - line) * 0.5],
            editing,
        });
    }
}

fn depth_grid(p: &mut Panel, body: Viewport, scale: f32, line: f32, st: &State) {
    // Left column: which material. Full names, one per row, always visible
    // so it never scrolls away from the knob it belongs to.
    let pick_w = 260.0 * scale;
    let gap = 4.0 * scale;
    // Seven rows have to share the panel's height whatever it is, so they
    // shrink to fit rather than running off the bottom of it.
    let pick_h = ((body.h - gap * 6.0) / 7.0)
        .min(line + 16.0 * scale)
        .max(line);
    for (i, (name, ..)) in MATERIALS.iter().enumerate() {
        let _ = name;
        let v = Viewport {
            x: body.x,
            y: body.y + i as f32 * (pick_h + gap),
            w: pick_w,
            h: pick_h,
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
        }
        plan.push(false);
    }
    let mut flow = Flow::new(rest, &plan, row_h, 380.0 * scale);
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

/// Rebuild the printable recipe from the live palette and materials.
///
/// Colors print as hex codes in a `Theme` literal, and materials print as
/// palette *expressions* rather than literals, so a baked recipe still
/// follows a later recolor.
pub fn recipe(t: &Theme, m: &Surfaces) -> String {
    let mut s = String::from("// --- paste into theme.rs: default_theme() ---\n");
    let mut group = "";
    for slot in SLOTS {
        if slot.group != group {
            group = slot.group;
            let _ = write!(s, "\n// {group}\n");
        }
        let _ = writeln!(s, "// {:<24} 0x{}", slot.label, hex_of((slot.get)(t)));
    }
    s.push_str("\n// --- paste into surface.rs: Surfaces::from_theme() ---\n");
    for (i, (shown, field, fill, border)) in MATERIALS.iter().enumerate() {
        let f = nth(m, i);
        let _ = write!(
            s,
            "{field}: Surface::flat({fill}, {:.1}) // {shown}",
            f.radius
        );
        if f.border > 0.0 {
            let _ = write!(s, "\n    .edge({:.1}, {border})", f.border);
        }
        let shade = shade_of(&f);
        if shade > 0.0 {
            let _ = write!(s, "\n    .shade(darken({fill}, {shade:.2}))");
        }
        if f.bevel[0] > 0.0 || f.bevel[1] > 0.0 {
            let _ = write!(
                s,
                "\n    .lit({:.2}, {:.2}, {:.1})",
                f.bevel[0], f.bevel[1], f.bevel[2]
            );
        }
        if f.shadow[2] > 0.0 {
            let _ = write!(
                s,
                "\n    .raised({:.1}, {:.1}, {:.2})",
                f.shadow[0], f.shadow[1], f.shadow[2]
            );
        }
        if f.inner[2] > 0.0 {
            let _ = write!(
                s,
                "\n    .recessed({:.1}, {:.1}, {:.2})",
                f.inner[0], f.inner[1], f.inner[2]
            );
        }
        if f.grain > 0.0 {
            let _ = write!(s, "\n    .textured({:.3})", f.grain);
        }
        s.push_str(",\n");
    }
    s
}

#[cfg(test)]
mod tests;
