//! The cursor that stacks the inspector's body: sections with their
//! headers and left rules, and the rows inside them — fields, switches,
//! sliders, checkboxes, colour chips, buttons. Every widget it lays out
//! lands in the page's typed slot lists; the page only orchestrates.

use spark_render::Viewport;
use spark_ui::{Checkbox, Segmented, Slider};

use super::field;
use super::page::{
    ButtonKind, ButtonSlot, CheckKind, CheckSlot, ChipSlot, EditKey, FieldSlot, Page, SectionKey,
    SectionSlot, SliderSlot, SliderTarget, SwitchKind, SwitchSlot,
};
use crate::props::Prop;
use crate::textbox::TextBox;

/// A section header row, and the air under it.
pub(super) const SECTION_H: f32 = 36.0;
const SECTION_GAP: f32 = 8.0;
/// Content indents under a section's left rule by this much.
pub(super) const INDENT: f32 = 16.0;
/// Air after a section's last row.
const SECTION_END: f32 = 14.0;
/// A field row: its caption line, the box, and the air after.
pub(super) const CAPTION_H: f32 = 24.0;
const FIELD_H: f32 = 46.0;
const FIELD_ROW_H: f32 = 80.0;
const FIELD_GAP: f32 = 10.0;
/// A slider row: its label line, the thumb's band, and the air after.
pub(super) const SLIDER_LABEL_H: f32 = 24.0;
const SLIDER_TRACK_H: f32 = 15.0;
const SLIDER_ROW_H: f32 = 64.0;
/// A switch row, a checkbox row and a button row, with their air.
const SWITCH_H: f32 = 46.0;
const CHECK_SIDE: f32 = 30.0;
const CHECK_ROW_H: f32 = 48.0;
pub(super) const BUTTON_H: f32 = 44.0;
const BUTTON_ROW_H: f32 = 54.0;
/// A colour chip row: the chip's side and the row's height.
pub(super) const CHIP: f32 = 34.0;
const CHIP_ROW_H: f32 = 46.0;
const GAP: f32 = 10.0;

/// Where the next row goes, and the section it goes into.
pub struct Cursor<'a> {
    pub page: &'a mut Page,
    pub s: f32,
    /// The current content column — inset while inside a section.
    pub x0: f32,
    pub w: f32,
    pub y: f32,
    /// The column outside any section.
    base_x0: f32,
    base_w: f32,
    /// The open section's index, and where its content began.
    section: Option<(usize, f32)>,
}

impl<'a> Cursor<'a> {
    pub fn new(page: &'a mut Page, s: f32, x0: f32, w: f32, y: f32) -> Self {
        Self {
            page,
            s,
            x0,
            w,
            y,
            base_x0: x0,
            base_w: w,
            section: None,
        }
    }

    /// Open a section: its header row, then — if it is unfolded — the
    /// content column steps in under the rule. Returns whether to lay the
    /// content out.
    pub fn section(&mut self, key: SectionKey, title: &str, open: bool) -> bool {
        let s = self.s;
        let header = Viewport {
            x: self.base_x0,
            y: self.y,
            w: self.base_w,
            h: SECTION_H * s,
        };
        self.y += (SECTION_H + SECTION_GAP) * s;
        let idx = self.page.sections.len();
        self.page.sections.push(SectionSlot {
            key,
            title: spaced(title),
            header,
            open,
            rule: Viewport {
                x: self.base_x0 + 4.0 * s,
                y: self.y,
                w: 2.0 * s,
                h: 0.0,
            },
        });
        if open {
            self.x0 = self.base_x0 + INDENT * s;
            self.w = self.base_w - INDENT * s;
            self.section = Some((idx, self.y));
        }
        open
    }

    /// Close the section: the rule runs the height its content took, the
    /// column steps back out, and a little air follows.
    /// Mark the fields and sliders whose setting rides the track — the
    /// dot on the control. `glow` is the Glow effect's id, since its
    /// slider keys the effect's radius.
    pub fn mark_reactions(&mut self, on: &dyn Fn(crate::anim::Target) -> bool, glow: Option<u32>) {
        use crate::anim::Target;
        for f in &mut self.page.fields {
            f.reacts = on(Target::Shape(f.prop));
        }
        for sl in &mut self.page.sliders {
            sl.reacts = match sl.target {
                SliderTarget::Prop(crate::props::Prop::Glow) => {
                    glow.is_some_and(|id| on(Target::Effect { id, param: 0 }))
                }
                SliderTarget::Prop(p) => on(Target::Shape(p)),
                SliderTarget::Effect { id, param } => on(Target::Effect {
                    id,
                    param: param as u8,
                }),
            };
        }
    }

    pub fn end_section(&mut self) {
        if let Some((idx, top)) = self.section.take() {
            let h = (self.y - top - GAP * self.s).max(0.0);
            self.page.sections[idx].rule.h = h;
            self.x0 = self.base_x0;
            self.w = self.base_w;
        }
        self.y += SECTION_END * self.s;
    }

    /// A row of scrub fields, `present` across; the row closes up around
    /// what the object has. The field being typed into is matched here.
    pub fn field_row(
        &mut self,
        present: &[(Prop, &'static str, f32)],
        edit: Option<&(EditKey, TextBox)>,
    ) {
        if present.is_empty() {
            return;
        }
        let s = self.s;
        let cols = present.len() as f32;
        let box_w = (self.w - FIELD_GAP * s * (cols - 1.0)) / cols;
        for (k, &(prop, cap, v)) in present.iter().enumerate() {
            let rect = Viewport {
                x: self.x0 + (box_w + FIELD_GAP * s) * k as f32,
                y: self.y + CAPTION_H * s,
                w: box_w,
                h: FIELD_H * s,
            };
            let shown = field::shown(prop, v);
            let slot = self.page.fields.len();
            if let Some((EditKey::Prop(p), tb)) = edit
                && *p == prop
            {
                self.page.edit = Some((slot, tb.clone()));
            }
            self.page.fields.push(FieldSlot {
                reacts: false,
                prop,
                caption: cap,
                col: k,
                rect,
                shown,
                text: field::format(shown),
            });
        }
        self.y += FIELD_ROW_H * s;
    }

    pub fn switch(&mut self, kind: SwitchKind, labels: &'static [&'static str], active: usize) {
        let s = self.s;
        let track = Viewport {
            x: self.x0,
            y: self.y,
            w: self.w,
            h: SWITCH_H * s,
        };
        self.page.switches.push(SwitchSlot {
            kind,
            seg: Segmented::new(track, labels.len(), s),
            labels,
            active,
        });
        self.y += (SWITCH_H + GAP) * s;
    }

    /// A slider for `target`, showing `value` inside `range`.
    pub fn slider(
        &mut self,
        target: SliderTarget,
        label: &'static str,
        value: f32,
        range: (f32, f32),
        readout: String,
    ) {
        let s = self.s;
        let track_h = SLIDER_TRACK_H * s;
        let thumb = Slider::thumb_side(Viewport {
            x: 0.0,
            y: 0.0,
            w: 1.0,
            h: track_h,
        });
        let band_y = self.y + SLIDER_LABEL_H * s;
        let (lo, hi) = range;
        self.page.sliders.push(SliderSlot {
            reacts: false,
            target,
            label,
            track: Viewport {
                x: self.x0,
                y: band_y + (thumb - track_h) * 0.5,
                w: self.w,
                h: track_h,
            },
            hit: Viewport {
                x: self.x0,
                y: band_y,
                w: self.w,
                h: thumb,
            },
            label_y: self.y,
            range,
            v: ((value - lo) / (hi - lo).max(1e-6)).clamp(0.0, 1.0),
            readout,
        });
        self.y += SLIDER_ROW_H * s;
    }

    pub fn check(&mut self, kind: CheckKind, label: &'static str, on: bool) {
        let s = self.s;
        self.page.checks.push(CheckSlot {
            kind,
            check: Checkbox::new(
                self.x0,
                self.y + (CHECK_ROW_H - CHECK_SIDE) * 0.5 * s,
                self.w,
                CHECK_SIDE * s,
                s,
            ),
            label,
            on,
        });
        self.y += CHECK_ROW_H * s;
    }

    /// A colour chip with a caption beside it.
    pub fn chip(&mut self, id: u32, param: usize, rgb: [f32; 3], label: &'static str) {
        let s = self.s;
        self.page.chips.push(ChipSlot {
            id,
            param,
            rect: Viewport {
                x: self.x0,
                y: self.y + (CHIP_ROW_H - CHIP) * 0.5 * s,
                w: CHIP * s,
                h: CHIP * s,
            },
            rgb,
            label,
        });
        self.y += CHIP_ROW_H * s;
    }

    /// A full-width button row.
    pub fn button(&mut self, kind: ButtonKind, label: String) {
        let s = self.s;
        self.page.buttons.push(ButtonSlot {
            kind,
            rect: Viewport {
                x: self.x0,
                y: self.y + (BUTTON_ROW_H - BUTTON_H) * 0.5 * s,
                w: self.w,
                h: BUTTON_H * s,
            },
            label,
        });
        self.y += BUTTON_ROW_H * s;
    }
}

/// A header's word, letter-spaced caps the way Lantern Studio's are:
/// "Style" → "S T Y L E".
pub fn spaced(title: &str) -> String {
    let mut out = String::new();
    for (i, c) in title.chars().enumerate() {
        if i > 0 {
            out.push(' ');
        }
        out.extend(c.to_uppercase());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn headers_are_letter_spaced_caps() {
        assert_eq!(spaced("Style"), "S T Y L E");
        assert_eq!(spaced("Color"), "C O L O R");
        assert_eq!(spaced("React"), "R E A C T");
        assert_eq!(spaced(""), "");
    }
}
