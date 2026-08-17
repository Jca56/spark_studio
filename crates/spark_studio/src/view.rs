//! The canvas view: where the 1920x1080 stage sits inside the viewport.
//! 100% zoom = the stage aspect-fit to the viewport exactly (the resting
//! default); zooming out grows a gutter around it, zooming in pans. One
//! mapping, the same deal the timeline's TimeView gives the time axis.
//! Also home to the zoom bar (right panel's bottom strip) and the
//! transparency checkerboard the stage sits on.

use spark_render::{CANVAS_H, CANVAS_W, Viewport};
use spark_ui::{UiRect, srgb, theme};

/// Canvas-units → window-px mapping: (scale, offset x, offset y).
pub type CanvasMap = (f32, f32, f32);

pub struct CanvasView {
    /// 1.0 = the stage aspect-fits the viewport exactly (100%).
    zoom: f32,
    /// The canvas point pinned to the viewport center.
    pan: [f32; 2],
}

impl CanvasView {
    pub fn new() -> Self {
        Self {
            zoom: 1.0,
            pan: [CANVAS_W * 0.5, CANVAS_H * 0.5],
        }
    }

    pub fn map(&self, vp: Viewport, _ui_scale: f32) -> CanvasMap {
        let fit = (vp.w / CANVAS_W).min(vp.h / CANVAS_H).max(0.0001);
        let s = fit * self.zoom;
        (
            s,
            vp.x + vp.w * 0.5 - self.pan[0] * s,
            vp.y + vp.h * 0.5 - self.pan[1] * s,
        )
    }

    /// The readout percentage (100 = exact fit).
    pub fn pct(&self) -> u16 {
        (self.zoom * 100.0).round() as u16
    }

    /// Zoom by `factor`, keeping the canvas point under the cursor still.
    pub fn zoom_at(&mut self, factor: f32, px: f32, py: f32, vp: Viewport, ui_scale: f32) {
        let (s, ox, oy) = self.map(vp, ui_scale);
        let c = [(px - ox) / s, (py - oy) / s];
        self.zoom = (self.zoom * factor).clamp(0.25, 8.0);
        let (s2, _, _) = self.map(vp, ui_scale);
        self.pan = [
            c[0] + (vp.x + vp.w * 0.5 - px) / s2,
            c[1] + (vp.y + vp.h * 0.5 - py) / s2,
        ];
        self.clamp_pan();
    }

    /// Zoom around the viewport center — the +/- buttons.
    pub fn zoom_step(&mut self, factor: f32, vp: Viewport, ui_scale: f32) {
        self.zoom_at(factor, vp.x + vp.w * 0.5, vp.y + vp.h * 0.5, vp, ui_scale);
    }

    /// Pan by a window-px delta — the stage follows the cursor.
    pub fn pan_px(&mut self, dx: f32, dy: f32, vp: Viewport, ui_scale: f32) {
        let (s, _, _) = self.map(vp, ui_scale);
        self.pan[0] -= dx / s;
        self.pan[1] -= dy / s;
        self.clamp_pan();
    }

    /// Back to the resting view: 100%, centered.
    pub fn reset(&mut self) {
        self.zoom = 1.0;
        self.pan = [CANVAS_W * 0.5, CANVAS_H * 0.5];
    }

    /// The viewport center never leaves the stage, so the canvas can't be
    /// panned out of sight.
    fn clamp_pan(&mut self) {
        self.pan[0] = self.pan[0].clamp(0.0, CANVAS_W);
        self.pan[1] = self.pan[1].clamp(0.0, CANVAS_H);
    }
}

impl Default for CanvasView {
    fn default() -> Self {
        Self::new()
    }
}

/// The zoom bar pinned to the bottom of the right panel: - / + steppers
/// and one button that shows the live percentage and refits to 100% on
/// click.
pub struct ZoomBar {
    pub minus: Viewport,
    pub plus: Viewport,
    /// The combined readout/refit button.
    pub pct: Viewport,
}

pub fn zoom_bar(bar: Viewport, scale: f32) -> ZoomBar {
    let btn = bar.h - 16.0 * scale;
    let y = bar.y + 8.0 * scale;
    let minus = Viewport {
        x: bar.x + 12.0 * scale,
        y,
        w: btn,
        h: btn,
    };
    let plus = Viewport {
        x: minus.x + btn + 8.0 * scale,
        y,
        w: btn,
        h: btn,
    };
    ZoomBar {
        minus,
        plus,
        pct: Viewport {
            x: plus.x + btn + 14.0 * scale,
            y,
            w: 130.0 * scale,
            h: btn,
        },
    }
}

/// Button chrome; glyphs are plain bars (a minus, a plus) — no shader
/// glyphs needed. The percent button's label is text, drawn by chrome.
pub fn zoom_bar_rects(zb: &ZoomBar, scale: f32, hover: Option<u8>) -> Vec<UiRect> {
    let t = theme();
    let mut out = Vec::new();
    for (i, r) in [zb.minus, zb.plus, zb.pct].into_iter().enumerate() {
        let bg = if hover == Some(i as u8) {
            t.button_hover
        } else {
            t.card
        };
        out.push(UiRect::region_rounded(r, bg, 10.0 * scale));
    }
    let len = zb.minus.w * 0.44;
    let thick = 3.5 * scale;
    let hbar = |r: Viewport| Viewport {
        x: r.x + (r.w - len) * 0.5,
        y: r.y + (r.h - thick) * 0.5,
        w: len,
        h: thick,
    };
    out.push(UiRect::region_rounded(
        hbar(zb.minus),
        t.icon_hover,
        thick * 0.5,
    ));
    out.push(UiRect::region_rounded(
        hbar(zb.plus),
        t.icon_hover,
        thick * 0.5,
    ));
    out.push(UiRect::region_rounded(
        Viewport {
            x: zb.plus.x + (zb.plus.w - thick) * 0.5,
            y: zb.plus.y + (zb.plus.h - len) * 0.5,
            w: thick,
            h: len,
        },
        t.icon_hover,
        thick * 0.5,
    ));
    out
}

/// The transparency checkerboard under the stage: screen-fixed cell size,
/// pattern anchored to the canvas origin so it rides pan and zoom without
/// swimming. Only visible cells are emitted.
pub fn checker_rects(map: CanvasMap, vp: Viewport, ui_scale: f32) -> Vec<UiRect> {
    let (s, ox, oy) = map;
    let x0 = ox.max(vp.x);
    let y0 = oy.max(vp.y);
    let x1 = (ox + CANVAS_W * s).min(vp.x + vp.w);
    let y1 = (oy + CANVAS_H * s).min(vp.y + vp.h);
    if x1 <= x0 || y1 <= y0 {
        return Vec::new();
    }
    let mut out = vec![UiRect::region(
        Viewport {
            x: x0,
            y: y0,
            w: x1 - x0,
            h: y1 - y0,
        },
        srgb(0x191919),
    )];
    let dark = srgb(0x242424);
    let cell = 22.0 * ui_scale;
    let i0 = ((x0 - ox) / cell).floor() as i64;
    let j0 = ((y0 - oy) / cell).floor() as i64;
    let i1 = ((x1 - ox) / cell).ceil() as i64;
    let j1 = ((y1 - oy) / cell).ceil() as i64;
    for j in j0..j1 {
        for i in i0..i1 {
            if (i + j).rem_euclid(2) == 0 {
                continue;
            }
            let rx = (ox + i as f32 * cell).max(x0);
            let ry = (oy + j as f32 * cell).max(y0);
            let rw = (ox + (i + 1) as f32 * cell).min(x1) - rx;
            let rh = (oy + (j + 1) as f32 * cell).min(y1) - ry;
            if rw > 0.0 && rh > 0.0 {
                out.push(UiRect::region(
                    Viewport {
                        x: rx,
                        y: ry,
                        w: rw,
                        h: rh,
                    },
                    dark,
                ));
            }
        }
    }
    out
}
