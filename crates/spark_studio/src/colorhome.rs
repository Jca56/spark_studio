//! The color home: the always-visible section at the top of the right
//! panel — color has exactly one home. It paints the selection (or the
//! gradient endpoint armed on its layer card); with nothing selected it
//! sets the draw color for the next shape. Pure layout + hit testing.

use spark_render::Viewport;
use spark_ui::picker::hsv_to_rgb;
use spark_ui::{ColorPicker, Layout, Swatches};

use crate::editor::PALETTE;
use crate::{Studio, layers};

impl Studio {
    /// The right panel's regions and laid-out layer cards.
    pub(crate) fn right_panel(&self, layout: &Layout) -> (Viewport, Viewport, layers::Cards) {
        let scale = self.scale();
        let (color_vp, cards_vp) = split(layout.right, scale, self.picker_hsv.is_some());
        let cards = layers::rows(
            cards_vp,
            scale,
            &self.editor,
            self.card_open,
            self.card_tab,
            self.layers_scroll,
        );
        (color_vp, cards_vp, cards)
    }

    /// The color home always shows the *current color* — never the
    /// selection's. Selecting a shape doesn't move it; the eyedropper does.
    /// That way the color you lined up survives clicking around the stack.
    pub(crate) fn color_home(&self, region: Viewport) -> ColorHome {
        build(
            region,
            self.scale(),
            self.editor.color(),
            self.editor.palette_match(),
            self.picker_hsv,
        )
    }
}

/// Split the right panel: the color section on top (taller while the
/// picker is open), layer cards below.
pub fn split(right: Viewport, scale: f32, picker_open: bool) -> (Viewport, Viewport) {
    let h = (if picker_open { 346.0 } else { 110.0 }) * scale;
    let h = h.min(right.h);
    (
        Viewport {
            x: right.x,
            y: right.y,
            w: right.w,
            h,
        },
        Viewport {
            x: right.x,
            y: right.y + h,
            w: right.w,
            h: (right.h - h).max(1.0),
        },
    )
}

pub struct ColorHome {
    pub region: Viewport,
    pub swatches: Swatches,
    /// Palette entry to ring as selected, if the active color matches one.
    pub palette: Option<usize>,
    /// The current-color bar; clicking it opens/closes the picker.
    pub custom: Viewport,
    /// The active color (linear) the bar previews.
    pub custom_rgb: [f32; 3],
    /// Open picker: geometry plus its H/S/V and hex readout position.
    pub picker: Option<(ColorPicker, [f32; 3], [f32; 2])>,
}

pub fn build(
    region: Viewport,
    scale: f32,
    active_rgb: [f32; 3],
    palette: Option<usize>,
    picker_hsv: Option<[f32; 3]>,
) -> ColorHome {
    let pad = 14.0 * scale;
    let content_w = (region.w - pad * 2.0).max(1.0);
    let mut y = region.y + 12.0 * scale;
    let n = PALETTE.len();
    let side = 40.0 * scale;
    let gap = ((content_w - side * n as f32) / (n - 1) as f32).max(6.0 * scale);
    let swatches = Swatches::new(region.x + pad, y, side, gap, n);
    y += side + 12.0 * scale;
    let custom = Viewport {
        x: region.x + pad,
        y,
        w: content_w,
        h: 28.0 * scale,
    };
    y += 40.0 * scale;
    let picker = picker_hsv.map(|hsv| {
        let p = ColorPicker::new(region.x + pad, y, content_w, 190.0 * scale, scale);
        y += 200.0 * scale;
        (p, hsv, [region.x + pad, y])
    });
    ColorHome {
        region,
        swatches,
        palette,
        custom,
        custom_rgb: active_rgb,
        picker,
    }
}

pub enum ColorHit {
    Swatch(usize),
    /// The current-color bar: open/close the picker.
    Custom,
    /// A click in the HSV square: (saturation, value).
    Sv(f32, f32),
    /// A click on the hue bar.
    Hue(f32),
}

impl ColorHome {
    pub fn hit(&self, px: f32, py: f32) -> Option<ColorHit> {
        if !self.region.contains(px, py) {
            return None;
        }
        if let Some(i) = self.swatches.hit(px, py) {
            return Some(ColorHit::Swatch(i));
        }
        if self.custom.contains(px, py) {
            return Some(ColorHit::Custom);
        }
        if let Some((p, _, _)) = &self.picker {
            if let Some((s, v)) = p.hit_sv(px, py) {
                return Some(ColorHit::Sv(s, v));
            }
            if let Some(h) = p.hit_hue(px, py) {
                return Some(ColorHit::Hue(h));
            }
        }
        None
    }
}

/// sRGB hex for the picker readout.
pub fn hex_of(hsv: [f32; 3]) -> String {
    let rgb = hsv_to_rgb(hsv[0], hsv[1], hsv[2]);
    format!(
        "#{:02X}{:02X}{:02X}",
        (rgb[0] * 255.0).round() as u8,
        (rgb[1] * 255.0).round() as u8,
        (rgb[2] * 255.0).round() as u8
    )
}
