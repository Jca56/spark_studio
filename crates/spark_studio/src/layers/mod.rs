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

use spark_render::{ShapeKind, Viewport};
use spark_ui::{
    ICON_CIRCLE, ICON_LINE, ICON_PATH, ICON_PENTAGON, ICON_SQUARE, ICON_STARS, Segmented,
};

use crate::anim::prop_bit;
use crate::chrome::UI_TEXT;
use crate::editor::{Editor, Prop};

mod detail;
mod draw;
pub mod effects;
mod folder;
mod hit;
#[cfg(test)]
mod tests;

use detail::detail;
pub use draw::rects;
pub use hit::{CardHit, hit};

/// Scrub-field / detail text size, a step under the body text.
pub const CARD_TEXT: f32 = 20.0;

/// A labeled two-way segmented toggle row (chrome draws the labels).
pub struct ToggleRow {
    pub seg: Segmented,
    /// Whether the second segment is the active one.
    pub on: bool,
    pub label_pos: [f32; 2],
}

/// A labeled segmented row with more than two options — the star form
/// picker. Same furniture as a [`ToggleRow`], addressed by index.
pub struct ChoiceRow {
    pub seg: Segmented,
    pub active: usize,
    pub options: &'static [&'static str],
    pub label_pos: [f32; 2],
}

/// Which scrub field is being typed into. Layer cards and folder headers
/// carry the same fields and now behave identically, so they share one way
/// of naming the focused one.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum EditField {
    Shape(usize, Prop),
    Folder(u32, Prop),
}

/// One drag-to-scrub numeric field on a card's transform strip.
pub struct ScrubField {
    pub prop: Prop,
    /// The value box itself. The label sits to its *left* — a box holding
    /// only its value reads as a place to type.
    pub rect: Viewport,
    pub label: &'static str,
    pub label_pos: [f32; 2],
    /// The label column's width, physical px: a letter's worth on the
    /// X/Y/R/S strip, a word's worth on the Z/Tilt/Turn one.
    pub label_w: f32,
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
    /// Where the readout's right edge sits. The number lives beside the
    /// track, not above it — see [`VALUE_W`].
    pub value_right: f32,
    pub keyed: bool,
}

/// Which half of an expanded card is showing. The cog opens the shape's own
/// settings; the effects button opens what you added to it.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum CardTab {
    #[default]
    Settings,
    Effects,
}

/// The expanded half of a card — one tab's worth.
pub struct CardDetail {
    /// Which half this is, so the head can light the matching button.
    pub tab: CardTab,
    /// The block the settings sit in, as its own surface: a card inside a
    /// card. Until this existed the expanded half was painted by whatever
    /// the card behind it happened to be, so the z-order ran straight from
    /// the card to the controls with nothing between them.
    pub panel: Viewport,
    /// Where the shape's plane sits: the Z / Tilt / Turn strip. Scrub
    /// fields rather than sliders, because Tilt and Turn count turns the
    /// way Rotation does and a slider can't type 720.
    pub scrubs: Vec<ScrubField>,
    pub sliders: Vec<SliderRow>,
    /// Dot/Sparkle/Cross — star fields only.
    pub form: Option<ChoiceRow>,
    /// Fill/Outline — absent for lines, paths and star fields.
    pub style: Option<ToggleRow>,
    /// Pure light instead of occluding. A checkbox rather than a
    /// `Normal | Additive` pair: `Normal` was never a choice, it was the
    /// absence of the other one, and it cost a whole row to say so.
    pub blend: Option<CheckRow>,
    /// The Effects tab's cards. Empty on the Settings tab.
    pub fx: Vec<effects::FxRow>,
}

/// A checkbox and the words beside it.
pub struct CheckRow {
    pub label: &'static str,
    pub check: spark_ui::Checkbox,
    pub on: bool,
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
    /// The effects-tab button, between the eye and the cog.
    pub fx_tab: Option<Viewport>,
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
    /// Fade the whole group. Its own full-width row rather than a fifth box
    /// in the strip: five boxes across a panel this wide are five boxes too
    /// narrow to read, and the strip matching a layer card's four is the
    /// thing that makes the two rows read as the same kind of object.
    pub fade: SliderRow,
}

pub(crate) fn kind_parts(kind: ShapeKind) -> (f32, &'static str) {
    match kind {
        ShapeKind::Circle => (ICON_CIRCLE, "circle"),
        ShapeKind::Box => (ICON_SQUARE, "box"),
        ShapeKind::Ngon => (ICON_PENTAGON, "polygon"),
        ShapeKind::Line => (ICON_LINE, "line"),
        ShapeKind::Path => (ICON_PATH, "path"),
        ShapeKind::Stars => (ICON_STARS, "stars"),
    }
}

/// Logical-px card metrics.
const HEAD_H: f32 = 46.0;
/// Inset from a value box's edge to its text.
pub(crate) const FIELD_PAD: f32 = 7.0;
pub(super) const SCRUB_H: f32 = 34.0;
/// Width of the label column to the left of a value box, logical px —
/// enough for the single letters X/Y/R/S plus a gap.
pub(crate) const SCRUB_LABEL_W: f32 = 20.0;
/// The same for the Z / Tilt / Turn strip, whose labels are words.
pub(super) const SPACE_LABEL_W: f32 = 56.0;
pub(super) const PAD: f32 = 10.0;
/// Between cards. The border plate overhangs the card by 2.5px a side, so
/// the gap you actually see is this minus 5.
pub(super) const GAP: f32 = 22.0;
/// Width reserved to the right of every slider for its readout, logical px.
/// Wide enough for a four-digit value at body size.
pub(crate) const VALUE_W: f32 = 72.0;
/// Gap between a slider track and its readout.
pub(crate) const VALUE_GAP: f32 = 10.0;

const SLIDER_H: f32 = 54.0;
const TOGGLE_H: f32 = 84.0;
/// A checkbox row: the box, and the air under it. A third of a segmented
/// pair's height, which is the whole reason it is one.
const CHECK_H: f32 = 46.0;
/// The box itself. Big — this is a target, and Alva reads from a distance.
pub(crate) const CHECK_SIDE: f32 = 30.0;
/// Gradient endpoint chips, on the Gradient effect's card.
pub(crate) const CHIPS_H: f32 = 52.0;
/// Folder header height, and how far its members indent.
pub(super) const FOLDER_H: f32 = 48.0;
const INDENT: f32 = 22.0;

pub struct Cards {
    pub rows: Vec<LayerRow>,
    pub folders: Vec<FolderRow>,
    /// Total content height (physical px), for scroll clamping.
    pub content_h: f32,
}

impl Cards {
    /// The box of the field being typed into. One lookup, so the caret, the
    /// focused style and a click that places the caret can never disagree
    /// about where the field is.
    pub fn focused_field(&self, e: EditField) -> Option<&ScrubField> {
        match e {
            EditField::Shape(i, prop) => {
                let lr = self.rows.iter().find(|lr| lr.index == i)?;
                lr.scrubs
                    .iter()
                    .chain(lr.detail.iter().flat_map(|d| d.scrubs.iter()))
                    .find(|f| f.prop == prop)
            }
            EditField::Folder(id, prop) => self
                .folders
                .iter()
                .find(|f| f.id == id)?
                .scrubs
                .iter()
                .find(|f| f.prop == prop),
        }
    }
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

/// `tab` says which half the expanded card is showing.
pub fn rows(
    panel: Viewport,
    scale: f32,
    ed: &Editor,
    open: Option<usize>,
    tab: CardTab,
    scroll: f32,
) -> Cards {
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
        // Left to right: eye, effects, cog. Group cards carry only an eye,
        // in the cog's slot.
        let fx_tab = (!grouped).then_some(Viewport {
            x: cog_slot.x - cog_side - 6.0 * scale,
            ..cog_slot
        });
        let eye = match fx_tab {
            Some(f) => Viewport {
                x: f.x - cog_side - 6.0 * scale,
                ..cog_slot
            },
            None => cog_slot,
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
                let fx = inner_x + (fw + fgap) * k as f32;
                let lw = SCRUB_LABEL_W * scale;
                scrubs.push(ScrubField {
                    prop,
                    rect: Viewport {
                        x: fx + lw,
                        y: cy,
                        w: (fw - lw).max(1.0),
                        h: SCRUB_H * scale,
                    },
                    label,
                    label_pos: [fx, cy],
                    label_w: lw,
                    value,
                    keyed: km & prop_bit(prop) != 0,
                });
            }
            cy += (SCRUB_H + 6.0) * scale;
        }

        let detail = expanded.then(|| {
            let keyed = |id: u32, param: u8| ed.fx_keyed(index, id, param);
            // The block's own top, before its contents advance the cursor.
            let top = cy;
            cy += PAD * scale;
            let mut d = detail(
                shape,
                ed.fx_of(index),
                &keyed,
                tab,
                inner_x,
                inner_w,
                scale,
                km,
                &mut cy,
            );
            cy += PAD * scale;
            d.panel = Viewport {
                x: inner_x - PAD * 0.5 * scale,
                y: top,
                w: inner_w + PAD * scale,
                h: (cy - top).max(1.0),
            };
            d
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
            fx_tab,
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
