//! The layer list in the right panel: folder headers and layer cards.
//!
//! Each card owns everything about the shape it represents — identity row
//! (kind glyph, name, eye, cogwheel), an always-visible X/Y/Rotation/Scale
//! scrub strip, and a cog-expanded detail section. One card expands at a
//! time. Merged groups collapse to a single identity-only card.
//!
//! Folders are headers with their members indented beneath; collapsing one
//! drops its members from the list without touching the canvas. Because
//! folder members are contiguous in the stack (see `editor/folders.rs`), the
//! list stays in exact draw order, top of stack first.
//!
//! Pure layout + hit testing — rects live in `draw`, text in `chrome`.

use spark_render::{Shape, ShapeKind, Viewport};
use spark_ui::{ICON_CIRCLE, ICON_LINE, ICON_PATH, ICON_PENTAGON, ICON_SQUARE, Segmented};

use crate::anim::prop_bit;
use crate::chrome::UI_TEXT;
use crate::editor::{Editor, Prop};
use crate::props::range;

mod draw;
mod folder;

pub use draw::rects;

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
    /// Eye toggled off — the card dims. Folder-aware: a layer inside a
    /// hidden folder reads as hidden too.
    pub hidden: bool,
    /// The row stands for a merged group (identity only, no controls).
    pub grouped: bool,
    /// The X/Y/Rotation/Scale strip (empty on group cards).
    pub scrubs: Vec<ScrubField>,
    /// Cog open — the full settings below the strip.
    pub detail: Option<CardDetail>,
}

/// A folder header row: the disclosure box, name and eye.
pub struct FolderRow {
    pub id: u32,
    /// The whole card — header strip plus the transform strip below it.
    pub row: Viewport,
    /// The identity strip: select / rename / reorder-drag.
    pub head: Viewport,
    /// The `−`/`+` disclosure box.
    pub disclose: Viewport,
    pub eye: Viewport,
    pub label_pos: [f32; 2],
    pub label: String,
    pub collapsed: bool,
    pub hidden: bool,
    /// Every member is selected — the header rings gold like a card does.
    pub selected: bool,
    /// Members, for the count badge chrome draws.
    pub count: usize,
    /// The folder transform's X/Y/R/S strip — same controls as a layer card,
    /// acting on everything inside.
    pub scrubs: Vec<ScrubField>,
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
pub(super) const SCRUB_H: f32 = 34.0;
pub(super) const PAD: f32 = 10.0;
/// Between cards. The border plate overhangs the card by 2.5px a side, so
/// the gap you actually see is this minus 5.
pub(super) const GAP: f32 = 22.0;
const SLIDER_H: f32 = 54.0;
const TOGGLE_H: f32 = 84.0;
const CHIPS_H: f32 = 52.0;
/// Folder header height, and how far its members indent.
pub(super) const FOLDER_H: f32 = 48.0;
const INDENT: f32 = 22.0;

pub struct Cards {
    pub rows: Vec<LayerRow>,
    pub folders: Vec<FolderRow>,
    /// Total content height (physical px), for scroll clamping.
    pub content_h: f32,
}

/// What the list shows, in order, top of stack first.
enum Entry {
    Folder(u32),
    Shape(usize),
}

/// Walk the stack top-down into list entries. A folder contributes its
/// header, then its members (unless collapsed) — members are contiguous, so
/// the run can be emitted whole and skipped over.
fn entries(ed: &Editor) -> Vec<Entry> {
    let mut out = Vec::new();
    let mut i = ed.shapes().len();
    while i > 0 {
        i -= 1;
        let f = ed.folder_of(i);
        if f == 0 {
            out.push(Entry::Shape(i));
            continue;
        }
        let members = ed.folder_members(f);
        out.push(Entry::Folder(f));
        if !ed.folder(f).is_some_and(|x| x.collapsed) {
            out.extend(members.iter().rev().map(|&m| Entry::Shape(m)));
        }
        // Drop below the run; the loop's own decrement steps past it.
        i = members.first().copied().unwrap_or(i);
    }
    out
}

pub fn rows(panel: Viewport, scale: f32, ed: &Editor, open: Option<usize>, scroll: f32) -> Cards {
    let pad = 12.0 * scale;
    let mut y = panel.y + pad - scroll;
    let mut out = Vec::new();
    let mut folder_rows = Vec::new();
    let shapes = ed.shapes();
    let selection = ed.selection();

    for entry in entries(ed) {
        let index = match entry {
            Entry::Folder(id) => {
                if let Some(fr) = folder::row(panel, scale, ed, id, &mut y) {
                    folder_rows.push(fr);
                }
                continue;
            }
            Entry::Shape(i) => i,
        };

        let shape = &shapes[index];
        // A merged group is one card, anchored at its top-most member.
        let g = ed.groups().get(index).copied().unwrap_or(0);
        if g != 0 && ed.groups().iter().rposition(|&x| x == g) != Some(index) {
            continue;
        }
        let grouped = g != 0;
        let expanded = !grouped && open == Some(index);
        // Members sit indented under their folder header.
        let indent = if ed.folder_of(index) == 0 {
            0.0
        } else {
            INDENT * scale
        };
        let card_x = panel.x + pad + indent;
        let card_w = panel.w - pad * 2.0 - indent;
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
        let km = ed.keyed_mask(index);

        let mut cy = head.y + head.h + 6.0 * scale;
        let mut scrubs = Vec::new();
        if !grouped {
            let c = shape.center();
            let fields: [(Prop, &str, String); 4] = [
                (Prop::X, "X", format!("{:.0}", c[0])),
                (Prop::Y, "Y", format!("{:.0}", c[1])),
                // No degree sign — the field is too narrow to spend a glyph
                // on it, and the R label already says what it is.
                (
                    Prop::Rotation,
                    "R",
                    format!("{:.0}", shape.rotation().to_degrees()),
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

        let detail = expanded.then(|| detail(shape, inner_x, inner_w, scale, km, &mut cy));

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
                let mut label = match Some(ed.name(index)).filter(|n| !n.is_empty()) {
                    Some(given) => given.to_string(),
                    None if grouped => format!("merged {}", index + 1),
                    None => format!("{kind_name} {}", index + 1),
                };
                if grouped {
                    let members = ed.groups().iter().filter(|&&x| x == g).count();
                    label.push_str(&format!(" x{members}"));
                }
                label
            },
            rgb: shape.rgb(),
            selected: selection.contains(&index),
            hidden: ed.is_hidden(index),
            grouped,
            scrubs,
            detail,
        });
        y = row.y + row.h + GAP * scale;
    }
    Cards {
        rows: out,
        folders: folder_rows,
        content_h: (y + scroll - panel.y).max(0.0),
    }
}

/// The cog-expanded settings block, advancing `cy` as it lays out.
fn detail(
    shape: &Shape,
    inner_x: f32,
    inner_w: f32,
    scale: f32,
    km: u16,
    cy: &mut f32,
) -> CardDetail {
    let mut sliders = Vec::new();
    let mut push = |prop: Prop, label: &'static str, v: f32, value: String, cy: &mut f32| {
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
    *cy += 4.0 * scale;
    if let Some([w, h]) = shape.box_size() {
        push(Prop::Width, "Width", w, format!("{w:.0}"), cy);
        push(Prop::Height, "Height", h, format!("{h:.0}"), cy);
    }
    let glow = shape.glow_radius();
    push(Prop::Glow, "Glow", glow, format!("{glow:.0}"), cy);
    let br = shape.brightness();
    push(Prop::Brightness, "Brightness", br, format!("{br:.1}"), cy);
    if let Some(sides) = shape.sides() {
        push(Prop::Sides, "Sides", sides as f32, format!("{sides}"), cy);
    }
    if let Some(th) = shape.thickness() {
        push(Prop::Thickness, "Thickness", th, format!("{th:.1}"), cy);
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
    let style = shape.outline().map(|o| toggle(cy, o));
    let blend = toggle(cy, shape.additive());
    let grad = toggle(cy, shape.gradient());
    let chips = shape.gradient().then(|| {
        let side = 40.0 * scale;
        let chips = [
            Viewport {
                x: inner_x,
                y: *cy,
                w: side,
                h: side,
            },
            Viewport {
                x: inner_x + side + 10.0 * scale,
                y: *cy,
                w: side,
                h: side,
            },
        ];
        *cy += CHIPS_H * scale;
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
    /// A folder's `−`/`+` disclosure box.
    FolderDisclose(u32),
    /// A folder's eye.
    FolderEye(u32),
    /// A folder header: select its contents / rename / drop layers onto it.
    FolderHead(u32),
    /// Start scrubbing one of a folder transform's fields.
    FolderScrub(u32, Prop),
}

/// Hits require the click inside the panel too — scrolled-out cards must
/// not swallow clicks meant for whatever's beneath them.
pub fn hit(cards: &Cards, panel: Viewport, px: f32, py: f32) -> Option<CardHit> {
    if !panel.contains(px, py) {
        return None;
    }
    for f in &cards.folders {
        if let Some(s) = f.scrubs.iter().find(|s| s.rect.contains(px, py)) {
            return Some(CardHit::FolderScrub(f.id, s.prop));
        }
        if !f.row.contains(px, py) {
            continue;
        }
        if f.disclose.contains(px, py) {
            return Some(CardHit::FolderDisclose(f.id));
        }
        if f.eye.contains(px, py) {
            return Some(CardHit::FolderEye(f.id));
        }
        return Some(CardHit::FolderHead(f.id));
    }
    let lr = cards.rows.iter().find(|r| r.row.contains(px, py))?;
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
