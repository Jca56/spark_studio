//! A live swatch of the theme's elevation ladder, drawn in the left panel's
//! reserved space.
//!
//! It exists because the chrome is entirely greys, and greys chosen by feel
//! drift together until a button, the card it sits on, and the panel *that*
//! sits on are all the same colour. This is the reference: four surfaces
//! genuinely stacked, each one rung apart, plus a sunken well to show that
//! depth goes both ways. If a new surface doesn't obviously belong to one of
//! these rungs, it's the wrong colour.
//!
//! Delete this the day the left panel grows real tool options.

use spark_render::Viewport;
use spark_ui::{UiRect, theme};

use crate::chrome::UI_TEXT;

/// One label the chrome pass draws over the stack.
pub struct Label {
    pub text: String,
    pub pos: [f32; 2],
    pub size: f32,
}

pub struct Demo {
    pub rects: Vec<UiRect>,
    pub labels: Vec<Label>,
}

/// Each surface sits on a near-black edge, which is what actually sells the
/// separation — the shade change reads as height, the seam reads as an
/// object boundary.
fn plate(out: &mut Vec<UiRect>, r: Viewport, fill: [f32; 4], radius: f32, scale: f32) {
    let e = 2.0 * scale;
    out.push(UiRect::region_rounded(
        Viewport {
            x: r.x - e,
            y: r.y - e,
            w: r.w + e * 2.0,
            h: r.h + e * 2.0,
        },
        theme().edge,
        radius + e,
    ));
    out.push(UiRect::region_rounded(r, fill, radius));
}

pub fn build(left: Viewport, scale: f32) -> Demo {
    let t = theme();
    let mut rects = Vec::new();
    let mut labels = Vec::new();
    let size = UI_TEXT * 0.82 * scale;
    let small = UI_TEXT * 0.66 * scale;
    let pad = 18.0 * scale;

    let mut label = |text: &str, x: f32, y: f32, size: f32| {
        labels.push(Label {
            text: text.to_string(),
            pos: [x, y],
            size,
        });
    };

    label("ELEVATION", left.x + pad, left.y + pad, small);

    // Rung 1: a section on the panel.
    let section = Viewport {
        x: left.x + pad,
        y: left.y + pad + 26.0 * scale,
        w: (left.w - pad * 2.0).max(1.0),
        h: 280.0 * scale,
    };
    plate(&mut rects, section, t.raised, 12.0 * scale, scale);
    label(
        "raised — a section on the panel",
        section.x + 14.0 * scale,
        section.y + 12.0 * scale,
        small,
    );

    // Rung 2: a card on the section.
    let card = Viewport {
        x: section.x + 14.0 * scale,
        y: section.y + 40.0 * scale,
        w: (section.w - 28.0 * scale).max(1.0),
        h: 224.0 * scale,
    };
    plate(&mut rects, card, t.card, 10.0 * scale, scale);
    label(
        "card — on the section",
        card.x + 14.0 * scale,
        card.y + 12.0 * scale,
        small,
    );

    // Rung 3: a control on the card.
    let button = Viewport {
        x: card.x + 14.0 * scale,
        y: card.y + 40.0 * scale,
        w: (card.w - 28.0 * scale).max(1.0),
        h: 56.0 * scale,
    };
    plate(&mut rects, button, t.control, 9.0 * scale, scale);
    label(
        "control",
        button.x + 14.0 * scale,
        button.y + (button.h - size * 1.2) * 0.5,
        size,
    );

    // And down the other way: a well sunk *into* the card.
    let well = Viewport {
        x: card.x + 14.0 * scale,
        y: button.y + button.h + 14.0 * scale,
        w: button.w,
        h: 50.0 * scale,
    };
    plate(&mut rects, well, t.well, 8.0 * scale, scale);
    label(
        "well — sunk into it",
        well.x + 14.0 * scale,
        well.y + (well.h - small * 1.2) * 0.5,
        small,
    );

    // The rungs as flat chips, so the steps are visible side by side.
    let swatch_y = well.y + well.h + 16.0 * scale;
    let steps: [([f32; 4], &str); 6] = [
        (t.well, "well"),
        (t.sunken, "sunken"),
        (t.panel, "panel"),
        (t.raised, "raised"),
        (t.card, "card"),
        (t.control, "control"),
    ];
    let sw = (card.w - 28.0 * scale) / steps.len() as f32;
    for (i, (fill, _)) in steps.into_iter().enumerate() {
        rects.push(UiRect::region(
            Viewport {
                x: card.x + 14.0 * scale + sw * i as f32,
                y: swatch_y,
                w: sw,
                h: 26.0 * scale,
            },
            fill,
        ));
    }

    Demo { rects, labels }
}
