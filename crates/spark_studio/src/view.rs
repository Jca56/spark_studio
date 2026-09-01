//! The canvas view: where the stage — the comp's canvas, whatever its size
//! — sits inside the viewport.
//! 100% zoom = the stage aspect-fit to the viewport exactly (the resting
//! default); zooming out grows a gutter around it, zooming in pans. One
//! mapping, the same deal the timeline's TimeView gives the time axis.
//! Also home to the transparency checkerboard the stage sits on. The zoom
//! buttons that drive it live on the transport toolbar (`timeline`).

use spark_render::{CANVAS, Viewport};
use spark_ui::{UiRect, theme};

/// Canvas-units → window-px mapping: (scale, offset x, offset y).
pub type CanvasMap = (f32, f32, f32);

pub struct CanvasView {
    /// 1.0 = the stage aspect-fits the viewport exactly (100%).
    zoom: f32,
    /// The canvas point pinned to the viewport center.
    pan: [f32; 2],
}

impl CanvasView {
    /// Resting on the default canvas; `reset` re-centres it on the comp's.
    pub fn new() -> Self {
        Self {
            zoom: 1.0,
            pan: [CANVAS[0] * 0.5, CANVAS[1] * 0.5],
        }
    }

    /// The map for a `canvas`-sized stage in `vp`.
    pub fn map(&self, vp: Viewport, canvas: [f32; 2]) -> CanvasMap {
        let fit = (vp.w / canvas[0]).min(vp.h / canvas[1]).max(0.0001);
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
    pub fn zoom_at(&mut self, factor: f32, px: f32, py: f32, vp: Viewport, canvas: [f32; 2]) {
        let (s, ox, oy) = self.map(vp, canvas);
        let c = [(px - ox) / s, (py - oy) / s];
        self.zoom = (self.zoom * factor).clamp(0.25, 8.0);
        let (s2, _, _) = self.map(vp, canvas);
        self.pan = [
            c[0] + (vp.x + vp.w * 0.5 - px) / s2,
            c[1] + (vp.y + vp.h * 0.5 - py) / s2,
        ];
        self.clamp_pan(canvas);
    }

    /// Zoom around the viewport center — the +/- buttons.
    pub fn zoom_step(&mut self, factor: f32, vp: Viewport, canvas: [f32; 2]) {
        self.zoom_at(factor, vp.x + vp.w * 0.5, vp.y + vp.h * 0.5, vp, canvas);
    }

    /// Pan by a window-px delta — the stage follows the cursor.
    pub fn pan_px(&mut self, dx: f32, dy: f32, vp: Viewport, canvas: [f32; 2]) {
        let (s, _, _) = self.map(vp, canvas);
        self.pan[0] -= dx / s;
        self.pan[1] -= dy / s;
        self.clamp_pan(canvas);
    }

    /// Back to the resting view: 100%, centered on `canvas`.
    pub fn reset(&mut self, canvas: [f32; 2]) {
        self.zoom = 1.0;
        self.pan = [canvas[0] * 0.5, canvas[1] * 0.5];
    }

    /// The viewport center never leaves the stage, so the canvas can't be
    /// panned out of sight.
    fn clamp_pan(&mut self, canvas: [f32; 2]) {
        self.pan[0] = self.pan[0].clamp(0.0, canvas[0]);
        self.pan[1] = self.pan[1].clamp(0.0, canvas[1]);
    }
}

impl Default for CanvasView {
    fn default() -> Self {
        Self::new()
    }
}

/// The transparency checkerboard under the stage: screen-fixed cell size,
/// pattern anchored to the canvas origin so it rides pan and zoom without
/// swimming. Only visible cells are emitted.
pub fn checker_rects(map: CanvasMap, vp: Viewport, ui_scale: f32, canvas: [f32; 2]) -> Vec<UiRect> {
    let (s, ox, oy) = map;
    let x0 = ox.max(vp.x);
    let y0 = oy.max(vp.y);
    let x1 = (ox + canvas[0] * s).min(vp.x + vp.w);
    let y1 = (oy + canvas[1] * s).min(vp.y + vp.h);
    if x1 <= x0 || y1 <= y0 {
        return Vec::new();
    }
    let [light, dark] = theme().checker;
    let mut out = vec![UiRect::region(
        Viewport {
            x: x0,
            y: y0,
            w: x1 - x0,
            h: y1 - y0,
        },
        light,
    )];
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
