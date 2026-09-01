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

mod colour;
mod field;
mod labels;
mod page;
mod popup;
#[cfg(test)]
mod tests;

pub use colour::{hsv_of, rgb_of, with_channel};
pub use page::{Hit, Page};
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
}

/// What a field being typed into is for: a number on the object, or the
/// popup's hex code or one of its channels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditKey {
    Prop(Prop),
    Hex,
    Chan(usize),
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
        let edit_box = page.edit.as_ref().and_then(|(slot, _)| {
            page.fields.get(*slot).map(|f| {
                (
                    f.rect,
                    f.rect.x + 14.0 * self.scale(),
                    crate::chrome::UI_TEXT * self.scale(),
                )
            })
        });
        let popup = self
            .popup_for()
            .map(|p| (p.rects(), p.labels(), p.edit_box()));
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
        if let Some((slot, _)) = &page.edit
            && hit == Some(Hit::Field(*slot))
        {
            let at = crate::textbox::index_at(&self.inspector.caret_xs, cx);
            if let Some((_, tb)) = &mut self.inspector.edit {
                tb.place(at);
            }
            return true;
        }
        let mut dirty = self.inspector_commit();
        match hit {
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
                }
            }
            Some(Hit::Slider(k)) => {
                if let Some(sl) = page.sliders.get(k) {
                    self.inspector_slider_to(sl.prop, Slider::t_at(sl.track, cx));
                    self.inspector.drag = Some(Drag::Slider(k));
                    dirty = true;
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
                    };
                }
            }
            None => {}
        }
        dirty
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
                    self.inspector_slider_to(sl.prop, Slider::t_at(sl.track, mx));
                }
                true
            }
            None => {
                let over = if panel.contains(mx, my) && !self.popup_contains(mx, my) {
                    self.inspector_page(panel).hit(mx, my)
                } else {
                    None
                };
                let dirty = over != self.inspector.over;
                self.inspector.over = over;
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

    /// The wheel over the right panel scrolls the body.
    pub(crate) fn inspector_wheel(&mut self, panel: Viewport, dy: f32) -> bool {
        let max = self.inspector_page(panel).max_scroll();
        let next = (self.inspector.scroll - dy * 60.0 * self.scale()).clamp(0.0, max);
        if (next - self.inspector.scroll).abs() < 0.5 {
            return false;
        }
        self.inspector.scroll = next;
        true
    }

    /// Keys while a field is being typed into. Enter commits, Esc lets
    /// go, the rest edit the buffer — digits and a sign and point for a
    /// number, hex digits for the code. True when the frame needs
    /// redrawing; the caller keeps every key from the editor meanwhile.
    pub(crate) fn inspector_key(&mut self, key: &winit::keyboard::Key) -> bool {
        use winit::keyboard::{Key, NamedKey};
        let shift = self.modifiers.shift_key();
        let Some((what, tb)) = &mut self.inspector.edit else {
            return false;
        };
        let what = *what;
        match key {
            Key::Named(NamedKey::Enter) => self.inspector_commit(),
            Key::Named(NamedKey::Escape) => {
                self.inspector.edit = None;
                true
            }
            Key::Named(NamedKey::Backspace) => tb.backspace(),
            Key::Named(NamedKey::Delete) => tb.delete(),
            Key::Named(NamedKey::ArrowLeft) => {
                tb.step(false, shift);
                true
            }
            Key::Named(NamedKey::ArrowRight) => {
                tb.step(true, shift);
                true
            }
            Key::Named(NamedKey::Home) => {
                tb.home(shift);
                true
            }
            Key::Named(NamedKey::End) => {
                tb.end(shift);
                true
            }
            Key::Character(s) => {
                let mut dirty = false;
                for c in s.chars() {
                    let ok = match what {
                        EditKey::Prop(_) => c.is_ascii_digit() || c == '.' || c == '-',
                        EditKey::Hex => c.is_ascii_hexdigit() || c == '#',
                        EditKey::Chan(_) => c.is_ascii_digit(),
                    };
                    if ok {
                        tb.insert(c);
                        dirty = true;
                    }
                }
                dirty
            }
            _ => false,
        }
    }

    /// Whether a field is being typed into — the keyboard is its.
    pub(crate) fn inspector_typing(&self) -> bool {
        self.inspector.edit.is_some()
    }

    /// Commit the field being typed into, if any: a number lands on the
    /// primary, a code or a channel on the popup's swatch; anything else
    /// is let go. True when there was one.
    pub(crate) fn inspector_commit(&mut self) -> bool {
        let Some((what, tb)) = self.inspector.edit.take() else {
            return false;
        };
        let slot = self.inspector.popup.unwrap_or(Slot::Fg);
        match what {
            EditKey::Prop(prop) => {
                if let Some(v) = field::parse(tb.text()) {
                    self.inspector_field_to(prop, v);
                    self.editor.end_gesture();
                }
            }
            EditKey::Hex => {
                if let Some(c) = spark_ui::from_hex(tb.text()) {
                    self.set_slot_colour(slot, [c[0], c[1], c[2]]);
                    self.editor.end_gesture();
                }
            }
            EditKey::Chan(k) => {
                if let Some(v) = field::parse(tb.text()) {
                    let rgb = with_channel(
                        self.slot_colour(slot),
                        k,
                        v.round().clamp(0.0, 255.0) as u8,
                    );
                    self.set_slot_colour(slot, rgb);
                    self.editor.end_gesture();
                }
            }
        }
        true
    }

    /// Write a field's shown number onto the primary, fitted.
    fn inspector_field_to(&mut self, prop: Prop, shown: f32) -> bool {
        let canvas = self.editor.canvas();
        let v = crate::props::fit(prop, field::stored(prop, shown), canvas);
        self.editor.set_prop(prop, v)
    }

    /// Move a slider to normalized `t`.
    fn inspector_slider_to(&mut self, prop: Prop, t: f32) {
        let canvas = self.editor.canvas();
        let (lo, hi) = crate::props::range(prop, canvas);
        self.editor
            .set_prop(prop, lo + t.clamp(0.0, 1.0) * (hi - lo));
    }
}
