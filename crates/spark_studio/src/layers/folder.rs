//! Folder header rows: the disclosure box, name, eye, and the folder
//! transform's X/Y/R/S strip — laid out exactly like a layer card so the two
//! read as the same kind of object.

use spark_render::Viewport;

use crate::anim::prop_bit;
use crate::chrome::UI_TEXT;
use crate::editor::{Editor, Prop};

use super::{FOLDER_H, FolderRow, GAP, PAD, SCRUB_H, ScrubField, SliderRow};

/// Lay out one folder's card, advancing `y` past it. `None` if the id has
/// gone stale between layout passes.
pub(super) fn row(
    panel: Viewport,
    scale: f32,
    ed: &Editor,
    id: u32,
    y: &mut f32,
) -> Option<FolderRow> {
    let pad = 12.0 * scale;
    let y0 = *y;

    let f = ed.folder(id)?;
    let members = ed.folder_members(id);
    // Laid out exactly like a layer card: header strip on top,
    // transform strip beneath, both inside one bordered plate.
    let card_x = panel.x + pad;
    let card_w = panel.w - pad * 2.0;
    let head = Viewport {
        x: card_x,
        y: y0 + PAD * scale,
        w: card_w,
        h: FOLDER_H * scale,
    };
    // Right to left: disclosure, then eye. The disclosure takes the slot a
    // layer card puts its cogwheel in, because that corner is where "open
    // this thing" already lives.
    let side = 34.0 * scale;
    let disclose = Viewport {
        x: head.x + head.w - PAD * scale - side,
        y: head.y + (head.h - side) * 0.5,
        w: side,
        h: side,
    };
    let eye = Viewport {
        x: disclose.x - side - 6.0 * scale,
        y: disclose.y,
        w: side,
        h: side,
    };
    let inner_x = card_x + PAD * scale;
    let inner_w = card_w - PAD * 2.0 * scale;
    let fgap = 6.0 * scale;
    let fw = (inner_w - fgap * 3.0) / 4.0;
    let km = f.anim.keyed_mask();
    let sy = head.y + head.h + 6.0 * scale;
    let fields: [(Prop, &str, String); 4] = [
        (Prop::X, "X", format!("{:.0}", f.x)),
        (Prop::Y, "Y", format!("{:.0}", f.y)),
        (
            Prop::Rotation,
            "R",
            format!("{:.0}", f.rotation.to_degrees()),
        ),
        (Prop::Scale, "S", format!("{:.2}", f.scale)),
    ];
    let scrubs = fields
        .into_iter()
        .enumerate()
        .map(|(k, (prop, label, value))| {
            let fx = inner_x + (fw + fgap) * k as f32;
            let lw = super::SCRUB_LABEL_W * scale;
            ScrubField {
                prop,
                rect: Viewport {
                    x: fx + lw,
                    y: sy,
                    w: (fw - lw).max(1.0),
                    h: SCRUB_H * scale,
                },
                label,
                label_pos: [fx, sy],
                value,
                keyed: km & prop_bit(prop) != 0,
            }
        })
        .collect();
    // Under the strip, laid out exactly like a card's detail sliders so the
    // two read the same: label left, track full width, readout right.
    let fy = sy + (SCRUB_H + 10.0) * scale;
    let fade = SliderRow {
        prop: Prop::Opacity,
        label: "Opacity",
        label_pos: [inner_x, fy],
        track: Viewport {
            x: inner_x,
            y: fy + 30.0 * scale,
            w: (inner_w - (super::VALUE_W + super::VALUE_GAP) * scale).max(1.0),
            h: 10.0 * scale,
        },
        t: f.opacity.clamp(0.0, 1.0),
        value: format!("{:.0}%", f.opacity * 100.0),
        value_right: inner_x + inner_w,
        keyed: km & prop_bit(Prop::Opacity) != 0,
    };
    let row = Viewport {
        x: card_x,
        y: y0,
        w: card_w,
        h: (fade.track.y + fade.track.h + PAD * 1.6 * scale - y0).max(1.0),
    };
    *y = row.y + row.h + GAP * scale;
    Some(FolderRow {
        id,
        row,
        head,
        disclose,
        eye,
        label_pos: [
            head.x + PAD * scale,
            head.y + (head.h - UI_TEXT * 1.2 * scale) * 0.5,
        ],
        label: f.name.clone(),
        collapsed: f.collapsed,
        hidden: f.hidden,
        selected: !members.is_empty() && members.iter().all(|m| ed.selection().contains(m)),
        count: members.len(),
        scrubs,
        fade,
    })
}
