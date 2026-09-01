//! The inspector's wheel and keyboard: the body's scroll, the keys a
//! field being typed into takes (digits, Enter, Esc, the caret), and
//! what committing a field means for each kind of field. Split from
//! `mod` so the press and move paths stay readable.

use super::*;

impl Studio {
    /// The wheel over the right panel scrolls the body.
    pub(crate) fn inspector_wheel(&mut self, panel: Viewport, dy: f32) -> bool {
        let max = self.inspector_page(panel).max_scroll();
        let next = (self.inspector.scroll - dy * 60.0 * self.scale()).clamp(0.0, max);
        if (next - self.inspector.scroll).abs() < 0.5 {
            return false;
        }
        self.inspector.scroll = next;
        true
    }

    /// Keys while a field is being typed into. Enter commits, Esc lets
    /// go, the rest edit the buffer — digits and a sign and point for a
    /// number, hex digits for the code. True when the frame needs
    /// redrawing; the caller keeps every key from the editor meanwhile.
    pub(crate) fn inspector_key(&mut self, key: &winit::keyboard::Key) -> bool {
        use winit::keyboard::{Key, NamedKey};
        let shift = self.modifiers.shift_key();
        let Some((what, tb)) = &mut self.inspector.edit else {
            return false;
        };
        let what = *what;
        match key {
            Key::Named(NamedKey::Enter) => self.inspector_commit(),
            Key::Named(NamedKey::Escape) => {
                self.inspector.edit = None;
                true
            }
            Key::Named(NamedKey::Backspace) => tb.backspace(),
            Key::Named(NamedKey::Delete) => tb.delete(),
            Key::Named(NamedKey::ArrowLeft) => {
                tb.step(false, shift);
                true
            }
            Key::Named(NamedKey::ArrowRight) => {
                tb.step(true, shift);
                true
            }
            Key::Named(NamedKey::Home) => {
                tb.home(shift);
                true
            }
            Key::Named(NamedKey::End) => {
                tb.end(shift);
                true
            }
            Key::Named(NamedKey::Space) => {
                // A name can have spaces; a number can't. (The transport
                // keeps Space when nothing is being typed.)
                if what == EditKey::Name {
                    tb.insert(' ');
                    true
                } else {
                    false
                }
            }
            Key::Character(s) => {
                let mut dirty = false;
                for c in s.chars() {
                    let ok = match what {
                        EditKey::Prop(_) => c.is_ascii_digit() || c == '.' || c == '-',
                        EditKey::Hex => c.is_ascii_hexdigit() || c == '#',
                        EditKey::Chan(_) => c.is_ascii_digit(),
                        EditKey::Name => !c.is_control(),
                    };
                    if ok {
                        tb.insert(c);
                        dirty = true;
                    }
                }
                dirty
            }
            _ => false,
        }
    }

    /// Whether a field is being typed into — the keyboard is its.
    pub(crate) fn inspector_typing(&self) -> bool {
        self.inspector.edit.is_some()
    }

    /// Commit the field being typed into, if any: a number lands on the
    /// primary, a code or a channel on the popup's swatch; anything else
    /// is let go. True when there was one.
    pub(crate) fn inspector_commit(&mut self) -> bool {
        let Some((what, tb)) = self.inspector.edit.take() else {
            return false;
        };
        let slot = self.inspector.popup.unwrap_or(Slot::Fg);
        match what {
            EditKey::Name => {
                // Emptied, the object goes back to its auto-label.
                self.editor.rename_primary(tb.text().trim().to_string());
            }
            EditKey::Prop(prop) => {
                if let Some(v) = field::parse(tb.text()) {
                    self.inspector_field_to(prop, v);
                    self.editor.end_gesture();
                }
            }
            EditKey::Hex => {
                if let Some(c) = spark_ui::from_hex(tb.text()) {
                    self.set_slot_colour(slot, [c[0], c[1], c[2]]);
                    self.editor.end_gesture();
                }
            }
            EditKey::Chan(k) => {
                if let Some(v) = field::parse(tb.text()) {
                    let rgb =
                        with_channel(self.slot_colour(slot), k, v.round().clamp(0.0, 255.0) as u8);
                    self.set_slot_colour(slot, rgb);
                    self.editor.end_gesture();
                }
            }
        }
        true
    }
}
