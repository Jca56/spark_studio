//! The material playground: a slider per knob of one [`Surface`], applied
//! live to the whole editor.
//!
//! Spark's look kept getting restyled by the wrong person — Claude can't see
//! the screen, so every attempt cost a build-look-describe-revert round trip
//! and the taste never survived it. This panel hands the knobs to Alva:
//! drag, watch the *real* cards and buttons and wells change on the next
//! frame, and when it looks right, print the recipe.
//!
//! Two rules this panel lives by:
//!
//! 1. **It never styles itself with the materials it edits.** It paints from
//!    plain theme colors, so cranking `card` to something unreadable can
//!    never make the panel that would undo it unreadable too.
//! 2. **It only edits numbers.** Colors already work; depth never existed.

use std::fmt::Write as _;

use spark_render::Viewport;
use spark_ui::{Surface, Surfaces, surfaces, theme};

pub use draw::{labels, rects};

/// Label size inside the panel — Alva reads from a distance.
pub const TEXT: f32 = 21.0;

/// The seven materials, in picker order, with the expression each one's
/// colors came from so a printed recipe still follows the palette.
pub const MATERIALS: [(&str, &str, &str); 7] = [
    ("card", "t.card", "t.card_border"),
    ("header", "t.header", "t.card_border"),
    ("plate", "t.card", "t.plate_edge"),
    ("well", "t.well", "t.card_border"),
    ("float", "t.card", "t.seam"),
    ("field", "t.slider_track", "t.seam"),
    ("hover", "t.button_hover", "t.card_border"),
];

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

/// One tunable number on a surface.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Knob {
    Radius,
    Border,
    Shade,
    Grain,
    BevelTop,
    BevelBottom,
    BevelSize,
    ShadowDrop,
    ShadowBlur,
    ShadowDark,
    InnerDrop,
    InnerBlur,
    InnerDark,
}

/// Every knob, with its label and the top of its range (all start at 0).
/// Ranges are chosen so the useful settings sit in the middle of the track
/// rather than bunched against one end.
pub const KNOBS: [(Knob, &str, f32); 13] = [
    (Knob::Radius, "Radius", 30.0),
    (Knob::Border, "Border", 8.0),
    (Knob::Shade, "Shade", 1.0),
    (Knob::Grain, "Grain", 0.25),
    (Knob::BevelTop, "Bevel light", 1.0),
    (Knob::BevelBottom, "Bevel shade", 1.0),
    (Knob::BevelSize, "Bevel depth", 12.0),
    (Knob::ShadowDrop, "Shadow drop", 16.0),
    (Knob::ShadowBlur, "Shadow blur", 40.0),
    (Knob::ShadowDark, "Shadow dark", 1.0),
    (Knob::InnerDrop, "Inner drop", 16.0),
    (Knob::InnerBlur, "Inner blur", 40.0),
    (Knob::InnerDark, "Inner dark", 1.0),
];

pub fn get(s: &Surface, k: Knob) -> f32 {
    match k {
        Knob::Radius => s.radius,
        Knob::Border => s.border,
        Knob::Grain => s.grain,
        Knob::BevelTop => s.bevel[0],
        Knob::BevelBottom => s.bevel[1],
        Knob::BevelSize => s.bevel[2],
        Knob::ShadowDrop => s.shadow[0],
        Knob::ShadowBlur => s.shadow[1],
        Knob::ShadowDark => s.shadow[2],
        Knob::InnerDrop => s.inner[0],
        Knob::InnerBlur => s.inner[1],
        Knob::InnerDark => s.inner[2],
        // Shade isn't stored as a number — it's the distance the gradient's
        // far end sits below the fill. Read it back off the two colors.
        Knob::Shade => shade_of(s),
    }
}

pub fn set(s: &mut Surface, k: Knob, v: f32) {
    match k {
        Knob::Radius => s.radius = v,
        Knob::Border => s.border = v,
        Knob::Grain => s.grain = v,
        Knob::BevelTop => s.bevel[0] = v,
        Knob::BevelBottom => s.bevel[1] = v,
        Knob::BevelSize => s.bevel[2] = v,
        Knob::ShadowDrop => s.shadow[0] = v,
        Knob::ShadowBlur => s.shadow[1] = v,
        Knob::ShadowDark => s.shadow[2] = v,
        Knob::InnerDrop => s.inner[0] = v,
        Knob::InnerBlur => s.inner[1] = v,
        Knob::InnerDark => s.inner[2] = v,
        Knob::Shade => {
            s.fill_to = if v <= 0.005 {
                [0.0; 4]
            } else {
                spark_ui::darken(s.fill, v)
            }
        }
    }
}

fn shade_of(s: &Surface) -> f32 {
    if s.fill_to[3] <= 0.0 {
        return 0.0;
    }
    let hi = s.fill[..3].iter().copied().fold(0.0f32, f32::max).max(1e-4);
    let lo = s.fill_to[..3].iter().copied().fold(0.0f32, f32::max);
    ((1.0 - lo / hi) / spark_ui::SHADE_DEPTH).clamp(0.0, 1.0)
}

/// A knob row: its label, the track it rides, and where the text goes.
pub struct Row {
    pub knob: Knob,
    pub label: &'static str,
    pub track: Viewport,
    /// Position along the track, 0..1.
    pub t: f32,
    pub value: String,
    pub label_pos: [f32; 2],
}

pub struct Panel {
    /// One chip per material, painted *in* that material so the picker
    /// doubles as a preview of what it selects.
    pub chips: Vec<Viewport>,
    pub labels: Vec<[f32; 2]>,
    pub rows: Vec<Row>,
    pub print: Viewport,
    pub reset: Viewport,
    /// Full laid-out height, for scroll clamping.
    pub content_h: f32,
}

/// Lay the panel out inside `area`, scrolled down by `scroll` logical px.
pub fn build(area: Viewport, scale: f32, pick: usize, scroll: f32) -> Panel {
    let pad = 14.0 * scale;
    let line = spark_text::Text::line_height(TEXT * scale);
    let x = area.x + pad;
    let w = (area.w - pad * 2.0).max(1.0);
    // Lay out in a scroll-free height accumulator and subtract the scroll
    // only when placing something. `content_h` then falls out exactly
    // scroll-invariant, which the clamp in `render` depends on.
    let mut used = pad;
    let top = |used: f32| area.y - scroll + used;

    // Picker: seven chips, two to a row.
    let cols = 2.0;
    let gap = 8.0 * scale;
    let chip_w = (w - gap * (cols - 1.0)) / cols;
    let chip_h = 46.0 * scale;
    let chips_y = top(used);
    let mut chips = Vec::with_capacity(MATERIALS.len());
    let mut labels = Vec::with_capacity(MATERIALS.len());
    for i in 0..MATERIALS.len() {
        let v = Viewport {
            x: x + (i % 2) as f32 * (chip_w + gap),
            y: chips_y + (i / 2) as f32 * (chip_h + gap),
            w: chip_w,
            h: chip_h,
        };
        labels.push([v.x + 14.0 * scale, v.y + (v.h - line) * 0.5]);
        chips.push(v);
    }
    used += MATERIALS.len().div_ceil(2) as f32 * (chip_h + gap) + 12.0 * scale;

    // Knob rows: label and value on one line, a full-width track beneath.
    let live = nth(&surfaces(), pick);
    let track_h = 20.0 * scale;
    let mut rows = Vec::with_capacity(KNOBS.len());
    for (knob, label, max) in KNOBS {
        let value = get(&live, knob);
        let row_y = top(used);
        rows.push(Row {
            knob,
            label,
            track: Viewport {
                x,
                y: row_y + line + 5.0 * scale,
                w,
                h: track_h,
            },
            t: (value / max).clamp(0.0, 1.0),
            value: format_value(knob, value),
            label_pos: [x, row_y],
        });
        used += line + 5.0 * scale + track_h + 14.0 * scale;
    }

    used += 6.0 * scale;
    let btn_h = 52.0 * scale;
    let btn_w = (w - gap) * 0.5;
    let btn_y = top(used);
    let print = Viewport {
        x,
        y: btn_y,
        w: btn_w,
        h: btn_h,
    };
    let reset = Viewport {
        x: x + btn_w + gap,
        y: btn_y,
        w: btn_w,
        h: btn_h,
    };
    used += btn_h + pad;

    Panel {
        chips,
        labels,
        rows,
        print,
        reset,
        content_h: used,
    }
}

fn format_value(knob: Knob, v: f32) -> String {
    match knob {
        // The 0..1 knobs read as percentages; the rest are logical px.
        Knob::Shade
        | Knob::Grain
        | Knob::BevelTop
        | Knob::BevelBottom
        | Knob::ShadowDark
        | Knob::InnerDark => format!("{}%", (v * 100.0).round()),
        _ => format!("{v:.1}"),
    }
}

/// Rebuild the printable recipe from the live materials.
///
/// Colors are emitted as the palette expressions they came from rather than
/// as literals, so a printed recipe still recolors with the theme — the
/// whole point of naming colors by role.
pub fn recipe(m: &Surfaces) -> String {
    let mut s = String::from("    pub fn from_theme(t: &Theme) -> Self {\n        Self {\n");
    for (i, (name, fill, border)) in MATERIALS.iter().enumerate() {
        let f = nth(m, i);
        let _ = write!(
            s,
            "            {name}: Surface::flat({fill}, {:.1})",
            f.radius
        );
        if f.border > 0.0 && !border.is_empty() {
            let _ = write!(s, "\n                .edge({:.1}, {border})", f.border);
        }
        let shade = shade_of(&f);
        if shade > 0.0 {
            let _ = write!(s, "\n                .shade(darken({fill}, {shade:.2}))");
        }
        if f.bevel[0] > 0.0 || f.bevel[1] > 0.0 {
            let _ = write!(
                s,
                "\n                .lit({:.2}, {:.2}, {:.1})",
                f.bevel[0], f.bevel[1], f.bevel[2]
            );
        }
        if f.shadow[2] > 0.0 {
            let _ = write!(
                s,
                "\n                .raised({:.1}, {:.1}, {:.2})",
                f.shadow[0], f.shadow[1], f.shadow[2]
            );
        }
        if f.inner[2] > 0.0 {
            let _ = write!(
                s,
                "\n                .recessed({:.1}, {:.1}, {:.2})",
                f.inner[0], f.inner[1], f.inner[2]
            );
        }
        if f.grain > 0.0 {
            let _ = write!(s, "\n                .textured({:.3})", f.grain);
        }
        s.push_str(",\n");
    }
    s.push_str("        }\n    }\n");
    s
}

/// A track is a thin thing to hit, so its grab box is fattened vertically.
/// Nothing else lives between the rows, so this costs no precision.
fn on_track(track: Viewport, cx: f32, cy: f32, scale: f32) -> bool {
    let grow = 12.0 * scale;
    cx >= track.x
        && cx <= track.x + track.w
        && cy >= track.y - grow
        && cy <= track.y + track.h + grow
}

impl crate::Studio {
    fn material_panel(&self) -> Option<(Panel, f32)> {
        let layout = self.layout()?;
        let scale = self.scale();
        Some((
            build(
                layout.left,
                scale,
                self.material_pick,
                self.materials_scroll,
            ),
            scale,
        ))
    }

    pub(crate) fn press_materials(&mut self, cx: f32, cy: f32) {
        let Some((panel, scale)) = self.material_panel() else {
            return;
        };
        if let Some(i) = panel.chips.iter().position(|c| c.contains(cx, cy)) {
            self.material_pick = i;
            self.request_redraw();
            return;
        }
        if let Some(row) = panel.rows.iter().find(|r| on_track(r.track, cx, cy, scale)) {
            self.material_drag = Some(row.knob);
            self.drag_material(cx);
            self.request_redraw();
            return;
        }
        if panel.print.contains(cx, cy) {
            self.print_recipe();
        } else if panel.reset.contains(cx, cy) {
            spark_ui::set_surfaces(Surfaces::from_theme(&theme()));
            self.request_redraw();
        }
    }

    /// Move the knob under the cursor. Returns whether anything changed.
    pub(crate) fn drag_material(&mut self, cx: f32) -> bool {
        let Some(knob) = self.material_drag else {
            return false;
        };
        let Some((panel, _)) = self.material_panel() else {
            return false;
        };
        let Some(row) = panel.rows.iter().find(|r| r.knob == knob) else {
            self.material_drag = None;
            return false;
        };
        let Some(&(_, _, max)) = KNOBS.iter().find(|(k, _, _)| *k == knob) else {
            return false;
        };
        let t = ((cx - row.track.x) / row.track.w).clamp(0.0, 1.0);
        let mut live = surfaces();
        set(nth_mut(&mut live, self.material_pick), knob, t * max);
        spark_ui::set_surfaces(live);
        true
    }

    /// Write the recipe next to wherever Spark was launched from, and echo
    /// it. The file is the reliable half — launched from a desktop entry
    /// there may be no terminal to print to.
    fn print_recipe(&mut self) {
        let body = recipe(&surfaces());
        let path = std::path::Path::new(&self.current_file)
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .map(|p| p.to_path_buf())
            .or_else(|| std::env::current_dir().ok())
            .unwrap_or_default()
            .join("spark_materials.txt");
        match std::fs::write(&path, &body) {
            Ok(()) => println!("materials -> {}\n{body}", path.display()),
            Err(e) => println!(
                "materials: could not write {} ({e})\n{body}",
                path.display()
            ),
        }
        self.request_redraw();
    }
}

mod draw;

#[cfg(test)]
#[cfg(test)]
mod tests;
