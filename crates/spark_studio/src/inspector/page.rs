//! The inspector's layout: the colour section pinned at the top under
//! its collapsible header, and under it a scrolling stack for the
//! primary selection — its kind glyph and name in an editable box, the
//! transform strip of scrub fields, the kind's own switch, its sliders,
//! and Additive. Nothing selected: the colour section alone.
//!
//! Pure geometry: built from a snapshot of state, it hands back hit
//! tests, and its rects (`rects`) and words (`labels`) for the passes,
//! and never touches the editor. The frame and the input path build the
//! same `Page` from the same inputs, so what lights is what clicks.

use spark_render::{LIGHT_KINDS, STAR_FORMS, Viewport};
use spark_ui::{Checkbox, Segmented, Slider};

use super::EditKey;
use super::field;
use super::popup::Slot;
use crate::editor::Editor;
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
/// A field row: its caption line, the box, and the air after.
pub(super) const CAPTION_H: f32 = 24.0;
const FIELD_H: f32 = 46.0;
const FIELD_ROW_H: f32 = 80.0;
const FIELD_GAP: f32 = 10.0;
/// A slider row: its label line, the thumb's band, and the air after.
pub(super) const SLIDER_LABEL_H: f32 = 24.0;
const SLIDER_TRACK_H: f32 = 15.0;
const SLIDER_ROW_H: f32 = 64.0;
/// A switch row and a checkbox row, with their air.
const SWITCH_H: f32 = 46.0;
const CHECK_SIDE: f32 = 30.0;
const CHECK_ROW_H: f32 = 48.0;
const GAP: f32 = 10.0;
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
    Field(usize),
    Slider(usize),
    /// A segment of one of the page's switches.
    Switch(usize, usize),
    Check(usize),
}

/// One scrub field, laid out.
#[derive(Clone, Debug, PartialEq)]
pub struct FieldSlot {
    pub prop: Prop,
    pub caption: &'static str,
    /// Its column in the row — its caption's colour.
    pub col: usize,
    pub rect: Viewport,
    /// The number shown (degrees for an angle).
    pub shown: f32,
    pub text: String,
}

/// One slider, laid out.
#[derive(Clone, Debug, PartialEq)]
pub struct SliderSlot {
    pub prop: Prop,
    pub label: &'static str,
    pub track: Viewport,
    /// The full-width band the thumb spans — the grab.
    pub hit: Viewport,
    pub label_y: f32,
    pub v: f32,
    pub readout: String,
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
}

pub struct CheckSlot {
    pub kind: CheckKind,
    pub check: Checkbox,
    pub label: &'static str,
    pub on: bool,
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
    pub fields: Vec<FieldSlot>,
    pub sliders: Vec<SliderSlot>,
    pub switches: Vec<SwitchSlot>,
    pub checks: Vec<CheckSlot>,
    /// How tall the body's content is, physical px — the scroll's limit.
    pub content_h: f32,
    /// The field being typed into, and its buffer.
    pub edit: Option<(usize, TextBox)>,
    /// The name being typed, if it is.
    pub name_edit: Option<TextBox>,
}

impl Page {
    /// Lay the inspector out for the editor's state. `scroll` is how far
    /// the body has been scrolled up, physical px.
    pub fn build(
        panel: Viewport,
        scale: f32,
        e: &Editor,
        scroll: f32,
        edit: Option<&(EditKey, TextBox)>,
        popup_on: Option<Slot>,
        color_open: bool,
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
            fields: Vec::new(),
            sliders: Vec::new(),
            switches: Vec::new(),
            checks: Vec::new(),
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
        let mut y = page.title_y + TITLE_H * s;

        // The transform strip: rows of three fields; a prop the shape
        // lacks is left out and the row closes up.
        let has = |prop: Prop| -> Option<f32> {
            let p = props.as_ref()?;
            match prop {
                Prop::X => Some(p.x),
                Prop::Y => Some(p.y),
                Prop::Z => Some(p.z),
                // A light is aimed, not spun; a line's angle is its ends'.
                Prop::Rotation => (!shape.is_light()).then_some(p.rotation),
                Prop::Tilt => Some(p.tilt),
                Prop::Turn => Some(p.turn),
                Prop::Scale => Some(p.size),
                Prop::Width => p.w,
                Prop::Height => p.h,
                Prop::Depth => p.d,
                _ => None,
            }
        };
        for row in field::ROWS {
            let present: Vec<(Prop, &str, f32)> = row
                .iter()
                .filter_map(|&(prop, cap)| has(prop).map(|v| (prop, cap, v)))
                .collect();
            if present.is_empty() {
                continue;
            }
            let cols = present.len().max(1) as f32;
            let box_w = (w - FIELD_GAP * s * (cols - 1.0)) / cols;
            for (k, (prop, cap, v)) in present.into_iter().enumerate() {
                let rect = Viewport {
                    x: x0 + (box_w + FIELD_GAP * s) * k as f32,
                    y: y + CAPTION_H * s,
                    w: box_w,
                    h: FIELD_H * s,
                };
                let shown = field::shown(prop, v);
                let slot = page.fields.len();
                if let Some((EditKey::Prop(p), tb)) = edit
                    && *p == prop
                {
                    page.edit = Some((slot, tb.clone()));
                }
                page.fields.push(FieldSlot {
                    prop,
                    caption: cap,
                    col: k,
                    rect,
                    shown,
                    text: field::format(shown),
                });
            }
            y += FIELD_ROW_H * s;
        }
        y += GAP * s;

        // The kind's switch.
        let switch = if shape.is_light() {
            shape
                .light_kind()
                .map(|k| (SwitchKind::LightKind, &LIGHT_KINDS[..], k.index()))
        } else if shape.is_stars() {
            shape
                .star_form()
                .map(|f| (SwitchKind::StarForm, &STAR_FORMS[..], f))
        } else {
            shape.outline().map(|o| {
                (
                    SwitchKind::FillOutline,
                    &["Fill", "Outline"][..],
                    usize::from(o),
                )
            })
        };
        if let Some((kind, labels, active)) = switch {
            let track = Viewport {
                x: x0,
                y,
                w,
                h: SWITCH_H * s,
            };
            page.switches.push(SwitchSlot {
                kind,
                seg: Segmented::new(track, labels.len(), s),
                labels,
                active,
            });
            y += (SWITCH_H + GAP) * s;
        }

        // The sliders: what this kind of thing has a bounded number for,
        // in Alva's order — Sides, Opacity, Brightness, Thickness, Glow,
        // a star field's sky after.
        let canvas = e.canvas();
        let glow = e
            .fx_of(i)
            .active(crate::fx::EffectKind::Glow)
            .map(|g| g.get(0))
            .unwrap_or(0.0);
        let mut specs: Vec<(Prop, &'static str)> = Vec::new();
        if shape.is_light() {
            specs.push((Prop::Brightness, "Intensity"));
            if shape.cone().is_some() {
                specs.push((Prop::Cone, "Cone"));
            }
            if shape.rim().is_some() {
                specs.push((Prop::Rim, "Rim"));
            }
        } else {
            if shape.sides().is_some() {
                specs.push((Prop::Sides, "Sides"));
            }
            specs.push((Prop::Opacity, "Opacity"));
            specs.push((Prop::Brightness, "Brightness"));
            if shape.thickness().is_some() {
                specs.push((
                    Prop::Thickness,
                    if shape.is_stars() { "Size" } else { "Thickness" },
                ));
            }
            if !shape.is_mesh() {
                specs.push((Prop::Glow, "Glow"));
            }
            if shape.is_stars() {
                specs.push((Prop::Density, "Density"));
                specs.push((Prop::Twinkle, "Twinkle"));
                specs.push((Prop::TwinkleRate, "Rate"));
            }
        }
        let track_h = SLIDER_TRACK_H * s;
        let thumb = Slider::thumb_side(Viewport {
            x: 0.0,
            y: 0.0,
            w: 1.0,
            h: track_h,
        });
        for (prop, label) in specs {
            let value = match prop {
                Prop::Glow => glow,
                p => crate::anim::prop_value(shape, p).unwrap_or(0.0),
            };
            let (lo, hi) = crate::props::range(prop, canvas);
            let band_y = y + SLIDER_LABEL_H * s;
            page.sliders.push(SliderSlot {
                prop,
                label,
                track: Viewport {
                    x: x0,
                    y: band_y + (thumb - track_h) * 0.5,
                    w,
                    h: track_h,
                },
                hit: Viewport {
                    x: x0,
                    y: band_y,
                    w,
                    h: thumb,
                },
                label_y: y,
                v: ((value - lo) / (hi - lo).max(1e-6)).clamp(0.0, 1.0),
                readout: crate::defaults::readout(prop, value),
            });
            y += SLIDER_ROW_H * s;
        }

        // Additive: pure light, for anything that can be.
        if !shape.is_light() && !shape.is_mesh() {
            page.checks.push(CheckSlot {
                kind: CheckKind::Additive,
                check: Checkbox::new(
                    x0,
                    y + (CHECK_ROW_H - CHECK_SIDE) * 0.5 * s,
                    w,
                    CHECK_SIDE * s,
                    s,
                ),
                label: "Additive",
                on: shape.additive(),
            });
            y += CHECK_ROW_H * s;
        }
        page.content_h = y + scroll - body.y + pad;
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
            if let Some(nb) = self.name_box
                && self.visible(nb)
                && nb.contains(x, y)
            {
                return Some(Hit::Name);
            }
            if let Some(k) = self
                .fields
                .iter()
                .position(|f| self.visible(f.rect) && f.rect.contains(x, y))
            {
                return Some(Hit::Field(k));
            }
            if let Some(k) = self
                .sliders
                .iter()
                .position(|sl| self.visible(sl.hit) && sl.hit.contains(x, y))
            {
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
