//! Reusable SparkUI widgets: icon bars, color swatches, segmented toggles,
//! sliders. All pure geometry + hit testing — callers own state and text.

use spark_render::Viewport;

use crate::rect::UiRect;
use crate::theme::{surfaces, theme};

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
                out.push(UiRect::region(r, t.accent_alt_bg));
            } else if is_hover {
                out.push(UiRect::region(r, t.button_hover));
            }
            let fg = if is_active {
                // Gold glyph on the purple highlight — Spark's two accents.
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
            vec![UiRect::region_rounded(self.anchor, t.accent_alt_bg, radius)]
        } else if hover {
            vec![UiRect::region_rounded(self.anchor, t.button_hover, radius)]
        } else {
            Vec::new()
        }
    }

    /// The floating panel: border, body, and the hovered row's highlight.
    /// Append these after everything else — menus draw on top.
    pub fn panel_rects(&self, hover: Option<usize>) -> Vec<UiRect> {
        let mut out = vec![surfaces().float.rect(self.panel, self.scale)];
        if let Some(i) = hover
            && let Some(&row) = self.items.get(i)
        {
            out.push(surfaces().hover.rect(row, self.scale));
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
        let field = surfaces().field;
        let mut out = vec![if focused {
            field.edged(self.rect, self.scale, t.accent_alt)
        } else {
            field.rect(self.rect, self.scale)
        }];
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
            out.push(UiRect::region_rounded(seg, t.segment_on, radius * 0.7));
        }
        out
    }
}

/// A square on/off box with its label beside it.
///
/// The compact form of a two-state choice, and the right form when one of
/// the two states is simply "not the other". A segmented pair spends a
/// whole row of the card saying `Normal | Additive`, where `Normal` carries
/// no information at all — it is the absence of Additive. A box you tick
/// says the same thing in a quarter of the width.
///
/// The tick is two capsules from the material renderer rather than a new
/// shader glyph, so it inherits colour and scale like everything else does.
pub struct Checkbox {
    /// The box itself.
    pub square: Viewport,
    /// Box *and* label: the whole row is the target, because a 26px square
    /// is not a target — it is a thing you miss.
    pub row: Viewport,
    pub label_pos: [f32; 2],
}

impl Checkbox {
    /// Lay one out at `(x, y)` spanning `w`, with a box `side` px on a side.
    pub fn new(x: f32, y: f32, w: f32, side: f32, scale: f32) -> Self {
        let square = Viewport {
            x,
            y,
            w: side,
            h: side,
        };
        Self {
            square,
            row: Viewport { x, y, w, h: side },
            label_pos: [x + side + 12.0 * scale, y],
        }
    }

    pub fn hit(&self, px: f32, py: f32) -> bool {
        self.row.contains(px, py)
    }

    pub fn rects(&self, on: bool, scale: f32) -> Vec<UiRect> {
        let t = theme();
        let radius = self.square.h * 0.24;
        // Ticked, the box fills with the accent and the check is cut in the
        // panel colour; empty, it is a well with an edge, so an unchecked
        // box still reads as something you can click rather than as a gap.
        let mut out = vec![
            UiRect::region_rounded(self.square, if on { t.accent } else { t.well }, radius)
                .stroke(2.0 * scale, if on { t.accent } else { t.card_border }),
        ];
        if on {
            let (x, y, s) = (self.square.x, self.square.y, self.square.h);
            let thick = (s * 0.15).max(2.0 * scale);
            // Short stroke down into the corner, long stroke back up.
            out.push(UiRect::line(
                [x + s * 0.26, y + s * 0.52],
                [x + s * 0.44, y + s * 0.70],
                thick,
                t.panel,
            ));
            out.push(UiRect::line(
                [x + s * 0.44, y + s * 0.70],
                [x + s * 0.76, y + s * 0.30],
                thick,
                t.panel,
            ));
        }
        out
    }
}

/// A horizontal slider: rounded track, accent fill, round thumb.
/// Pure geometry — the caller owns the value mapping and drag state.
pub struct Slider;

impl Slider {
    /// The thumb's diameter, as a multiple of the track's height.
    const THUMB: f32 = 2.2;

    pub fn thumb_side(track: Viewport) -> f32 {
        track.h * Self::THUMB
    }

    /// The value under the cursor. The thumb's travel is inset by half its
    /// own width at each end so it never hangs off the track, and this has
    /// to use the same inset or the thumb would not sit under the cursor.
    pub fn t_at(track: Viewport, mx: f32) -> f32 {
        let side = Self::thumb_side(track);
        let travel = (track.w - side).max(0.001);
        ((mx - track.x - side * 0.5) / travel).clamp(0.0, 1.0)
    }

    pub fn rects(track: Viewport, t: f32) -> Vec<UiRect> {
        let th = theme();
        let t = t.clamp(0.0, 1.0);
        let radius = track.h * 0.5;
        let side = Self::thumb_side(track);
        // Inset travel: at 0 the thumb sits *on* the left end of the track
        // rather than half-way off it, which used to push it clean out of
        // whatever panel the slider lived in.
        let cx = track.x + side * 0.5 + (track.w - side).max(0.0) * t;
        let fill_w = (cx - track.x).max(track.h);
        // Purple→gold fill that "reveals" as the value rises. Gold is
        // perceptually much brighter than deep purple, so a linear ramp reads
        // gold-dominated — bias hard toward purple and let gold arrive late.
        let gold = t.powf(2.5);
        let [from, to] = th.slider_fill;
        let mut fill_end = from;
        for (f, g) in fill_end.iter_mut().zip(to) {
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
                from,
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

#[cfg(test)]
mod tests {
    use super::*;

    fn track() -> Viewport {
        Viewport {
            x: 100.0,
            y: 50.0,
            w: 300.0,
            h: 20.0,
        }
    }

    /// The thumb used to be centred at `track.x + track.w * t`, so at 0 half
    /// of it hung off the left of the track — and off the panel, and in the
    /// playground clean off the window. It has to stay inside its own track.
    #[test]
    fn the_thumb_never_leaves_its_track() {
        let tr = track();
        let side = Slider::thumb_side(tr);
        for t in [0.0, 0.25, 0.5, 0.75, 1.0] {
            let thumb = Slider::rects(tr, t)[2];
            assert!(
                thumb.pos[0] >= tr.x - 0.01,
                "t={t}: thumb starts at {}, track at {}",
                thumb.pos[0],
                tr.x
            );
            assert!(
                thumb.pos[0] + side <= tr.x + tr.w + 0.01,
                "t={t}: thumb ends past the track"
            );
        }
    }

    /// Drawing and hit testing have to agree, or the thumb slides out from
    /// under the cursor as you drag toward either end.
    #[test]
    fn the_cursor_lands_on_the_thumb() {
        let tr = track();
        let side = Slider::thumb_side(tr);
        for t in [0.0, 0.3, 0.5, 0.9, 1.0] {
            let centre = Slider::rects(tr, t)[2].pos[0] + side * 0.5;
            let back = Slider::t_at(tr, centre);
            assert!((back - t).abs() < 0.001, "t={t} read back as {back}");
        }
    }

    #[test]
    fn the_value_clamps_outside_the_track() {
        let tr = track();
        assert_eq!(Slider::t_at(tr, tr.x - 500.0), 0.0);
        assert_eq!(Slider::t_at(tr, tr.x + tr.w + 500.0), 1.0);
    }

    /// The box is the small part; the row is the target. A 30px square
    /// asks to be missed — the label beside it is part of the same click.
    #[test]
    fn a_checkbox_is_bigger_than_its_box() {
        let c = Checkbox::new(100.0, 50.0, 300.0, 30.0, 1.0);
        assert_eq!(c.square.w, 30.0);
        assert_eq!(c.row.w, 300.0, "the row is only as wide as the box");
        assert!(c.hit(112.0, 60.0), "a click on the box missed");
        assert!(c.hit(280.0, 60.0), "a click on the label missed");
        assert!(!c.hit(500.0, 60.0), "a click past the row landed");
        assert!(!c.hit(112.0, 200.0), "a click below the row landed");
        // The label starts clear of the box, or the words sit on the tick.
        assert!(c.label_pos[0] >= c.square.x + c.square.w);
    }

    /// Ticked and empty have to be tellable apart by more than a colour:
    /// the tick is two capsules that only exist when it is on.
    #[test]
    fn a_ticked_box_draws_a_tick() {
        let c = Checkbox::new(0.0, 0.0, 200.0, 30.0, 1.0);
        let off = c.rects(false, 1.0);
        let on = c.rects(true, 1.0);
        assert_eq!(off.len(), 1, "an empty box drew more than its own square");
        assert_eq!(on.len(), 3, "a ticked box is the square plus two strokes");
        assert_ne!(off[0].color, on[0].color, "the box fill did not change");
        // Every part of the tick stays inside the box it belongs to.
        for r in &on[1..] {
            assert!(
                r.pos[0] >= c.square.x && r.pos[0] + r.size[0] <= c.square.x + c.square.w,
                "the tick escaped its box"
            );
        }
    }
}
