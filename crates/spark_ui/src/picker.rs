//! The color picker: an HSV square and hue bar painted by the ui shader's
//! dedicated gradient fills. Pure geometry — the caller owns the H/S/V
//! state and applies the picked color.

use spark_render::Viewport;

use crate::rects::{ICON_CIRCLE, ICON_HSV, ICON_HUE, UiRect};
use crate::theme::theme;

pub struct ColorPicker {
    pub sv: Viewport,
    pub hue: Viewport,
}

impl ColorPicker {
    pub fn new(x: f32, y: f32, w: f32, h: f32, scale: f32) -> Self {
        let bar = 34.0 * scale;
        let gap = 14.0 * scale;
        Self {
            sv: Viewport {
                x,
                y,
                w: (w - bar - gap).max(10.0),
                h,
            },
            hue: Viewport {
                x: x + w - bar,
                y,
                w: bar,
                h,
            },
        }
    }

    /// `h`, `s`, `v` in 0..1.
    pub fn rects(&self, h: f32, s: f32, v: f32, scale: f32) -> Vec<UiRect> {
        let t = theme();
        let hue_rgb = hsv_to_rgb(h, 1.0, 1.0);
        let mut sv_fill = UiRect::region_rounded(
            self.sv,
            [hue_rgb[0], hue_rgb[1], hue_rgb[2], 1.0],
            8.0 * scale,
        );
        sv_fill.icon[0] = ICON_HSV;
        let mut hue_fill = UiRect::region_rounded(self.hue, [1.0; 4], 8.0 * scale);
        hue_fill.icon[0] = ICON_HUE;

        let ring = 18.0 * scale;
        let mx = self.sv.x + s.clamp(0.0, 1.0) * self.sv.w;
        let my = self.sv.y + (1.0 - v.clamp(0.0, 1.0)) * self.sv.h;
        let hy = self.hue.y + h.clamp(0.0, 1.0) * self.hue.h;
        vec![
            sv_fill,
            hue_fill,
            UiRect::icon_sized(
                Viewport {
                    x: mx - ring * 0.5,
                    y: my - ring * 0.5,
                    w: ring,
                    h: ring,
                },
                ICON_CIRCLE,
                2.0 * scale,
                [1.0, 1.0, 1.0, 0.95],
                0.4,
            ),
            UiRect::region_rounded(
                Viewport {
                    x: self.hue.x - 3.0 * scale,
                    y: hy - 2.5 * scale,
                    w: self.hue.w + 6.0 * scale,
                    h: 5.0 * scale,
                },
                t.slider_thumb,
                2.5 * scale,
            ),
        ]
    }

    /// Saturation/value at a point inside the square.
    pub fn hit_sv(&self, px: f32, py: f32) -> Option<(f32, f32)> {
        self.sv.contains(px, py).then(|| self.sv_at(px, py))
    }

    /// Saturation/value at a point, clamped (for drags that wander off).
    pub fn sv_at(&self, px: f32, py: f32) -> (f32, f32) {
        (
            ((px - self.sv.x) / self.sv.w).clamp(0.0, 1.0),
            (1.0 - (py - self.sv.y) / self.sv.h).clamp(0.0, 1.0),
        )
    }

    pub fn hit_hue(&self, px: f32, py: f32) -> Option<f32> {
        self.hue.contains(px, py).then(|| self.hue_at(py))
    }

    pub fn hue_at(&self, py: f32) -> f32 {
        ((py - self.hue.y) / self.hue.h).clamp(0.0, 0.999)
    }
}

/// Display-space (sRGB) HSV → RGB, all 0..1.
pub fn hsv_to_rgb(h: f32, s: f32, v: f32) -> [f32; 3] {
    let k = h.clamp(0.0, 1.0) * 6.0;
    let r = (k - 3.0).abs() - 1.0;
    let g = 2.0 - (k - 2.0).abs();
    let b = 2.0 - (k - 4.0).abs();
    [
        v * (1.0 - s + s * r.clamp(0.0, 1.0)),
        v * (1.0 - s + s * g.clamp(0.0, 1.0)),
        v * (1.0 - s + s * b.clamp(0.0, 1.0)),
    ]
}

/// Display-space RGB → HSV, all 0..1.
pub fn rgb_to_hsv(rgb: [f32; 3]) -> [f32; 3] {
    let [r, g, b] = rgb;
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let d = max - min;
    let h = if d < 1e-5 {
        0.0
    } else if max == r {
        (((g - b) / d).rem_euclid(6.0)) / 6.0
    } else if max == g {
        ((b - r) / d + 2.0) / 6.0
    } else {
        ((r - g) / d + 4.0) / 6.0
    };
    let s = if max < 1e-5 { 0.0 } else { d / max };
    [h, s, max]
}

pub fn srgb_to_linear(c: f32) -> f32 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

pub fn linear_to_srgb(c: f32) -> f32 {
    if c <= 0.0031308 {
        c * 12.92
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    }
}
