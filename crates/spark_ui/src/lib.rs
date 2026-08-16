//! SparkUI — the editor's own UI, drawn by the engine.
//!
//! v0: the editor layout as flat charcoal panels. Next: container/grid
//! layout framework, then the reusable widget suite.

use spark_render::Viewport;

mod rects;
mod theme;

pub use rects::{UiPass, UiRect};
pub use theme::{srgb, theme};

/// The editor's panel regions, computed from window size + UI scale.
///
/// Slim top toolbar; left all-purpose panel; right inspector; full-width
/// timeline along the bottom (time deserves every horizontal pixel); the
/// remaining center is the viewport, canvas aspect-fit.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Layout {
    pub top: Viewport,
    pub left: Viewport,
    pub right: Viewport,
    pub timeline: Viewport,
    pub viewport: Viewport,
}

impl Layout {
    pub fn compute(width: u32, height: u32, scale: f32) -> Self {
        let (w, h) = (width as f32, height as f32);
        let top_h = (64.0 * scale).min(h * 0.12);
        let bottom_h = (280.0 * scale).min(h * 0.35);
        let left_w = (340.0 * scale).min(w * 0.22);
        let right_w = (380.0 * scale).min(w * 0.24);
        let mid_h = (h - top_h - bottom_h).max(1.0);
        Self {
            top: Viewport {
                x: 0.0,
                y: 0.0,
                w,
                h: top_h,
            },
            left: Viewport {
                x: 0.0,
                y: top_h,
                w: left_w,
                h: mid_h,
            },
            right: Viewport {
                x: w - right_w,
                y: top_h,
                w: right_w,
                h: mid_h,
            },
            timeline: Viewport {
                x: 0.0,
                y: h - bottom_h,
                w,
                h: bottom_h,
            },
            viewport: Viewport {
                x: left_w,
                y: top_h,
                w: (w - left_w - right_w).max(1.0),
                h: mid_h,
            },
        }
    }

    /// The chrome as flat rects: panels plus seam lines between regions.
    pub fn panel_rects(&self, scale: f32) -> Vec<UiRect> {
        let t = theme();
        let seam = (2.0 * scale).max(1.0);
        vec![
            UiRect::region(self.top, t.toolbar),
            UiRect::region(self.left, t.panel),
            UiRect::region(self.right, t.panel),
            UiRect::region(self.timeline, t.timeline),
            // seams
            UiRect {
                pos: [self.top.x, self.top.y + self.top.h - seam],
                size: [self.top.w, seam],
                color: t.seam,
            },
            UiRect {
                pos: [self.timeline.x, self.timeline.y],
                size: [self.timeline.w, seam],
                color: t.seam,
            },
            UiRect {
                pos: [self.left.x + self.left.w - seam, self.left.y],
                size: [seam, self.left.h],
                color: t.seam,
            },
            UiRect {
                pos: [self.right.x, self.right.y],
                size: [seam, self.right.h],
                color: t.seam,
            },
        ]
    }
}
