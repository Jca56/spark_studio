//! Elevation demo for the left panel: background -> panel -> card -> button.
//!
//! These shades are **local to this demo** and deliberately not wired into
//! the theme — it's a proposal to look at, not a change to the app.
//!
//! The background is the locked panel grey. Every step above it is ~28 sRGB
//! units, because the chrome's real problem was steps of 2–11 that read as
//! one flat colour. If a stack needs three levels, the top one has to be
//! *obviously* lighter than the bottom, not technically lighter.

use spark_render::Viewport;
use spark_ui::{UiRect, srgb};

use crate::chrome::UI_TEXT;

/// The stack, bottom to top. The background is `theme().panel`, untouched.
const PANEL: u32 = 0x313131;
const CARD: u32 = 0x4f4f4f;
const BUTTON: u32 = 0x6d6d6d;
/// The near-black seam under each surface — the shade change reads as
/// height, the seam reads as an edge.
const SEAM: u32 = 0x080808;

/// One label the chrome pass draws over the stack.
pub struct Label {
    pub text: &'static str,
    pub pos: [f32; 2],
    pub size: f32,
}

pub struct Demo {
    pub rects: Vec<UiRect>,
    pub labels: Vec<Label>,
}

fn plate(out: &mut Vec<UiRect>, r: Viewport, fill: u32, radius: f32, scale: f32) {
    let e = 2.0 * scale;
    out.push(UiRect::region_rounded(
        Viewport {
            x: r.x - e,
            y: r.y - e,
            w: r.w + e * 2.0,
            h: r.h + e * 2.0,
        },
        srgb(SEAM),
        radius + e,
    ));
    out.push(UiRect::region_rounded(r, srgb(fill), radius));
}

pub fn build(left: Viewport, scale: f32) -> Demo {
    let mut rects = Vec::new();
    let mut labels = Vec::new();
    // Body size. Never below it.
    let size = UI_TEXT * scale;
    let line = size * 1.35;
    let pad = 20.0 * scale;

    // 1. A panel on the background.
    let panel = Viewport {
        x: left.x + pad,
        y: left.y + pad,
        w: (left.w - pad * 2.0).max(1.0),
        h: 320.0 * scale,
    };
    plate(&mut rects, panel, PANEL, 14.0 * scale, scale);
    labels.push(Label {
        text: "panel",
        pos: [panel.x + 20.0 * scale, panel.y + 18.0 * scale],
        size,
    });

    // 2. A card on that panel.
    let card = Viewport {
        x: panel.x + 20.0 * scale,
        y: panel.y + 18.0 * scale + line,
        w: (panel.w - 40.0 * scale).max(1.0),
        h: 210.0 * scale,
    };
    plate(&mut rects, card, CARD, 12.0 * scale, scale);
    labels.push(Label {
        text: "card",
        pos: [card.x + 20.0 * scale, card.y + 18.0 * scale],
        size,
    });

    // 3. A button in that card.
    let button = Viewport {
        x: card.x + 20.0 * scale,
        y: card.y + 18.0 * scale + line,
        w: (card.w - 40.0 * scale).max(1.0),
        h: 80.0 * scale,
    };
    plate(&mut rects, button, BUTTON, 10.0 * scale, scale);
    labels.push(Label {
        text: "button",
        pos: [button.x + 20.0 * scale, button.y + (button.h - line) * 0.5],
        size,
    });

    Demo { rects, labels }
}
