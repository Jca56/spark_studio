//! The colour popup: click the foreground or background swatch and a
//! floating panel opens beside it — Lantern Studio's (Alva, 2026-08-31):
//! the HSV square and hue bar, a hex field and R G B fields, every one
//! typeable, an × to close. It edits the swatch it was opened on and
//! follows that swatch's colour while it is up. No alpha slider: a
//! shape's colour carries none — Opacity is its own number on the
//! inspector — and a slider that did nothing would be a lie.

use spark_render::Viewport;
use spark_ui::picker::linear_to_srgb;
use spark_ui::{ColorPicker, ICON_X, UiRect, hex_of, surfaces, theme};

use super::EditKey;
use crate::chrome::{Align, Label, MENU_TEXT, UI_TEXT};
use crate::textbox::TextBox;

/// The popup's size, logical px.
pub const W: f32 = 420.0;
pub const H: f32 = 540.0;
const PAD: f32 = 18.0;
/// The title row, the × in it, the picker, and the two field rows.
const TITLE_H: f32 = 48.0;
const CLOSE: f32 = 36.0;
const PICKER_H: f32 = 330.0;
const FIELD_H: f32 = 44.0;
const HEX_W: f32 = 160.0;
const CAP_W: f32 = 28.0;
const GAP: f32 = 12.0;
/// Air the popup keeps from the swatch it opened on and the window's edge.
const MARGIN: f32 = 16.0;

/// Which swatch the popup edits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Slot {
    Fg,
    Bg,
}

impl Slot {
    pub fn title(self) -> &'static str {
        match self {
            Slot::Fg => "Foreground",
            Slot::Bg => "Background",
        }
    }
}

/// A widget on the popup.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PopHit {
    Close,
    Sv,
    Hue,
    Hex,
    Chan(usize),
}

pub struct Popup {
    pub slot: Slot,
    pub panel: Viewport,
    pub close: Viewport,
    pub picker: ColorPicker,
    pub hex: Viewport,
    pub chans: [Viewport; 3],
    pub hsv: [f32; 3],
    pub rgb: [f32; 3],
    pub scale: f32,
    /// The hex or channel field being typed into, if one is.
    pub edit: Option<(EditKey, TextBox)>,
}

/// Display-space channels, 0..255, of a linear colour — what the R G B
/// fields show and take.
pub fn channels(rgb: [f32; 3]) -> [u8; 3] {
    rgb.map(|c| (linear_to_srgb(c).clamp(0.0, 1.0) * 255.0).round() as u8)
}

/// The code a linear colour prints as — lowercase, Lantern Studio's way.
pub fn hex(rgb: [f32; 3]) -> String {
    hex_of([rgb[0], rgb[1], rgb[2], 1.0]).to_lowercase()
}

/// Lay the popup out beside `anchor` (the swatch it opened on), pulled
/// back inside `win`. It goes to the swatch's left — the inspector sits
/// at the window's right edge — and never off the window.
pub fn build(
    anchor: Viewport,
    win: Viewport,
    scale: f32,
    slot: Slot,
    rgb: [f32; 3],
    hsv: [f32; 3],
    edit: Option<&(EditKey, TextBox)>,
) -> Popup {
    let s = scale;
    let (w, h) = (W * s, H * s);
    let margin = MARGIN * s;
    let x = (anchor.x - w - margin)
        .min(win.x + win.w - w)
        .max(win.x);
    let y = (anchor.y - PAD * s).min(win.y + win.h - h).max(win.y);
    let panel = Viewport { x, y, w, h };
    let pad = PAD * s;
    let x0 = panel.x + pad;
    let inner = w - pad * 2.0;
    let close_side = CLOSE * s;
    let close = Viewport {
        x: panel.x + w - pad - close_side,
        y: panel.y + pad + (TITLE_H * s - close_side) * 0.5 - 6.0 * s,
        w: close_side,
        h: close_side,
    };
    let mut yy = panel.y + pad + TITLE_H * s;
    let picker = ColorPicker::new(x0, yy, inner, PICKER_H * s, s);
    yy += PICKER_H * s + GAP * s * 1.5;
    let field_h = FIELD_H * s;
    let cap = CAP_W * s;
    let hex = Viewport {
        x: x0 + cap,
        y: yy,
        w: HEX_W * s,
        h: field_h,
    };
    yy += field_h + GAP * s;
    let step = inner / 3.0;
    let chans = std::array::from_fn(|k| Viewport {
        x: x0 + step * k as f32 + cap,
        y: yy,
        w: step - cap - GAP * s,
        h: field_h,
    });
    Popup {
        slot,
        panel,
        close,
        picker,
        hex,
        chans,
        hsv,
        rgb,
        scale,
        edit: edit
            .filter(|(k, _)| matches!(k, EditKey::Hex | EditKey::Chan(_)))
            .cloned(),
    }
}

impl Popup {
    pub fn hit(&self, x: f32, y: f32) -> Option<PopHit> {
        if self.close.contains(x, y) {
            return Some(PopHit::Close);
        }
        if self.picker.hit_sv(x, y).is_some() {
            return Some(PopHit::Sv);
        }
        if self.picker.hit_hue(x, y).is_some() {
            return Some(PopHit::Hue);
        }
        if self.hex.contains(x, y) {
            return Some(PopHit::Hex);
        }
        if let Some(k) = self.chans.iter().position(|c| c.contains(x, y)) {
            return Some(PopHit::Chan(k));
        }
        None
    }

    /// Where the text of a field starts.
    fn text_x(&self, field: Viewport) -> f32 {
        field.x + 14.0 * self.scale
    }

    /// The field being typed into: its box, text origin and font size,
    /// for the caret the frame draws once it has measured the text.
    pub fn edit_box(&self) -> Option<(Viewport, f32, f32)> {
        let field = match self.edit.as_ref()?.0 {
            EditKey::Hex => self.hex,
            EditKey::Chan(k) => *self.chans.get(k)?,
            _ => return None,
        };
        Some((field, self.text_x(field), UI_TEXT * self.scale))
    }

    fn editing(&self, key: EditKey) -> bool {
        self.edit.as_ref().is_some_and(|(k, _)| *k == key)
    }

    pub fn rects(&self) -> Vec<UiRect> {
        let t = theme();
        let m = surfaces();
        let s = self.scale;
        let mut out = vec![m.float.rect(self.panel, s)];
        out.push(m.plate.rect(self.close, s));
        out.push(UiRect::icon_sized(self.close, ICON_X, 2.0 * s, t.icon, 0.34));
        let [h, sat, v] = self.hsv;
        out.extend(self.picker.rects(h, sat, v, s));
        let well = |r: Viewport, lit: bool| {
            if lit {
                m.well.edged(r, s, t.accent)
            } else {
                m.well.rect(r, s)
            }
        };
        out.push(well(self.hex, self.editing(EditKey::Hex)));
        for (k, c) in self.chans.iter().enumerate() {
            out.push(well(*c, self.editing(EditKey::Chan(k))));
        }
        out
    }

    pub fn labels(&self) -> Vec<Label> {
        let t = theme();
        let s = self.scale;
        let size = UI_TEXT * s;
        let line = |sz: f32| spark_text::Text::line_height(sz);
        let mut out = vec![Label {
            text: self.slot.title().to_string(),
            size: MENU_TEXT * s,
            pos: [self.panel.x + PAD * s, self.panel.y + 14.0 * s],
            color: t.accent,
            max_w: self.panel.w - PAD * 2.0 * s - self.close.w,
            align: Align::Left,
        }];
        let text_of = |key: EditKey, shown: String| -> String {
            match &self.edit {
                Some((k, tb)) if *k == key => tb.text().to_string(),
                _ => shown,
            }
        };
        let field = |out: &mut Vec<Label>, caption: &str, r: Viewport, key: EditKey, shown: String| {
            let y = r.y + (r.h - line(size)) * 0.5;
            out.push(Label {
                text: caption.to_string(),
                size,
                pos: [r.x - 6.0 * s, y],
                color: t.text_dim,
                max_w: CAP_W * s,
                align: Align::Right,
            });
            out.push(Label {
                text: text_of(key, shown),
                size,
                pos: [self.text_x(r), y],
                color: t.text,
                max_w: r.w - 20.0 * s,
                align: Align::Left,
            });
        };
        field(&mut out, "#", self.hex, EditKey::Hex, hex(self.rgb));
        let ch = channels(self.rgb);
        for (k, cap) in ["R", "G", "B"].iter().enumerate() {
            field(
                &mut out,
                cap,
                self.chans[k],
                EditKey::Chan(k),
                ch[k].to_string(),
            );
        }
        out
    }
}
