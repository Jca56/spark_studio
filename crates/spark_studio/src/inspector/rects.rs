//! The inspector's chrome, from the page's geometry: the colour section
//! (pinned, clipped to the panel) and the body (clipped to its window).

use spark_render::Viewport;
use spark_ui::{Slider, UiRect, surfaces, theme};

/// A reacting setting's mark: a small gold dot on its control.
fn dot(cx: f32, cy: f32, s: f32) -> UiRect {
    let r = 4.5 * s;
    UiRect::region_rounded(
        Viewport {
            x: cx - r,
            y: cy - r,
            w: r * 2.0,
            h: r * 2.0,
        },
        theme().accent,
        r,
    )
}

use super::build::SLIDER_LABEL_H;
use super::page::{HEADER_H, Hit, Page};
use super::popup::Slot;
use crate::props::swatch_grid;

/// A section header's triangle: down when open, right when folded.
fn triangle(header: Viewport, open: bool, s: f32) -> UiRect {
    let t = theme();
    let side = HEADER_H * s * 0.55;
    let tri = Viewport {
        x: header.x,
        y: header.y + (header.h - side) * 0.5,
        w: side,
        h: side,
    };
    let chevron = UiRect::chevron(tri, 2.5 * s, t.accent, 0.3);
    if open { chevron } else { chevron.rotate(0.75) }
}

impl Page {
    /// The pinned chrome: the header's triangle, the pair (background
    /// first, so the foreground overlaps it), the grid with the
    /// foreground's chip ringed, and the rule — clipped to the panel.
    pub fn pinned_rects(&self) -> Vec<UiRect> {
        let t = theme();
        let s = self.scale;
        let mut out = vec![triangle(self.header, self.color_open, s)];
        if self.color_open {
            let swatch = |r: Viewport, rgb: [f32; 3], lit: bool| {
                UiRect::region_rounded(r, [rgb[0], rgb[1], rgb[2], 1.0], 6.0 * s).stroke(
                    2.0 * s,
                    if lit { t.accent } else { t.card_border },
                )
            };
            out.push(swatch(self.bg, self.bg_rgb, self.popup_on == Some(Slot::Bg)));
            out.push(swatch(self.fg, self.fg_rgb, self.popup_on == Some(Slot::Fg)));
            for (i, (chip, rgb)) in self.grid.iter().zip(swatch_grid()).enumerate() {
                let r = UiRect::region_rounded(*chip, [rgb[0], rgb[1], rgb[2], 1.0], chip.w * 0.2);
                out.push(if self.grid_sel == Some(i) {
                    r.stroke_outer(2.0 * s, t.slider_thumb)
                } else {
                    r
                });
            }
        }
        out.push(UiRect::region(self.divider, t.card_border));
        out
    }

    /// The body's chrome, clipped to the body's window. `over` lights the
    /// field, name or button under the cursor; an edited box wears the
    /// accent.
    pub fn body_rects(&self, over: Option<Hit>) -> Vec<UiRect> {
        let t = theme();
        let s = self.scale;
        let m = surfaces();
        let mut out = Vec::new();
        if let Some(nb) = self.name_box
            && self.visible(nb)
        {
            out.push(if self.name_edit.is_some() {
                m.well.edged(nb, s, t.accent)
            } else if over == Some(Hit::Name) {
                m.well.edged(nb, s, t.accent_alt)
            } else {
                m.well.rect(nb, s)
            });
        }
        if let (Some((_, icon, tint)), Some(g)) = (&self.title, self.glyph)
            && self.visible(g)
        {
            out.push(UiRect::icon_sized(g, *icon, 2.0 * s, *tint, 0.4));
        }
        for sec in &self.sections {
            if self.visible(sec.header) {
                out.push(triangle(sec.header, sec.open, s));
            }
            // The rule down the content's left — only the part in view.
            if sec.open && sec.rule.h > 0.0 && self.visible(sec.rule) {
                let top = sec.rule.y.max(self.body.y);
                let bottom = (sec.rule.y + sec.rule.h).min(self.body.y + self.body.h);
                out.push(UiRect::region(
                    Viewport {
                        x: sec.rule.x,
                        y: top,
                        w: sec.rule.w,
                        h: (bottom - top).max(0.0),
                    },
                    t.card_border,
                ));
            }
        }
        for (k, f) in self.fields.iter().enumerate() {
            if !self.visible(f.rect) {
                continue;
            }
            let editing = self.edit.as_ref().is_some_and(|(slot, _)| *slot == k);
            out.push(if editing {
                m.well.edged(f.rect, s, t.accent)
            } else if over == Some(Hit::Field(k)) {
                m.well.edged(f.rect, s, t.accent_alt)
            } else {
                m.well.rect(f.rect, s)
            });
            // The dot: this setting rides the track.
            if f.reacts {
                out.push(dot(f.rect.x + f.rect.w - 9.0 * s, f.rect.y + 9.0 * s, s));
            }
        }
        for sl in &self.sliders {
            if self.visible(sl.hit) {
                out.extend(Slider::rects(sl.track, sl.v));
                if sl.reacts {
                    out.push(dot(
                        sl.hit.x + sl.hit.w + 10.0 * s,
                        sl.label_y + SLIDER_LABEL_H * s * 0.5,
                        s,
                    ));
                }
            }
        }
        for sw in &self.switches {
            if sw.seg.segments.first().is_some_and(|r| self.visible(*r)) {
                out.extend(sw.seg.rects(sw.active));
            }
        }
        for c in &self.checks {
            if self.visible(c.check.row) {
                out.extend(c.check.rects(c.on, s));
            }
        }
        for c in &self.chips {
            if self.visible(c.rect) {
                out.push(
                    UiRect::region_rounded(c.rect, [c.rgb[0], c.rgb[1], c.rgb[2], 1.0], 6.0 * s)
                        .stroke(2.0 * s, t.card_border),
                );
            }
        }
        for (k, b) in self.buttons.iter().enumerate() {
            if self.visible(b.rect) {
                out.push(if over == Some(Hit::Button(k)) {
                    m.plate.filled(t.button_hover).rect(b.rect, s)
                } else {
                    m.plate.rect(b.rect, s)
                });
            }
        }
        out
    }
}
