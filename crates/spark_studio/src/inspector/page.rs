//! The inspector's layout: the colour section pinned at the top under
//! its collapsible header, and under it a scrolling stack of
//! **sections** for the primary selection — the name row, then
//! `Transform`, `Style` (`Light` for a light), and one section per effect
//! on the object, each with an *Enabled* box, its settings and a red
//! *Remove* (Ember's inspector, at Alva's text size). Sections fold under
//! their headers. Nothing selected: the colour section alone.
//!
//! Pure geometry: built from a snapshot of state, it hands back hit
//! tests, and its rects (`rects`) and words (`labels`) for the passes,
//! and never touches the editor. The frame and the input path build the
//! same `Page` from the same inputs, so what lights is what clicks.

use spark_render::Viewport;
use spark_ui::{Checkbox, Segmented};

pub use super::EditKey;
use super::build::Cursor;
use super::sections;
use super::popup::Slot;
use crate::editor::Editor;
use crate::fx::EffectKind;
use crate::props::{Prop, SWATCH_COLS, SWATCH_ROWS, swatch_grid};
use crate::textbox::TextBox;

/// Inset from the panel's edges, logical px.
pub const PAD: f32 = 18.0;
/// The colour section's header row — the triangle and the word.
pub(super) const HEADER_H: f32 = 36.0;
pub(super) const HEADER_TEXT: f32 = 22.0;
/// The foreground/background pair: the square's side, and how far the
/// background sits down-right under the foreground.
const PAIR: f32 = 46.0;
const PAIR_OFF: f32 = 22.0;
/// Air between the pair and the grid, and between grid chips.
const GRID_INSET: f32 = 18.0;
const GRID_GAP: f32 = 6.0;
/// The rule under the colour section, and the air around it.
const DIVIDER: f32 = 2.0;
const HOME_GAP: f32 = 14.0;
/// The name row: the box's height, its font, the glyph beside it, and
/// the row's air.
pub(super) const NAME_H: f32 = 54.0;
pub(super) const NAME_TEXT: f32 = 30.0;
pub(super) const GLYPH: f32 = 34.0;
pub(super) const TITLE_H: f32 = 68.0;
/// Caption font size — small for a caption, big for Alva.
pub(super) const CAPTION_TEXT: f32 = 19.0;

/// A widget on the page.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hit {
    /// The colour section's header — click to fold it.
    ColorHeader,
    /// The foreground and background swatches, and a grid chip.
    Fg,
    Bg,
    Chip(usize),
    /// The object's name box.
    Name,
    /// A body section's header — click to fold it.
    Section(usize),
    Field(usize),
    Slider(usize),
    /// A segment of one of the page's switches.
    Switch(usize, usize),
    Check(usize),
    /// An effect's colour chip.
    FxChip(usize),
    Button(usize),
}

/// Which section a header opens.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SectionKey {
    Transform,
    Style,
    Effect(EffectKind),
}

/// A section, laid out: its header, whether it is open, and the rule
/// down its content's left (zero-height while folded).
#[derive(Clone, Debug, PartialEq)]
pub struct SectionSlot {
    pub key: SectionKey,
    pub title: String,
    pub header: Viewport,
    pub open: bool,
    pub rule: Viewport,
}

/// One scrub field, laid out.
#[derive(Clone, Debug, PartialEq)]
pub struct FieldSlot {
    /// A reaction rides this setting — the dot on the field.
    pub reacts: bool,
    pub prop: Prop,
    pub caption: &'static str,
    /// Its column in the row — its caption's colour.
    pub col: usize,
    pub rect: Viewport,
    /// The number shown (degrees for an angle).
    pub shown: f32,
    pub text: String,
}

/// What a slider moves: a property of the object, or a parameter of one
/// of its effects.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SliderTarget {
    Prop(Prop),
    Effect { id: u32, param: usize },
}

/// One slider, laid out.
#[derive(Clone, Debug, PartialEq)]
pub struct SliderSlot {
    /// A reaction rides this setting — the dot on the slider.
    pub reacts: bool,
    pub target: SliderTarget,
    pub label: &'static str,
    pub track: Viewport,
    /// The full-width band the thumb spans — the grab.
    pub hit: Viewport,
    pub label_y: f32,
    /// What the ends of the track mean.
    pub range: (f32, f32),
    pub v: f32,
    pub readout: String,
}

impl SliderSlot {
    /// The property it moves, if it moves one.
    #[cfg(test)]
    pub fn prop(&self) -> Option<Prop> {
        match self.target {
            SliderTarget::Prop(p) => Some(p),
            SliderTarget::Effect { .. } => None,
        }
    }
}

/// What a switch switches.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SwitchKind {
    FillOutline,
    StarForm,
    LightKind,
}

pub struct SwitchSlot {
    pub kind: SwitchKind,
    pub seg: Segmented,
    pub labels: &'static [&'static str],
    pub active: usize,
}

/// What a checkbox checks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CheckKind {
    Additive,
    /// An effect's on switch, by its id.
    EffectOn(u32),
}

pub struct CheckSlot {
    pub kind: CheckKind,
    pub check: Checkbox,
    pub label: &'static str,
    pub on: bool,
}

/// An effect's colour, as a chip: click takes the background colour.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ChipSlot {
    pub id: u32,
    pub param: usize,
    pub rect: Viewport,
    pub rgb: [f32; 3],
    pub label: &'static str,
}

/// What a button does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ButtonKind {
    RemoveEffect(u32),
}

#[derive(Clone, Debug, PartialEq)]
pub struct ButtonSlot {
    pub kind: ButtonKind,
    pub rect: Viewport,
    pub label: String,
}

pub struct Page {
    pub scale: f32,
    // -- pinned: the colour section ------------------------------------
    /// The header row, and whether the section under it is open.
    pub header: Viewport,
    pub color_open: bool,
    /// The foreground and background swatches (the background under and
    /// offset from the foreground), their colours, and which one the
    /// popup is open on. Empty rectangles while the section is folded.
    pub fg: Viewport,
    pub bg: Viewport,
    pub fg_rgb: [f32; 3],
    pub bg_rgb: [f32; 3],
    pub popup_on: Option<Slot>,
    /// The swatch grid, row-major (empty while folded), and the chip
    /// that is the foreground.
    pub grid: Vec<Viewport>,
    pub grid_sel: Option<usize>,
    /// The rule under the colour section.
    pub divider: Viewport,
    // -- the body, scrolled --------------------------------------------
    /// Where the body draws: everything below the rule.
    pub body: Viewport,
    /// The selection's name and kind glyph (icon kind, tint), if any;
    /// where the name row sits (the body's top, scrolled); the glyph's
    /// square and the name's box.
    pub title: Option<(String, f32, [f32; 4])>,
    pub title_y: f32,
    pub glyph: Option<Viewport>,
    pub name_box: Option<Viewport>,
    pub sections: Vec<SectionSlot>,
    pub fields: Vec<FieldSlot>,
    pub sliders: Vec<SliderSlot>,
    pub switches: Vec<SwitchSlot>,
    pub checks: Vec<CheckSlot>,
    pub chips: Vec<ChipSlot>,
    pub buttons: Vec<ButtonSlot>,
    /// How tall the body's content is, physical px — the scroll's limit.
    pub content_h: f32,
    /// The field being typed into, and its buffer.
    pub edit: Option<(usize, TextBox)>,
    /// The name being typed, if it is.
    pub name_edit: Option<TextBox>,
}

/// How an effect parameter's readout prints: to a hundredth over a
/// short or middling range (React's amounts run to 20, and a dialled
/// 0.25 has to read as 0.25), whole over a long one.
pub fn fmt_param(v: f32, spec: &crate::fx::ParamSpec) -> String {
    if spec.max - spec.min <= 50.0 {
        format!("{v:.2}")
    } else {
        format!("{v:.0}")
    }
}

impl Page {
    /// Lay the inspector out for the editor's state. `scroll` is how far
    /// the body has been scrolled up, physical px; `folded` the sections
    /// closed under their headers.
    #[allow(clippy::too_many_arguments)]
    pub fn build(
        panel: Viewport,
        scale: f32,
        e: &Editor,
        scroll: f32,
        edit: Option<&(EditKey, TextBox)>,
        popup_on: Option<Slot>,
        color_open: bool,
        folded: &[SectionKey],
    ) -> Self {
        let s = scale;
        let pad = PAD * s;
        let x0 = panel.x + pad;
        let w = panel.w - pad * 2.0;
        let mut y = panel.y + pad;

        // The colour section under its header: the foreground/background
        // pair at the left, the swatch grid filling the rest, a rule
        // under both — or, folded, just the header and the rule.
        let header = Viewport {
            x: x0,
            y,
            w,
            h: HEADER_H * s,
        };
        y += HEADER_H * s + 6.0 * s;
        let none = Viewport {
            x: x0,
            y,
            w: 0.0,
            h: 0.0,
        };
        let (mut fg, mut bg, mut grid) = (none, none, Vec::new());
        if color_open {
            let pair = PAIR * s;
            let off = PAIR_OFF * s;
            fg = Viewport {
                x: x0,
                y,
                w: pair,
                h: pair,
            };
            bg = Viewport {
                x: x0 + off,
                y: y + off,
                w: pair,
                h: pair,
            };
            let grid_x = x0 + pair + off + GRID_INSET * s;
            let grid_w = panel.x + panel.w - pad - grid_x;
            let gap = GRID_GAP * s;
            let cols = SWATCH_COLS as f32;
            let rows = SWATCH_ROWS as f32;
            let chip = ((grid_w - gap * (cols - 1.0)) / cols).max(4.0);
            grid = (0..SWATCH_COLS * SWATCH_ROWS)
                .map(|i| {
                    let (c, r) = ((i % SWATCH_COLS) as f32, (i / SWATCH_COLS) as f32);
                    Viewport {
                        x: grid_x + (chip + gap) * c,
                        y: y + (chip + gap) * r,
                        w: chip,
                        h: chip,
                    }
                })
                .collect();
            let section_h = (pair + off).max(chip * rows + gap * (rows - 1.0));
            y += section_h + HOME_GAP * s;
        }
        let fg_rgb = e.color();
        let bg_rgb = e.color_b();
        let grid_sel = swatch_grid()
            .iter()
            .position(|c| c.iter().zip(fg_rgb).all(|(a, b)| (a - b).abs() < 1e-3));
        let divider = Viewport {
            x: x0,
            y,
            w,
            h: (DIVIDER * s).max(1.0),
        };
        y += divider.h + HOME_GAP * s;
        let body = Viewport {
            x: panel.x,
            y,
            w: panel.w,
            h: (panel.y + panel.h - y).max(1.0),
        };

        let mut page = Self {
            scale,
            header,
            color_open,
            fg,
            bg,
            fg_rgb,
            bg_rgb,
            popup_on,
            grid,
            grid_sel,
            divider,
            body,
            title: None,
            title_y: body.y - scroll,
            glyph: None,
            name_box: None,
            sections: Vec::new(),
            fields: Vec::new(),
            sliders: Vec::new(),
            switches: Vec::new(),
            checks: Vec::new(),
            chips: Vec::new(),
            buttons: Vec::new(),
            content_h: 0.0,
            edit: None,
            name_edit: None,
        };
        let Some(i) = e.primary() else {
            return page;
        };
        let Some(shape) = e.shapes().get(i) else {
            return page;
        };
        let props = e.selected_props();
        let (icon, _) = crate::props::kind_parts(shape.kind());
        let rgb = shape.rgb();
        page.title = Some((e.display_name(i), icon, [rgb[0], rgb[1], rgb[2], 1.0]));
        if let Some((EditKey::Name, tb)) = edit {
            page.name_edit = Some(tb.clone());
        }

        // The name row: the kind glyph, then the name in its own box.
        let g = GLYPH * s;
        page.glyph = Some(Viewport {
            x: x0,
            y: page.title_y + (NAME_H * s - g) * 0.5,
            w: g,
            h: g,
        });
        page.name_box = Some(Viewport {
            x: x0 + g + 12.0 * s,
            y: page.title_y,
            w: w - g - 12.0 * s,
            h: NAME_H * s,
        });
        let start_y = page.title_y + TITLE_H * s;
        let mut c = Cursor::new(&mut page, s, x0, w, start_y);
        sections::body(&mut c, e, i, shape, props.as_ref(), edit, folded);
        c.mark_reactions(
            &|t| e.reaction(i, t).is_some(),
            e.fx_of(i)
                .find_kind(crate::fx::EffectKind::Glow)
                .map(|g| g.id),
        );
        let end_y = c.y;
        page.content_h = end_y + scroll - body.y + pad;
        page
    }

    /// How far the body can scroll: content past its window, or nothing.
    pub fn max_scroll(&self) -> f32 {
        (self.content_h - self.body.h).max(0.0)
    }

    /// Whether a body widget is in the body's window — scrolled out is
    /// neither drawn nor clickable.
    pub(super) fn visible(&self, r: Viewport) -> bool {
        r.y + r.h > self.body.y && r.y < self.body.y + self.body.h
    }

    /// The widget under a point, if it can be clicked.
    pub fn hit(&self, x: f32, y: f32) -> Option<Hit> {
        if self.header.contains(x, y) {
            return Some(Hit::ColorHeader);
        }
        if self.body.contains(x, y) {
            let vis = |r: Viewport| self.visible(r) && r.contains(x, y);
            if self.name_box.is_some_and(vis) {
                return Some(Hit::Name);
            }
            if let Some(k) = self.sections.iter().position(|sec| vis(sec.header)) {
                return Some(Hit::Section(k));
            }
            if let Some(k) = self.fields.iter().position(|f| vis(f.rect)) {
                return Some(Hit::Field(k));
            }
            if let Some(k) = self.sliders.iter().position(|sl| vis(sl.hit)) {
                return Some(Hit::Slider(k));
            }
            for (w, sw) in self.switches.iter().enumerate() {
                if let Some(i) = sw.seg.hit(x, y)
                    && self.visible(sw.seg.segments[i])
                {
                    return Some(Hit::Switch(w, i));
                }
            }
            if let Some(k) = self
                .checks
                .iter()
                .position(|c| self.visible(c.check.row) && c.check.hit(x, y))
            {
                return Some(Hit::Check(k));
            }
            if let Some(k) = self.chips.iter().position(|c| vis(c.rect)) {
                return Some(Hit::FxChip(k));
            }
            if let Some(k) = self.buttons.iter().position(|b| vis(b.rect)) {
                return Some(Hit::Button(k));
            }
            return None;
        }
        // The foreground lies over the background: it is asked first.
        if self.fg.contains(x, y) {
            return Some(Hit::Fg);
        }
        if self.bg.contains(x, y) {
            return Some(Hit::Bg);
        }
        if let Some(i) = self.grid.iter().position(|c| c.contains(x, y)) {
            return Some(Hit::Chip(i));
        }
        None
    }
}
