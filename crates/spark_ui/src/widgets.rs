//! Reusable SparkUI widgets. First resident: the icon button bar.

use spark_render::Viewport;

use crate::rects::UiRect;
use crate::theme::theme;

/// A row of square icon buttons inside a host rect, left-aligned and
/// vertically centered. Generic over the caller's id type so the same widget
/// serves tool bars, transport controls, tab strips, and the rest.
pub struct IconBar<I: Copy + PartialEq> {
    buttons: Vec<(I, f32, Viewport)>,
    scale: f32,
}

impl<I: Copy + PartialEq> IconBar<I> {
    /// `items` is `(id, icon kind)` per button (icon kinds from `rects`).
    pub fn new(rect: Viewport, scale: f32, items: &[(I, f32)]) -> Self {
        let pad = 4.0 * scale;
        let side = (rect.h - pad * 2.0).max(1.0);
        let gap = 8.0 * scale;
        let mut x = rect.x + pad;
        let mut buttons = Vec::with_capacity(items.len());
        for &(id, icon) in items {
            buttons.push((
                id,
                icon,
                Viewport {
                    x,
                    y: rect.y + pad,
                    w: side,
                    h: side,
                },
            ));
            x += side + gap;
        }
        Self { buttons, scale }
    }

    pub fn hit(&self, px: f32, py: f32) -> Option<I> {
        self.buttons
            .iter()
            .find(|(_, _, v)| v.contains(px, py))
            .map(|(id, _, _)| *id)
    }

    pub fn rects(&self, hover: Option<I>, active: Option<I>) -> Vec<UiRect> {
        let t = theme();
        let mut out = Vec::with_capacity(self.buttons.len() * 2);
        for &(id, icon, r) in &self.buttons {
            let is_active = active == Some(id);
            let is_hover = hover == Some(id);
            if is_active {
                out.push(UiRect::region(r, t.accent_bg));
            } else if is_hover {
                out.push(UiRect::region(r, t.button_hover));
            }
            let fg = if is_active {
                t.accent
            } else if is_hover {
                t.icon_hover
            } else {
                t.icon
            };
            out.push(UiRect::icon_sized(r, icon, 2.0 * self.scale, fg, 0.34));
        }
        out
    }
}

/// A horizontal slider: rounded track, accent fill, round thumb.
/// Pure geometry — the caller owns the value mapping and drag state.
pub struct Slider;

impl Slider {
    pub fn rects(track: Viewport, t: f32) -> Vec<UiRect> {
        let th = theme();
        let t = t.clamp(0.0, 1.0);
        let radius = track.h * 0.5;
        let fill_w = (track.w * t).max(track.h);
        let side = track.h * 2.2;
        let cx = track.x + track.w * t;
        vec![
            UiRect::region_rounded(track, th.slider_track, radius),
            UiRect::region_rounded(
                Viewport {
                    x: track.x,
                    y: track.y,
                    w: fill_w,
                    h: track.h,
                },
                th.accent,
                radius,
            ),
            UiRect::region_rounded(
                Viewport {
                    x: cx - side * 0.5,
                    y: track.y + track.h * 0.5 - side * 0.5,
                    w: side,
                    h: side,
                },
                th.slider_thumb,
                side * 0.5,
            ),
        ]
    }
}
