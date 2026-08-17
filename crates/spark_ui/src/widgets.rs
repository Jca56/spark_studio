//! Reusable SparkUI widgets: icon bars, color swatches, segmented toggles,
//! sliders. All pure geometry + hit testing — callers own state and text.

use spark_render::Viewport;

use crate::rect::UiRect;
use crate::theme::{srgb, theme};

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
        let pad = 6.0 * scale;
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
                // Gold glyph on the purple highlight — Spark's two accents.
                t.grad_gold
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

/// A drop-down menu: a text anchor button plus, when open, a floating panel
/// of item rows layered over whatever is beneath it. Pure geometry — the
/// caller measures labels, owns the open state, and draws all text.
pub struct Menu {
    pub anchor: Viewport,
    pub panel: Viewport,
    pub items: Vec<Viewport>,
    scale: f32,
}

impl Menu {
    /// `item_w` is the measured width of the widest item label (physical px);
    /// rows pad around it. The panel drops from the anchor's bottom edge.
    pub fn new(anchor: Viewport, item_count: usize, item_w: f32, scale: f32) -> Self {
        let pad = 8.0 * scale;
        let row_h = 52.0 * scale;
        let w = (item_w + 48.0 * scale).max(anchor.w);
        let panel = Viewport {
            x: anchor.x,
            y: anchor.y + anchor.h + 4.0 * scale,
            w,
            h: row_h * item_count as f32 + pad * 2.0,
        };
        let items = (0..item_count)
            .map(|i| Viewport {
                x: panel.x + pad,
                y: panel.y + pad + row_h * i as f32,
                w: w - pad * 2.0,
                h: row_h,
            })
            .collect();
        Self {
            anchor,
            panel,
            items,
            scale,
        }
    }

    pub fn hit_anchor(&self, px: f32, py: f32) -> bool {
        self.anchor.contains(px, py)
    }

    /// Only meaningful while the caller holds the menu open.
    pub fn hit_item(&self, px: f32, py: f32) -> Option<usize> {
        self.items.iter().position(|v| v.contains(px, py))
    }

    pub fn anchor_rects(&self, open: bool, hover: bool) -> Vec<UiRect> {
        let t = theme();
        let radius = 8.0 * self.scale;
        if open {
            vec![UiRect::region_rounded(self.anchor, t.accent_bg, radius)]
        } else if hover {
            vec![UiRect::region_rounded(self.anchor, t.button_hover, radius)]
        } else {
            Vec::new()
        }
    }

    /// The floating panel: border, body, and the hovered row's highlight.
    /// Append these after everything else — menus draw on top.
    pub fn panel_rects(&self, hover: Option<usize>) -> Vec<UiRect> {
        let t = theme();
        let border = 3.0 * self.scale;
        let radius = 10.0 * self.scale;
        let mut out =
            vec![UiRect::region_rounded(self.panel, t.card, radius).stroke(border, t.seam)];
        if let Some(i) = hover
            && let Some(&row) = self.items.get(i)
        {
            out.push(UiRect::region_rounded(
                row,
                t.button_hover,
                8.0 * self.scale,
            ));
        }
        out
    }
}

/// A single-line text input field: body, focus border, solid caret. Pure
/// geometry — the caller owns the string, measures it, draws the text, and
/// passes the caret's x offset from the text origin (physical px).
pub struct TextField {
    pub rect: Viewport,
    scale: f32,
}

impl TextField {
    pub fn new(rect: Viewport, scale: f32) -> Self {
        Self { rect, scale }
    }

    /// Left edge where the caller starts drawing the text.
    pub fn text_x(&self) -> f32 {
        self.rect.x + 14.0 * self.scale
    }

    pub fn rects(&self, focused: bool, caret_x: f32) -> Vec<UiRect> {
        let t = theme();
        let border = 3.0 * self.scale;
        let radius = 8.0 * self.scale;
        let edge = if focused { t.accent } else { t.seam };
        let mut out =
            vec![UiRect::region_rounded(self.rect, t.slider_track, radius).stroke(border, edge)];
        if focused {
            out.push(UiRect::region(
                Viewport {
                    x: self.text_x() + caret_x + 2.0 * self.scale,
                    y: self.rect.y + 8.0 * self.scale,
                    w: 2.5 * self.scale,
                    h: (self.rect.h - 16.0 * self.scale).max(1.0),
                },
                t.slider_thumb,
            ));
        }
        out
    }
}

/// A row of rounded color chips with a ring around the selected one.
/// Pure geometry + hit testing — the caller owns the palette and selection.
pub struct Swatches {
    chips: Vec<Viewport>,
}

impl Swatches {
    /// Lay out `count` square chips of `side` px from `(x, y)`, `gap` apart.
    pub fn new(x: f32, y: f32, side: f32, gap: f32, count: usize) -> Self {
        let chips = (0..count)
            .map(|i| Viewport {
                x: x + (side + gap) * i as f32,
                y,
                w: side,
                h: side,
            })
            .collect();
        Self { chips }
    }

    pub fn hit(&self, px: f32, py: f32) -> Option<usize> {
        self.chips.iter().position(|v| v.contains(px, py))
    }

    /// `colors` are linear RGB, one per chip (extras are skipped).
    pub fn rects(&self, colors: &[[f32; 3]], selected: Option<usize>) -> Vec<UiRect> {
        let t = theme();
        let mut out = Vec::with_capacity(self.chips.len());
        for (i, (&chip, &[r, g, b])) in self.chips.iter().zip(colors).enumerate() {
            // The selection ring rides outside the chip so the swatch shows
            // its full color, ring or no ring.
            let swatch = UiRect::region_rounded(chip, [r, g, b, 1.0], chip.w * 0.3);
            out.push(if selected == Some(i) {
                swatch.stroke_outer(chip.w * 0.12, t.slider_thumb)
            } else {
                swatch
            });
        }
        out
    }
}

/// An n-way segmented toggle: rounded track, accent-filled active segment.
/// Pure geometry — the caller draws the segment labels and owns the state.
pub struct Segmented {
    track: Viewport,
    pub segments: Vec<Viewport>,
}

impl Segmented {
    pub fn new(track: Viewport, count: usize, scale: f32) -> Self {
        let pad = 4.0 * scale;
        let n = count.max(1) as f32;
        let w = (track.w - pad * (n + 1.0)) / n;
        let segments = (0..count)
            .map(|i| Viewport {
                x: track.x + pad + (w + pad) * i as f32,
                y: track.y + pad,
                w,
                h: track.h - pad * 2.0,
            })
            .collect();
        Self { track, segments }
    }

    pub fn hit(&self, px: f32, py: f32) -> Option<usize> {
        self.segments.iter().position(|v| v.contains(px, py))
    }

    pub fn rects(&self, active: usize) -> Vec<UiRect> {
        let t = theme();
        let radius = self.track.h * 0.24;
        let mut out = vec![UiRect::region_rounded(self.track, t.slider_track, radius)];
        if let Some(&seg) = self.segments.get(active) {
            // Raised neutral well; the gold label carries the accent.
            out.push(UiRect::region_rounded(seg, srgb(0x3a3a3a), radius * 0.7));
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
        // Purple→gold fill that "reveals" as the value rises. Gold is
        // perceptually much brighter than deep purple, so a linear ramp reads
        // gold-dominated — bias hard toward purple and let gold arrive late.
        let gold = t.powf(2.5);
        let mut fill_end = th.grad_purple;
        for (f, g) in fill_end.iter_mut().zip(th.grad_gold) {
            *f += (g - *f) * gold;
        }
        vec![
            UiRect::region_rounded(track, th.slider_track, radius),
            UiRect::region_rounded_gradient(
                Viewport {
                    x: track.x,
                    y: track.y,
                    w: fill_w,
                    h: track.h,
                },
                th.grad_purple,
                fill_end,
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
