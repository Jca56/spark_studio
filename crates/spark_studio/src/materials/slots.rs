//! What the playground can edit, as plain tables.
//!
//! Names here are **what you see on screen**, not what the field is called
//! in the code. "header" and "plate" meant nothing to anyone who hadn't read
//! `surface.rs`; "Folder header" and "Toolbar button" name the thing you can
//! point at. The code names live in the right-hand column, because they are
//! what a printed recipe has to say.

use spark_ui::{Surface, Theme};

type Get = fn(&Theme) -> [f32; 4];
type Set = fn(&mut Theme, [f32; 4]);

/// One editable color: where it lives in the palette, and what to call it.
pub struct Slot {
    /// The heading it files under in the grid.
    pub group: &'static str,
    /// What it is, in the words of someone looking at the screen.
    pub label: &'static str,
    pub get: Get,
    pub set: Set,
}

const fn slot(group: &'static str, label: &'static str, get: Get, set: Set) -> Slot {
    Slot {
        group,
        label,
        get,
        set,
    }
}

/// Every color the editor draws with. Grouped by where you'd go looking.
pub static SLOTS: &[Slot] = &[
    slot(
        "Window",
        "Around the canvas",
        |t| t.gutter,
        |t, c| t.gutter = c,
    ),
    slot("Window", "Title bar", |t| t.title, |t, c| t.title = c),
    slot("Window", "Side panels", |t| t.panel, |t, c| t.panel = c),
    slot(
        "Window",
        "Tool + transport bars",
        |t| t.toolbar,
        |t, c| t.toolbar = c,
    ),
    slot("Window", "Timeline", |t| t.timeline, |t, c| t.timeline = c),
    slot("Window", "Status strip", |t| t.status, |t, c| t.status = c),
    slot("Surfaces", "Layer card", |t| t.card, |t, c| t.card = c),
    slot(
        "Surfaces",
        "Folder header",
        |t| t.header,
        |t, c| t.header = c,
    ),
    slot("Surfaces", "Number field", |t| t.well, |t, c| t.well = c),
    slot(
        "Surfaces",
        "Lane name box",
        |t| t.well_deep,
        |t, c| t.well_deep = c,
    ),
    slot(
        "Surfaces",
        "Hover wash",
        |t| t.button_hover,
        |t, c| t.button_hover = c,
    ),
    slot(
        "Surfaces",
        "Toggle, active half",
        |t| t.segment_on,
        |t, c| t.segment_on = c,
    ),
    slot(
        "Surfaces",
        "Slider track",
        |t| t.slider_track,
        |t, c| t.slider_track = c,
    ),
    slot(
        "Edges",
        "Card border",
        |t| t.card_border,
        |t, c| t.card_border = c,
    ),
    slot(
        "Edges",
        "Button edge",
        |t| t.plate_edge,
        |t, c| t.plate_edge = c,
    ),
    slot("Edges", "Panel seams", |t| t.seam, |t, c| t.seam = c),
    slot("Text", "Label", |t| t.text, |t, c| t.text = c),
    slot(
        "Text",
        "Label, secondary",
        |t| t.text_dim,
        |t, c| t.text_dim = c,
    ),
    slot(
        "Text",
        "Label, hidden",
        |t| t.text_off,
        |t, c| t.text_off = c,
    ),
    slot("Text", "Icon", |t| t.icon, |t, c| t.icon = c),
    slot(
        "Text",
        "Icon, lit",
        |t| t.icon_hover,
        |t, c| t.icon_hover = c,
    ),
    slot(
        "Accents",
        "Selected / active",
        |t| t.accent,
        |t, c| t.accent = c,
    ),
    slot(
        "Accents",
        "Second accent",
        |t| t.accent_alt,
        |t, c| t.accent_alt = c,
    ),
    slot(
        "Accents",
        "Selected text wash",
        |t| t.accent_bg,
        |t, c| t.accent_bg = c,
    ),
    slot(
        "Accents",
        "Second accent wash",
        |t| t.accent_alt_bg,
        |t, c| t.accent_alt_bg = c,
    ),
    slot("Accents", "Playhead", |t| t.playhead, |t, c| t.playhead = c),
    slot("Accents", "Waveform", |t| t.wave, |t, c| t.wave = c),
    slot("Accents", "Arrange / snap red", |t| t.red, |t, c| t.red = c),
    slot(
        "Accents",
        "Close button hover",
        |t| t.close_hover,
        |t, c| t.close_hover = c,
    ),
    slot("Transport", "Play glyph", |t| t.play, |t, c| t.play = c),
    slot(
        "Transport",
        "Play plate, lit",
        |t| t.play_bg,
        |t, c| t.play_bg = c,
    ),
    slot(
        "Transport",
        "Play plate",
        |t| t.play_rest,
        |t, c| t.play_rest = c,
    ),
    slot(
        "Transport",
        "Play plate, hover",
        |t| t.play_hover,
        |t, c| t.play_hover = c,
    ),
    slot(
        "Controls",
        "Slider thumb",
        |t| t.slider_thumb,
        |t, c| t.slider_thumb = c,
    ),
    slot(
        "Controls",
        "Slider fill, low",
        |t| t.slider_fill[0],
        |t, c| t.slider_fill[0] = c,
    ),
    slot(
        "Controls",
        "Slider fill, high",
        |t| t.slider_fill[1],
        |t, c| t.slider_fill[1] = c,
    ),
    slot(
        "Controls",
        "Checkerboard, light",
        |t| t.checker[0],
        |t, c| t.checker[0] = c,
    ),
    slot(
        "Controls",
        "Checkerboard, dark",
        |t| t.checker[1],
        |t, c| t.checker[1] = c,
    ),
];

/// The seven shared materials: what you see, and the field a recipe names.
/// The color columns are the palette expressions a printed recipe uses, so
/// a baked recipe still follows a recolor.
pub const MATERIALS: [(&str, &str, &str, &str); 7] = [
    ("Layer card", "card", "t.card", "t.card_border"),
    ("Folder header", "header", "t.header", "t.card_border"),
    ("Toolbar button", "plate", "t.card", "t.plate_edge"),
    ("Number field", "well", "t.well", "t.card_border"),
    ("Menu popup", "float", "t.card", "t.seam"),
    ("Text input", "field", "t.slider_track", "t.seam"),
    (
        "Hover highlight",
        "hover",
        "t.button_hover",
        "t.card_border",
    ),
];

/// One tunable number on a surface.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Knob {
    Radius,
    Border,
    Shade,
    Grain,
    BevelTop,
    BevelBottom,
    BevelSize,
    ShadowDrop,
    ShadowBlur,
    ShadowDark,
    InnerDrop,
    InnerBlur,
    InnerDark,
}

/// Every knob: its heading, what it does in plain words, and the top of its
/// range. The old labels ("Bevel light", "Shade") described the code rather
/// than the effect, which made the panel unreadable to anyone who hadn't
/// written it.
pub const KNOBS: [(Knob, &str, &str, f32); 13] = [
    (Knob::Radius, "Shape", "Corner rounding", 30.0),
    (Knob::Border, "Shape", "Border thickness", 8.0),
    (Knob::Shade, "Shape", "Darken toward bottom", 1.0),
    (Knob::Grain, "Shape", "Surface texture", 0.25),
    (Knob::BevelTop, "Edge light", "Highlight along top", 1.0),
    (Knob::BevelBottom, "Edge light", "Shadow along bottom", 1.0),
    (Knob::BevelSize, "Edge light", "How far it reaches in", 12.0),
    (Knob::ShadowDrop, "Drop shadow", "Offset down", 16.0),
    (Knob::ShadowBlur, "Drop shadow", "Softness", 40.0),
    (Knob::ShadowDark, "Drop shadow", "Strength", 1.0),
    (Knob::InnerDrop, "Inner shadow", "Offset down", 16.0),
    (Knob::InnerBlur, "Inner shadow", "Softness", 40.0),
    (Knob::InnerDark, "Inner shadow", "Strength", 1.0),
];

pub fn get(s: &Surface, k: Knob) -> f32 {
    match k {
        Knob::Radius => s.radius,
        Knob::Border => s.border,
        Knob::Grain => s.grain,
        Knob::BevelTop => s.bevel[0],
        Knob::BevelBottom => s.bevel[1],
        Knob::BevelSize => s.bevel[2],
        Knob::ShadowDrop => s.shadow[0],
        Knob::ShadowBlur => s.shadow[1],
        Knob::ShadowDark => s.shadow[2],
        Knob::InnerDrop => s.inner[0],
        Knob::InnerBlur => s.inner[1],
        Knob::InnerDark => s.inner[2],
        // Not a stored number: it's how far the gradient's far end sits
        // below the fill, read back off the two colors.
        Knob::Shade => shade_of(s),
    }
}

pub fn set(s: &mut Surface, k: Knob, v: f32) {
    match k {
        Knob::Radius => s.radius = v,
        Knob::Border => s.border = v,
        Knob::Grain => s.grain = v,
        Knob::BevelTop => s.bevel[0] = v,
        Knob::BevelBottom => s.bevel[1] = v,
        Knob::BevelSize => s.bevel[2] = v,
        Knob::ShadowDrop => s.shadow[0] = v,
        Knob::ShadowBlur => s.shadow[1] = v,
        Knob::ShadowDark => s.shadow[2] = v,
        Knob::InnerDrop => s.inner[0] = v,
        Knob::InnerBlur => s.inner[1] = v,
        Knob::InnerDark => s.inner[2] = v,
        Knob::Shade => {
            s.fill_to = if v <= 0.005 {
                [0.0; 4]
            } else {
                spark_ui::darken(s.fill, v)
            }
        }
    }
}

pub fn shade_of(s: &Surface) -> f32 {
    if s.fill_to[3] <= 0.0 {
        return 0.0;
    }
    let hi = s.fill[..3].iter().copied().fold(0.0f32, f32::max).max(1e-4);
    let lo = s.fill_to[..3].iter().copied().fold(0.0f32, f32::max);
    ((1.0 - lo / hi) / spark_ui::SHADE_DEPTH).clamp(0.0, 1.0)
}

pub fn format_value(knob: Knob, v: f32) -> String {
    match knob {
        // The 0..1 knobs read as percentages; the rest are logical px.
        Knob::Shade
        | Knob::Grain
        | Knob::BevelTop
        | Knob::BevelBottom
        | Knob::ShadowDark
        | Knob::InnerDark => format!("{}%", (v * 100.0).round()),
        _ => format!("{v:.1}"),
    }
}
