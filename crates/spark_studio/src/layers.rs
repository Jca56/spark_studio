//! Layer cards in the right panel: each card owns everything about the
//! shape it represents — identity row (chip, kind icon, name, cogwheel),
//! an always-visible X/Y/Rotation/Scale scrub strip, and a cog-expanded
//! detail section with the rest of the settings. One card expands at a
//! time. Merged groups collapse to a single identity-only card. Pure
//! layout — rendering and clicks live in main.

use spark_render::{Shape, ShapeKind, Viewport};
use spark_ui::{
    ICON_CIRCLE, ICON_LINE, ICON_PATH, ICON_PENTAGON, ICON_SQUARE, Segmented, UiRect, srgb, theme,
};

use crate::anim::prop_bit;
use crate::chrome::UI_TEXT;
use crate::editor::Prop;
use crate::props::range;

/// Scrub-field / detail text size, a step under the body text.
pub const CARD_TEXT: f32 = 20.0;

/// A labeled two-way segmented toggle row (chrome draws the labels).
pub struct ToggleRow {
    pub seg: Segmented,
    /// Whether the second segment is the active one.
    pub on: bool,
    pub label_pos: [f32; 2],
}

/// One drag-to-scrub numeric field on a card's transform strip.
pub struct ScrubField {
    pub prop: Prop,
    pub rect: Viewport,
    pub label: &'static str,
    pub value: String,
    /// Property has keyframes — the value reads out in gold.
    pub keyed: bool,
}

/// A full-width slider row inside an expanded card.
pub struct SliderRow {
    pub prop: Prop,
    pub label: &'static str,
    pub label_pos: [f32; 2],
    pub track: Viewport,
    pub t: f32,
    pub value: String,
    pub keyed: bool,
}

/// The cog-expanded settings section.
pub struct CardDetail {
    pub sliders: Vec<SliderRow>,
    /// Fill/Outline — absent for lines and paths.
    pub style: Option<ToggleRow>,
    /// Solid/Add compositing.
    pub blend: ToggleRow,
    /// Gradient Off/On.
    pub grad: ToggleRow,
    /// Gradient endpoint chips [A, B] while the gradient is on; clicking
    /// one arms it as the color home's target.
    pub chips: Option<[Viewport; 2]>,
    pub rgb2: [f32; 3],
}

pub struct LayerRow {
    /// Index into the editor's shape list.
    pub index: usize,
    /// The whole card.
    pub row: Viewport,
    /// The identity strip: select / double-click-rename / reorder-drag.
    pub head: Viewport,
    /// The kind glyph, tinted the shape's color — a stand-in preview
    /// until real layer thumbnails arrive.
    pub icon: Viewport,
    /// The visibility eye — every card has one; a group card's eye flips
    /// the whole group.
    pub eye: Viewport,
    /// The cogwheel expand button (absent on group cards).
    pub cog: Option<Viewport>,
    pub icon_kind: f32,
    /// Ngon side count for the glyph (0 = not an ngon).
    pub icon_sides: f32,
    /// Top-left of the label text (physical px).
    pub label_pos: [f32; 2],
    pub label: String,
    pub rgb: [f32; 3],
    pub selected: bool,
    /// Eye toggled off — the card dims.
    pub hidden: bool,
    /// The row stands for a merged group (identity only, no controls).
    pub grouped: bool,
    /// The X/Y/Rotation/Scale strip (empty on group cards).
    pub scrubs: Vec<ScrubField>,
    /// Cog open — the full settings below the strip.
    pub detail: Option<CardDetail>,
}

pub(crate) fn kind_parts(kind: ShapeKind) -> (f32, &'static str) {
    match kind {
        ShapeKind::Circle => (ICON_CIRCLE, "circle"),
        ShapeKind::Box => (ICON_SQUARE, "box"),
        ShapeKind::Ngon => (ICON_PENTAGON, "polygon"),
        ShapeKind::Line => (ICON_LINE, "line"),
        ShapeKind::Path => (ICON_PATH, "path"),
    }
}

/// Logical-px card metrics.
const HEAD_H: f32 = 46.0;
const SCRUB_H: f32 = 34.0;
const PAD: f32 = 10.0;
const GAP: f32 = 10.0;
const SLIDER_H: f32 = 54.0;
const TOGGLE_H: f32 = 84.0;
const CHIPS_H: f32 = 52.0;

pub struct Cards {
    pub rows: Vec<LayerRow>,
    /// Total content height (physical px), for scroll clamping.
    pub content_h: f32,
}

#[allow(clippy::too_many_arguments)]
pub fn rows(
    panel: Viewport,
    scale: f32,
    shapes: &[Shape],
    names: &[String],
    groups: &[u32],
    keyed: &[u16],
    hidden: &[bool],
    selection: &[usize],
    open: Option<usize>,
    scroll: f32,
) -> Cards {
    let pad = 12.0 * scale;
    let mut y = panel.y + pad - scroll;
    let mut out = Vec::new();
    for (index, shape) in shapes.iter().enumerate().rev() {
        // A merged group is one card, anchored at its top-most member.
        let g = groups.get(index).copied().unwrap_or(0);
        if g != 0 && groups.iter().rposition(|&x| x == g) != Some(index) {
            continue;
        }
        let grouped = g != 0;
        let expanded = !grouped && open == Some(index);
        let card_x = panel.x + pad;
        let card_w = panel.w - pad * 2.0;
        let inner_x = card_x + PAD * scale;
        let inner_w = card_w - PAD * 2.0 * scale;
        let head = Viewport {
            x: card_x,
            y: y + PAD * scale,
            w: card_w,
            h: HEAD_H * scale,
        };
        let icon = Viewport {
            x: inner_x,
            y: head.y,
            w: head.h,
            h: head.h,
        };
        let cog_side = 38.0 * scale;
        let cog_slot = Viewport {
            x: card_x + card_w - PAD * scale - cog_side,
            y: head.y + (head.h - cog_side) * 0.5,
            w: cog_side,
            h: cog_side,
        };
        let cog = (!grouped).then_some(cog_slot);
        // The eye sits left of the cog (or in its slot on group cards).
        let eye = if grouped {
            cog_slot
        } else {
            Viewport {
                x: cog_slot.x - cog_side - 6.0 * scale,
                ..cog_slot
            }
        };
        let (icon_kind, kind_name) = kind_parts(shape.kind());
        let km = keyed.get(index).copied().unwrap_or(0);

        let mut cy = head.y + head.h + 6.0 * scale;
        let mut scrubs = Vec::new();
        if !grouped {
            let c = shape.center();
            let fields: [(Prop, &str, String); 4] = [
                (Prop::X, "X", format!("{:.0}", c[0])),
                (Prop::Y, "Y", format!("{:.0}", c[1])),
                (
                    Prop::Rotation,
                    "R",
                    format!("{:.0}\u{b0}", shape.rotation().to_degrees()),
                ),
                (Prop::Scale, "S", format!("{:.0}", shape.size())),
            ];
            let fgap = 6.0 * scale;
            let fw = (inner_w - fgap * 3.0) / 4.0;
            for (k, (prop, label, value)) in fields.into_iter().enumerate() {
                scrubs.push(ScrubField {
                    prop,
                    rect: Viewport {
                        x: inner_x + (fw + fgap) * k as f32,
                        y: cy,
                        w: fw,
                        h: SCRUB_H * scale,
                    },
                    label,
                    value,
                    keyed: km & prop_bit(prop) != 0,
                });
            }
            cy += (SCRUB_H + 6.0) * scale;
        }

        let detail = expanded.then(|| {
            let mut sliders = Vec::new();
            let mut push =
                |prop: Prop, label: &'static str, v: f32, value: String, cy: &mut f32| {
                    let (min, max) = range(prop);
                    sliders.push(SliderRow {
                        prop,
                        label,
                        label_pos: [inner_x, *cy],
                        track: Viewport {
                            x: inner_x,
                            y: *cy + 30.0 * scale,
                            w: inner_w,
                            h: 10.0 * scale,
                        },
                        t: ((v - min) / (max - min)).clamp(0.0, 1.0),
                        value,
                        keyed: km & prop_bit(prop) != 0,
                    });
                    *cy += SLIDER_H * scale;
                };
            cy += 4.0 * scale;
            if let Some([w, h]) = shape.box_size() {
                push(Prop::Width, "Width", w, format!("{w:.0}"), &mut cy);
                push(Prop::Height, "Height", h, format!("{h:.0}"), &mut cy);
            }
            let glow = shape.glow_radius();
            push(Prop::Glow, "Glow", glow, format!("{glow:.0}"), &mut cy);
            let br = shape.brightness();
            push(
                Prop::Brightness,
                "Brightness",
                br,
                format!("{br:.1}"),
                &mut cy,
            );
            if let Some(sides) = shape.sides() {
                push(
                    Prop::Sides,
                    "Sides",
                    sides as f32,
                    format!("{sides}"),
                    &mut cy,
                );
            }
            if let Some(th) = shape.thickness() {
                push(
                    Prop::Thickness,
                    "Thickness",
                    th,
                    format!("{th:.1}"),
                    &mut cy,
                );
            }
            let toggle = |cy: &mut f32, on: bool| {
                let t = ToggleRow {
                    label_pos: [inner_x, *cy],
                    seg: Segmented::new(
                        Viewport {
                            x: inner_x,
                            y: *cy + 32.0 * scale,
                            w: inner_w,
                            h: 44.0 * scale,
                        },
                        2,
                        scale,
                    ),
                    on,
                };
                *cy += TOGGLE_H * scale;
                t
            };
            let style = shape.outline().map(|o| toggle(&mut cy, o));
            let blend = toggle(&mut cy, shape.additive());
            let grad = toggle(&mut cy, shape.gradient());
            let chips = shape.gradient().then(|| {
                let side = 40.0 * scale;
                let chips = [
                    Viewport {
                        x: inner_x,
                        y: cy,
                        w: side,
                        h: side,
                    },
                    Viewport {
                        x: inner_x + side + 10.0 * scale,
                        y: cy,
                        w: side,
                        h: side,
                    },
                ];
                cy += CHIPS_H * scale;
                chips
            });
            CardDetail {
                sliders,
                style,
                blend,
                grad,
                chips,
                rgb2: shape.rgb2(),
            }
        });

        let row = Viewport {
            x: card_x,
            y,
            w: card_w,
            h: (cy + PAD * scale - y).max(1.0),
        };
        let label_x = icon.x + icon.w + 4.0 * scale;
        out.push(LayerRow {
            index,
            row,
            head,
            icon,
            eye,
            cog,
            icon_kind,
            icon_sides: shape.sides().map(|s| s as f32).unwrap_or(0.0),
            label_pos: [label_x, head.y + (head.h - UI_TEXT * 1.2 * scale) * 0.5],
            label: {
                let mut label = match names.get(index).filter(|n| !n.is_empty()) {
                    Some(given) => given.clone(),
                    None if grouped => format!("merged {}", index + 1),
                    None => format!("{kind_name} {}", index + 1),
                };
                if grouped {
                    let members = groups.iter().filter(|&&x| x == g).count();
                    label.push_str(&format!(" x{members}"));
                }
                label
            },
            rgb: shape.rgb(),
            selected: selection.contains(&index),
            hidden: hidden.get(index).copied().unwrap_or(false),
            grouped,
            scrubs,
            detail,
        });
        y = row.y + row.h + GAP * scale;
    }
    Cards {
        rows: out,
        content_h: (y + scroll - panel.y).max(0.0),
    }
}

pub enum CardHit {
    /// The identity strip (or dead card space): select / rename / reorder.
    Head(usize),
    /// The visibility eye.
    Eye(usize),
    Cog(usize),
    /// Start scrubbing this field.
    Scrub(usize, Prop),
    /// A detail slider: (shape, prop, normalized position).
    Slider(usize, Prop, f32),
    Outline(usize, bool),
    Blend(usize, bool),
    Gradient(usize, bool),
    /// Arm a gradient endpoint as the color home's target (true = B).
    Chip(usize, bool),
}

/// Hits require the click inside the panel too — scrolled-out cards must
/// not swallow clicks meant for whatever's beneath them.
pub fn hit(rows: &[LayerRow], panel: Viewport, px: f32, py: f32) -> Option<CardHit> {
    if !panel.contains(px, py) {
        return None;
    }
    let lr = rows.iter().find(|r| r.row.contains(px, py))?;
    let i = lr.index;
    if lr.eye.contains(px, py) {
        return Some(CardHit::Eye(i));
    }
    if lr.cog.is_some_and(|c| c.contains(px, py)) {
        return Some(CardHit::Cog(i));
    }
    if let Some(f) = lr.scrubs.iter().find(|f| f.rect.contains(px, py)) {
        return Some(CardHit::Scrub(i, f.prop));
    }
    if let Some(d) = &lr.detail {
        if let Some(row) = d.sliders.iter().find(|r| {
            px >= r.track.x
                && px <= r.track.x + r.track.w
                && (py - (r.track.y + r.track.h * 0.5)).abs() <= r.track.h * 2.2
        }) {
            let t = ((px - row.track.x) / row.track.w).clamp(0.0, 1.0);
            return Some(CardHit::Slider(i, row.prop, t));
        }
        if let Some(s) = &d.style
            && let Some(k) = s.seg.hit(px, py)
        {
            return Some(CardHit::Outline(i, k == 1));
        }
        if let Some(k) = d.blend.seg.hit(px, py) {
            return Some(CardHit::Blend(i, k == 1));
        }
        if let Some(k) = d.grad.seg.hit(px, py) {
            return Some(CardHit::Gradient(i, k == 1));
        }
        if let Some(chips) = &d.chips {
            for (k, c) in chips.iter().enumerate() {
                if c.contains(px, py) {
                    return Some(CardHit::Chip(i, k == 1));
                }
            }
        }
    }
    // The head, or dead card space: either way it's the card's click.
    Some(CardHit::Head(i))
}

/// Card furniture: backgrounds, chips, icons, cog, scrub wells, slider
/// tracks, toggles, gradient chips. Text is chrome's job.
pub fn rects(
    rows: &[LayerRow],
    scale: f32,
    grad_edit_b: bool,
    cog_hover: Option<usize>,
    editing: Option<(usize, Prop)>,
) -> Vec<UiRect> {
    let th = theme();
    let mut out = Vec::new();
    for lr in rows {
        if lr.selected {
            // Selection reads as a gold border, not a purple wash.
            let b = 2.5 * scale;
            out.push(UiRect::region_rounded(
                Viewport {
                    x: lr.row.x - b,
                    y: lr.row.y - b,
                    w: lr.row.w + b * 2.0,
                    h: lr.row.h + b * 2.0,
                },
                th.playhead,
                12.0 * scale,
            ));
        }
        out.push(UiRect::region_rounded(lr.row, th.card, 10.0 * scale));
        // The kind glyph wears the shape's color (dimmed when hidden).
        let mut icon_col = [lr.rgb[0], lr.rgb[1], lr.rgb[2], 1.0];
        if lr.hidden {
            icon_col = [
                icon_col[0] * 0.35,
                icon_col[1] * 0.35,
                icon_col[2] * 0.35,
                1.0,
            ];
        }
        let mut icon = UiRect::icon_sized(lr.icon, lr.icon_kind, 2.5 * scale, icon_col, 0.34);
        icon.icon[2] = lr.icon_sides;
        out.push(icon);
        out.push(UiRect::icon_sized(
            lr.eye,
            if lr.hidden {
                spark_ui::ICON_EYE_OFF
            } else {
                spark_ui::ICON_EYE
            },
            1.8 * scale,
            if lr.hidden { srgb(0x6a6a6a) } else { th.icon },
            0.36,
        ));
        if let Some(cog) = lr.cog {
            let open = lr.detail.is_some();
            if open || cog_hover == Some(lr.index) {
                out.push(UiRect::region_rounded(cog, th.button_hover, 8.0 * scale));
            }
            out.push(UiRect::icon_sized(
                cog,
                spark_ui::ICON_GEAR,
                0.0,
                if open { th.grad_gold } else { th.icon },
                0.40,
            ));
        }
        for f in &lr.scrubs {
            // A sunken well so each field reads as its own box; the one
            // being text-edited rings gold.
            if editing == Some((lr.index, f.prop)) {
                let b = 2.0 * scale;
                out.push(UiRect::region_rounded(
                    Viewport {
                        x: f.rect.x - b,
                        y: f.rect.y - b,
                        w: f.rect.w + b * 2.0,
                        h: f.rect.h + b * 2.0,
                    },
                    th.playhead,
                    8.0 * scale,
                ));
            }
            out.push(UiRect::region_rounded(f.rect, srgb(0x141414), 6.0 * scale));
        }
        if let Some(d) = &lr.detail {
            for row in &d.sliders {
                out.extend(spark_ui::Slider::rects(row.track, row.t));
            }
            for t in [d.style.as_ref(), Some(&d.blend), Some(&d.grad)]
                .into_iter()
                .flatten()
            {
                out.extend(t.seg.rects(t.on as usize));
            }
            if let Some(chips) = &d.chips {
                for (k, c) in chips.iter().enumerate() {
                    let rgb = if k == 0 { lr.rgb } else { d.rgb2 };
                    // The armed endpoint (the color home's target) rings gold.
                    if lr.selected && (k == 1) == grad_edit_b {
                        let b = 3.0 * scale;
                        out.push(UiRect::region_rounded(
                            Viewport {
                                x: c.x - b,
                                y: c.y - b,
                                w: c.w + b * 2.0,
                                h: c.h + b * 2.0,
                            },
                            th.playhead,
                            9.0 * scale,
                        ));
                    }
                    out.push(UiRect::region_rounded(
                        *c,
                        [rgb[0], rgb[1], rgb[2], 1.0],
                        7.0 * scale,
                    ));
                }
            }
        }
    }
    out
}
