//! Printing the look back out as source.
//!
//! The playground edits a live skin; this is how a session's work leaves it.
//! Colours print as hex codes, and materials print as palette *expressions*
//! rather than literals wherever they can, so a baked recipe still follows a
//! later recolour instead of freezing today's palette into the source.

use std::fmt::Write as _;

use spark_ui::{Surfaces, TURN, Theme, hex_of};

use super::{MATERIALS, SLOTS, nth};

/// The constructor a colour has to be written with: six digits carry no
/// alpha, eight do, and `hex_of` already prints exactly one of the two.
fn ctor(c: [f32; 4]) -> &'static str {
    match c[3] >= 1.0 {
        true => "srgb",
        false => "srgba",
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
        let c = (slot.get)(t);
        let _ = writeln!(s, "// {:<24} {}(0x{})", slot.label, ctor(c), hex_of(c));
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
        if f.fill_to[3] > 0.0 {
            let _ = write!(
                s,
                "\n    .shade({}(0x{}))",
                ctor(f.fill_to),
                hex_of(f.fill_to)
            );
            if (f.grad[0] - TURN).abs() > 1e-4 {
                let _ = write!(s, "\n    .toward({:.3})", f.grad[0]);
            }
            if f.grad[1] > 0.5 {
                let _ = write!(s, "\n    .radial(true)");
            }
            if f.grad_span != [0.0, 1.0] {
                let _ = write!(
                    s,
                    "\n    .span({:.3}, {:.3})",
                    f.grad_span[0], f.grad_span[1]
                );
            }
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
