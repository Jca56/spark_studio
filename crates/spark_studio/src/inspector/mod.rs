//! The inspector: the right panel, where an object's base state is
//! edited — ⑤ of the object/clip build order, first cut (Alva's calls,
//! 2026-08-31): **transform, colour and look**; effects and audio-react
//! next.
//!
//! **The colour section** is pinned at the top, Lantern Studio's way:
//! the foreground/background pair at the left, the swatch grid beside
//! it, a rule under both. Foreground is the colour you draw and paint
//! with — a click on a grid chip sets it, and it paints the selection;
//! background is the second colour — a right-click on a chip sets it,
//! and it paints a selected shape's gradient end when it has one.
//! Right-click the pair to swap them; left-click a swatch to open the
//! colour popup on it (`popup`, routed in `colour`). Nothing selected:
//! the colour section alone.
//!
//! Below it, for the primary selection: scrub fields for the numbers
//! with no ceiling (place, aim, size — drag to scrub, click to type),
//! captions coloured by the axis they move so they match the gizmo's
//! arrows and rings; sliders for the bounded ones; the kind's own
//! switch; and Additive. Sliders and fields edit the *primary*; colour,
//! the switches and Additive paint the whole selection — the editor's
//! own rules, unchanged. A scrub or a picker drag is one undo step.

mod build;
mod colour;
mod field;
mod keyboard;
mod labels;
mod page;
mod popup;
mod react;
mod rects;
mod sections;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_body;

pub use build::spaced;
pub use field::{ROWS, format as fmt_number, is_angle};
pub use sections::style_specs;
pub use page::fmt_param;
pub use colour::{hsv_of, rgb_of, with_channel};
pub use page::{Hit, Page, SectionKey, SliderTarget};
pub use popup::Slot;

use spark_render::Viewport;
use spark_ui::Slider;

use crate::Studio;
use crate::chrome::Label;
use crate::props::{Prop, swatch_grid};
use crate::textbox::TextBox;

/// A drag on the inspector or its popup.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Drag {
    /// The popup's square and hue bar.
    Sv,
    Hue,
    /// A scrub: which field, where the press was, what it showed then,
    /// and whether the cursor has travelled — a press that never does is
    /// a click, which opens the field for typing.
    Scrub {
        slot: usize,
        start_y: f32,
        start: f32,
        moved: bool,
    },
    Slider(usize),
    /// The React popup's intensity slider.
    ReactAmount,
}

/// What a field being typed into is for: a number on the object, or the
/// popup's hex code or one of its channels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditKey {
    Prop(Prop),
    Hex,
    Chan(usize),
    /// The object's name.
    Name,
}

/// The inspector's own state, on the studio.
pub struct State {
    /// How far the body is scrolled up, physical px.
    pub scroll: f32,
    /// The popup's HSV — keeps its hue through a grey while it is the
    /// one that wrote the colour.
    pub hsv: [f32; 3],
    pub over: Option<Hit>,
    pub drag: Option<Drag>,
    /// A field being typed into, and the buffer.
    pub edit: Option<(EditKey, TextBox)>,
    /// The edited field's caret table — every char boundary's x — cached
    /// by the frame that measured it, for the next click.
    pub caret_xs: Vec<(usize, f32)>,
    /// The colour popup, on the swatch it was opened on.
    pub popup: Option<Slot>,
    /// The React popup, on the setting it was opened on (see `react`),
    /// and what is under the cursor on it.
    pub react: Option<react::Open>,
    pub react_over: Option<react::ReactHit>,
    /// Whether the colour section is unfolded under its header.
    pub color_open: bool,
    /// The body sections folded under their headers.
    pub folded: Vec<SectionKey>,
}

impl State {
    pub fn new() -> Self {
        Self {
            scroll: 0.0,
            hsv: [0.0; 3],
            over: None,
            drag: None,
            edit: None,
            caret_xs: Vec::new(),
            popup: None,
            react: None,
            react_over: None,
            color_open: true,
            folded: Vec::new(),
        }
    }
}

/// A field's box, its text origin and font size — for the caret the
/// frame draws once the text engine has measured the text.
pub type EditBox = Option<(Viewport, f32, f32)>;

/// What the frame draws for the inspector: the pinned colour section,
/// the scrolled body (clipped to its window), the words, the edited
/// field — and the popup, which floats over everything.
pub struct Frame {
    pub pinned: Vec<spark_ui::UiRect>,
    pub body: Vec<spark_ui::UiRect>,
    pub body_clip: Viewport,
    pub labels: Vec<Label>,
    pub edit_box: EditBox,
    pub popup: Option<(Vec<spark_ui::UiRect>, Vec<Label>, EditBox)>,
}

impl Studio {
    /// The page for this frame's layout and state.
    fn inspector_page(&self, panel: Viewport) -> Page {
        Page::build(
            panel,
            self.scale(),
            &self.editor,
            self.inspector.scroll,
            self.inspector.edit.as_ref(),
            self.inspector.popup,
            self.inspector.color_open,
            &self.inspector.folded,
        )
    }

    /// Housekeeping before a frame: the popup's picker follows its
    /// swatch's colour when something else moved it (`C`, the eyedropper,
    /// a chip), and the scroll stays inside the content.
    pub(crate) fn inspector_tick(&mut self, panel: Viewport) {
        let c = self.slot_colour(self.inspector.popup.unwrap_or(Slot::Fg));
        if !colour::same_colour(rgb_of(self.inspector.hsv), c) {
            self.inspector.hsv = hsv_of(c);
        }
        let max = self.inspector_page(panel).max_scroll();
        self.inspector.scroll = self.inspector.scroll.clamp(0.0, max);
    }

    /// Everything the frame draws for the inspector.
    pub(crate) fn inspector_frame(&self, panel: Viewport) -> Frame {
        let page = self.inspector_page(panel);
        let dragging = match self.inspector.drag {
            Some(Drag::Slider(k)) => Some(k),
            _ => None,
        };
        let s = self.scale();
        let edit_box = page
            .edit
            .as_ref()
            .and_then(|(slot, _)| {
                page.fields
                    .get(*slot)
                    .map(|f| (f.rect, f.rect.x + 14.0 * s, crate::chrome::UI_TEXT * s))
            })
            .or_else(|| {
                // The name being typed: its box, at the name's size.
                page.name_edit
                    .as_ref()
                    .and(page.name_box)
                    .map(|nb| (nb, nb.x + 14.0 * s, page::NAME_TEXT * s))
            });
        let popup = self
            .popup_for()
            .map(|p| (p.rects(), p.labels(), p.edit_box()))
            .or_else(|| {
                self.react_page()
                    .map(|p| (p.rects(self.inspector.react_over), p.labels(), None))
            });
        Frame {
            pinned: page.pinned_rects(),
            body: page.body_rects(self.inspector.over),
            body_clip: page.body,
            labels: page.labels(self.inspector.over, dragging),
            edit_box,
            popup,
        }
    }

    /// A left press in the right panel. A swatch opens (or closes) the
    /// popup on itself; a grid chip sets the foreground; a field starts
    /// a scrub that becomes a click if it never travels; a slider jumps
    /// to the cursor and follows; a switch or a checkbox flips. A press
    /// anywhere on the panel first commits a field being typed into.
    /// True when the frame needs redrawing.
    pub(crate) fn inspector_press(&mut self, panel: Viewport, cx: f32, cy: f32) -> bool {
        let page = self.inspector_page(panel);
        let hit = page.hit(cx, cy);
        // A click inside the field being edited places the caret; any
        // other click commits it first.
        let in_edited = match (&page.edit, hit) {
            (Some((slot, _)), Some(Hit::Field(k))) => *slot == k,
            _ => page.name_edit.is_some() && hit == Some(Hit::Name),
        };
        if in_edited {
            let at = crate::textbox::index_at(&self.inspector.caret_xs, cx);
            if let Some((_, tb)) = &mut self.inspector.edit {
                tb.place(at);
            }
            return true;
        }
        let mut dirty = self.inspector_commit();
        match hit {
            Some(Hit::ColorHeader) => {
                // Fold or unfold the colour section; a popup on a swatch
                // that just vanished goes with it.
                self.inspector.color_open = !self.inspector.color_open;
                if !self.inspector.color_open {
                    self.inspector.popup = None;
                }
                dirty = true;
            }
            Some(Hit::Name) => {
                // The given name, not the auto-label: an unnamed object
                // opens empty, and typing names it.
                let given = self
                    .editor
                    .primary()
                    .map(|i| self.editor.name(i).to_string())
                    .unwrap_or_default();
                self.inspector.edit = Some((EditKey::Name, TextBox::selecting_all(given)));
                dirty = true;
            }
            Some(Hit::Fg) | Some(Hit::Bg) => {
                let slot = if hit == Some(Hit::Fg) { Slot::Fg } else { Slot::Bg };
                self.inspector.popup = if self.inspector.popup == Some(slot) {
                    None
                } else {
                    Some(slot)
                };
                dirty = true;
            }
            Some(Hit::Chip(i)) => {
                if let Some(&rgb) = swatch_grid().get(i) {
                    self.set_slot_colour(Slot::Fg, rgb);
                    dirty = true;
                }
            }
            Some(Hit::Field(k)) => {
                if let Some(f) = page.fields.get(k) {
                    self.inspector.drag = Some(Drag::Scrub {
                        slot: k,
                        start_y: cy,
                        start: f.shown,
                        moved: false,
                    });
                    // With the clip view open, touching a setting here
                    // lists it there — the inspector is the picker.
                    dirty |= self.clip_view_arm(crate::anim::Target::Shape(f.prop));
                }
            }
            Some(Hit::Section(k)) => {
                // Fold or unfold a body section.
                if let Some(sec) = page.sections.get(k) {
                    let key = sec.key;
                    match self.inspector.folded.iter().position(|f| *f == key) {
                        Some(at) => {
                            self.inspector.folded.remove(at);
                        }
                        None => self.inspector.folded.push(key),
                    }
                    dirty = true;
                }
            }
            Some(Hit::Slider(k)) => {
                if let Some(sl) = page.sliders.get(k) {
                    self.inspector_slider_to(sl.target, sl.range, Slider::t_at(sl.track, cx));
                    self.inspector.drag = Some(Drag::Slider(k));
                    // Listed in the clip view too, once the press has
                    // made sure the effect exists (a Glow slider's first
                    // touch adds the Glow effect).
                    if let Some(t) = self.slider_key_target(sl.target) {
                        self.clip_view_arm(t);
                    }
                    dirty = true;
                }
            }
            Some(Hit::FxChip(k)) => {
                // The chip takes the background colour — the gradient's
                // far end is what the background paints.
                if page.chips.get(k).is_some() {
                    let bg = self.editor.color_b();
                    dirty |= self.editor.set_color_b(bg);
                }
            }
            Some(Hit::Button(k)) => {
                if let Some(b) = page.buttons.get(k)
                    && let Some(i) = self.editor.primary()
                {
                    dirty |= match b.kind {
                        page::ButtonKind::RemoveEffect(id) => self.editor.remove_effect(i, id),
                    };
                }
            }
            Some(Hit::Switch(w, i)) => {
                if let Some(sw) = page.switches.get(w) {
                    dirty |= match sw.kind {
                        page::SwitchKind::FillOutline => self.editor.set_outline(i == 1),
                        page::SwitchKind::StarForm => self.editor.set_star_form(i),
                        page::SwitchKind::LightKind => self.editor.set_light_kind(i),
                    };
                }
            }
            Some(Hit::Check(k)) => {
                if let Some(c) = page.checks.get(k) {
                    dirty |= match c.kind {
                        page::CheckKind::Additive => self.editor.set_additive(!c.on),
                        page::CheckKind::EffectOn(id) => match self.editor.primary() {
                            Some(i) => self.editor.toggle_effect(i, id),
                            None => false,
                        },
                    };
                }
            }
            None => {}
        }
        dirty
    }

    /// What a slider keys: a property, or an effect's parameter — Glow's
    /// slider keys the Glow effect's radius, which has to exist first.
    fn slider_key_target(&self, target: SliderTarget) -> Option<crate::anim::Target> {
        use crate::anim::Target;
        match target {
            SliderTarget::Prop(Prop::Glow) => {
                let i = self.editor.primary()?;
                let g = self
                    .editor
                    .fx_of(i)
                    .find_kind(crate::fx::EffectKind::Glow)?;
                Some(Target::Effect { id: g.id, param: 0 })
            }
            SliderTarget::Prop(p) => Some(Target::Shape(p)),
            SliderTarget::Effect { id, param } => Some(Target::Effect {
                id,
                param: param as u8,
            }),
        }
    }

    /// A right press in the right panel: on the pair, foreground and
    /// background swap; on a grid chip, it becomes the background. True
    /// when the press was the inspector's — otherwise the context menu
    /// may have it.
    pub(crate) fn inspector_right_press(&mut self, panel: Viewport, cx: f32, cy: f32) -> bool {
        let page = self.inspector_page(panel);
        match page.hit(cx, cy) {
            Some(Hit::Fg) | Some(Hit::Bg) => {
                self.editor.swap_colors();
                true
            }
            Some(Hit::Chip(i)) => {
                if let Some(&rgb) = swatch_grid().get(i) {
                    self.set_slot_colour(Slot::Bg, rgb);
                }
                true
            }
            // A setting's control: the React popup opens beside it.
            Some(Hit::Field(k)) => {
                if let Some(f) = page.fields.get(k) {
                    self.open_react(crate::anim::Target::Shape(f.prop), f.rect);
                }
                true
            }
            Some(Hit::Slider(k)) => {
                if let Some(sl) = page.sliders.get(k) {
                    match self.slider_key_target(sl.target) {
                        Some(t) => self.open_react(t, sl.hit),
                        None => {
                            self.export_note =
                                Some("give it some Glow first, then React on it".to_string())
                        }
                    }
                }
                true
            }
            _ => false,
        }
    }

    /// The cursor moved: a drag follows it, otherwise what is under it
    /// lights. True when the frame needs redrawing.
    pub(crate) fn inspector_moved(&mut self, panel: Viewport, mx: f32, my: f32) -> bool {
        match self.inspector.drag {
            Some(Drag::Sv) => {
                let Some(p) = self.popup_for() else {
                    return false;
                };
                let (s, v) = p.picker.sv_at(mx, my);
                self.inspector_set_hsv([self.inspector.hsv[0], s, v]);
                true
            }
            Some(Drag::Hue) => {
                let Some(p) = self.popup_for() else {
                    return false;
                };
                let h = p.picker.hue_at(my);
                self.inspector_set_hsv([h, self.inspector.hsv[1], self.inspector.hsv[2]]);
                true
            }
            Some(Drag::Scrub {
                slot,
                start_y,
                start,
                moved,
            }) => {
                let dy = my - start_y;
                if !moved && dy.abs() < 3.0 * self.scale() {
                    return false;
                }
                let page = self.inspector_page(panel);
                let Some(f) = page.fields.get(slot) else {
                    return false;
                };
                let prop = f.prop;
                let shown =
                    field::scrubbed(prop, start, dy, self.scale(), self.modifiers.shift_key());
                self.inspector.drag = Some(Drag::Scrub {
                    slot,
                    start_y,
                    start,
                    moved: true,
                });
                self.inspector_field_to(prop, shown)
            }
            Some(Drag::Slider(k)) => {
                let page = self.inspector_page(panel);
                if let Some(sl) = page.sliders.get(k) {
                    self.inspector_slider_to(sl.target, sl.range, Slider::t_at(sl.track, mx));
                }
                true
            }
            Some(Drag::ReactAmount) => self.react_amount_at(mx),
            None => {
                let over = if panel.contains(mx, my) && !self.popup_contains(mx, my) {
                    self.inspector_page(panel).hit(mx, my)
                } else {
                    None
                };
                let react_over = self.react_page().and_then(|p| p.hit(mx, my));
                let dirty = over != self.inspector.over || react_over != self.inspector.react_over;
                self.inspector.over = over;
                self.inspector.react_over = react_over;
                dirty
            }
        }
    }

    /// The button came up: a scrub that never travelled opens its field
    /// for typing, with the number selected so typing replaces it; every
    /// other drag simply ends. True when the frame needs redrawing.
    pub(crate) fn inspector_release(&mut self, panel: Viewport) -> bool {
        match self.inspector.drag.take() {
            Some(Drag::Scrub {
                slot, moved: false, ..
            }) => {
                let page = self.inspector_page(panel);
                if let Some(f) = page.fields.get(slot) {
                    self.inspector.edit = Some((
                        EditKey::Prop(f.prop),
                        TextBox::selecting_all(f.text.clone()),
                    ));
                }
                true
            }
            Some(_) => true,
            None => false,
        }
    }

    /// Write a field's shown number onto the primary, fitted.
    fn inspector_field_to(&mut self, prop: Prop, shown: f32) -> bool {
        let canvas = self.editor.canvas();
        let v = crate::props::fit(prop, field::stored(prop, shown), canvas);
        self.editor.set_prop(prop, v)
    }

    /// Move a slider to normalized `t` of its range: a property of the
    /// primary, or a parameter of one of its effects.
    fn inspector_slider_to(&mut self, target: SliderTarget, range: (f32, f32), t: f32) {
        let (lo, hi) = range;
        let v = lo + t.clamp(0.0, 1.0) * (hi - lo);
        match target {
            SliderTarget::Prop(prop) => {
                self.editor.set_prop(prop, v);
            }
            SliderTarget::Effect { id, param } => {
                if let Some(i) = self.editor.primary() {
                    self.editor.set_effect_param(i, id, param as u8, v);
                }
            }
        }
    }
}
