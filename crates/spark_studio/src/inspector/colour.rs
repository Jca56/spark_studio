//! The inspector's colour: the foreground and background swatches, the
//! popup that edits them, and the conversions between the linear light
//! every shape holds and the display-space numbers a person reads.
//! Split from `inspector` so the panel's routing stays readable.

use spark_render::Viewport;
use spark_ui::picker::{hsv_to_rgb, linear_to_srgb, rgb_to_hsv, srgb_to_linear};

use super::popup::{self, PopHit, Popup, Slot};
use super::{Drag, EditKey};
use crate::Studio;
use crate::textbox::TextBox;

/// The popup's HSV for a colour, which every shape holds in linear light
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

pub(super) fn same_colour(a: [f32; 3], b: [f32; 3]) -> bool {
    a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-4)
}

/// A colour with one display-space channel replaced — what typing into
/// an R, G or B field means.
pub fn with_channel(rgb: [f32; 3], k: usize, value: u8) -> [f32; 3] {
    let mut ch = popup::channels(rgb);
    if let Some(c) = ch.get_mut(k) {
        *c = value;
    }
    ch.map(|c| srgb_to_linear(c as f32 / 255.0))
}

impl Studio {
    pub(super) fn slot_colour(&self, slot: Slot) -> [f32; 3] {
        match slot {
            Slot::Fg => self.editor.color(),
            Slot::Bg => self.editor.color_b(),
        }
    }

    /// Write a swatch's colour: the foreground paints the selection, the
    /// background a selected shape's gradient end. Always a change on
    /// screen — the swatch itself.
    pub(super) fn set_slot_colour(&mut self, slot: Slot, rgb: [f32; 3]) {
        match slot {
            Slot::Fg => {
                self.editor.set_current_color(rgb, false);
            }
            Slot::Bg => {
                self.editor.set_color_b(rgb);
            }
        }
    }

    /// The popup, laid out beside the swatch it is open on.
    pub(super) fn popup_for(&self) -> Option<Popup> {
        let slot = self.inspector.popup?;
        let layout = self.layout()?;
        let (w, h) = self.gpu.as_ref()?.size();
        let page = self.inspector_page(layout.right);
        let anchor = match slot {
            Slot::Fg => page.fg,
            Slot::Bg => page.bg,
        };
        Some(popup::build(
            anchor,
            Viewport {
                x: 0.0,
                y: 0.0,
                w: w as f32,
                h: h as f32,
            },
            self.scale(),
            slot,
            self.slot_colour(slot),
            self.inspector.hsv,
            self.inspector.edit.as_ref(),
        ))
    }

    /// Whether a point is on the open popup.
    pub(crate) fn popup_contains(&self, x: f32, y: f32) -> bool {
        self.popup_for().is_some_and(|p| p.panel.contains(x, y)) || self.react_contains(x, y)
    }

    /// A left press while the popup is up. Inside it, its widgets take
    /// the click (`Some(true)`: swallowed). On the inspector the popup
    /// stays up and the press is the inspector's (`None`); anywhere else
    /// closes it and the press goes on to whatever it hit (`Some(false)`).
    pub(crate) fn popup_press(&mut self, cx: f32, cy: f32) -> Option<bool> {
        self.inspector.popup?;
        let Some(p) = self.popup_for() else {
            self.inspector.popup = None;
            return Some(false);
        };
        if !p.panel.contains(cx, cy) {
            let in_right = self.layout().is_some_and(|l| l.right.contains(cx, cy));
            if in_right {
                return None;
            }
            self.inspector_commit();
            self.inspector.popup = None;
            return Some(false);
        }
        let hit = p.hit(cx, cy);
        // A click inside the field being edited places the caret.
        if let Some((key, _)) = &p.edit {
            let same = match (key, hit) {
                (EditKey::Hex, Some(PopHit::Hex)) => true,
                (EditKey::Chan(a), Some(PopHit::Chan(b))) => *a == b,
                _ => false,
            };
            if same {
                let at = crate::textbox::index_at(&self.inspector.caret_xs, cx);
                if let Some((_, tb)) = &mut self.inspector.edit {
                    tb.place(at);
                }
                return Some(true);
            }
        }
        self.inspector_commit();
        let rgb = self.slot_colour(p.slot);
        match hit {
            Some(PopHit::Close) => self.inspector.popup = None,
            Some(PopHit::Sv) => {
                let (s, v) = p.picker.sv_at(cx, cy);
                self.inspector_set_hsv([self.inspector.hsv[0], s, v]);
                self.inspector.drag = Some(Drag::Sv);
            }
            Some(PopHit::Hue) => {
                let h = p.picker.hue_at(cy);
                self.inspector_set_hsv([h, self.inspector.hsv[1], self.inspector.hsv[2]]);
                self.inspector.drag = Some(Drag::Hue);
            }
            Some(PopHit::Hex) => {
                self.inspector.edit =
                    Some((EditKey::Hex, TextBox::selecting_all(popup::hex(rgb))));
            }
            Some(PopHit::Chan(k)) => {
                let ch = popup::channels(rgb);
                self.inspector.edit = Some((
                    EditKey::Chan(k),
                    TextBox::selecting_all(ch.get(k).copied().unwrap_or(0).to_string()),
                ));
            }
            None => {}
        }
        Some(true)
    }

    /// Close the popup if it is up; true when it was.
    pub(crate) fn popup_close(&mut self) -> bool {
        let react = self.inspector.react.take().is_some();
        if self.inspector.popup.take().is_some() || react {
            self.inspector_commit();
            self.inspector.drag = None;
            true
        } else {
            false
        }
    }

    /// The popup's picker moved: its swatch's colour follows — the
    /// foreground painting the selection, the background a gradient end.
    pub(super) fn inspector_set_hsv(&mut self, hsv: [f32; 3]) {
        self.inspector.hsv = hsv;
        let slot = self.inspector.popup.unwrap_or(Slot::Fg);
        self.set_slot_colour(slot, rgb_of(hsv));
    }
}
