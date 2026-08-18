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
    slot("Surfaces", "Card", |t| t.card, |t, c| t.card = c),
    slot(
        "Surfaces",
        "Inner card",
        |t| t.card_inner,
        |t, c| t.card_inner = c,
    ),
    slot(
        "Surfaces",
        "Effect card",
        |t| t.fx_card,
        |t, c| t.fx_card = c,
    ),
    slot("Surfaces", "Button", |t| t.button, |t, c| t.button = c),
    slot("Surfaces", "Menu popup", |t| t.popup, |t, c| t.popup = c),
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
    // Named for the job, not for a rank. "Primary / Dimmed / Hidden" sorted
    // them by importance, which told you nothing about *which* text you were
    // about to change — and left the odd-looking result that the value in a
    // box was "primary" while the label naming that box was "dimmed". They
    // are not ranked. They answer different questions: one says what a thing
    // *is*, the other says what a field is *called*.
    slot("Text", "Names and values", |t| t.text, |t, c| t.text = c),
    slot(
        "Text",
        "Field labels",
        |t| t.text_dim,
        |t, c| t.text_dim = c,
    ),
    slot("Icons", "Icon", |t| t.icon, |t, c| t.icon = c),
    slot(
        "Icons",
        "Icon, lit",
        |t| t.icon_hover,
        |t, c| t.icon_hover = c,
    ),
    // Not a rank either, and not only text: this is what a *switched-off*
    // thing wears — a hidden layer's name and eye, an effect toggled off.
    // It filed under "Text" as "Hidden", which read as a colour for
    // something invisible; it is the colour of something you can still see
    // and that is turned off.
    slot(
        "Switched off",
        "Hidden layer, off effect",
        |t| t.text_off,
        |t, c| t.text_off = c,
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
pub const MATERIALS: [(&str, &str, &str, &str); 13] = [
    // The window regions first: they are the largest areas on screen, and
    // a look starts with the surface everything else sits on.
    ("Side panels", "panel", "t.panel", "t.seam"),
    ("Tool + transport bars", "bar", "t.toolbar", "t.seam"),
    ("Timeline", "timeline", "t.timeline", "t.seam"),
    ("Status strip", "status", "t.status", "t.seam"),
    // Then the boxes, outermost first. "Layer card" named this for one of
    // its two callers — a timeline lane is the same box — and implied it
    // was about layers specifically. It is the generic container.
    ("Card", "card", "t.card", "t.card_border"),
    ("Inner card", "card_inner", "t.card_inner", "t.card_border"),
    ("Effect card", "fx_card", "t.fx_card", "t.card_border"),
    ("Folder header", "header", "t.header", "t.card_border"),
    // Buttons used to be painted with the card colour, so recolouring a
    // layer card recoloured the whole toolbar with it.
    ("Button", "plate", "t.button", "t.plate_edge"),
    ("Menu popup", "float", "t.popup", "t.seam"),
    // These two were "Number field" and "Text input", which named neither
    // where you see them nor how they differ. They are the same box in two
    // states: at rest, and while a caret is sitting in it.
    ("Number box", "well", "t.well", "t.card_border"),
    ("Number box, typing", "field", "t.slider_track", "t.seam"),
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
    /// Which way the gradient runs, in turns.
    GradAngle,
    /// Where along the surface the blend starts and finishes, 0..1.
    GradStart,
    GradEnd,
    /// Center→corners instead of along a direction. A switch, stored as a
    /// number, the same way an effect's `On` parameter is.
    GradRadial,
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
pub const KNOBS: [(Knob, &str, &str, f32); 16] = [
    (Knob::Radius, "Shape", "Corner rounding", 30.0),
    (Knob::Border, "Shape", "Border thickness", 8.0),
    (Knob::Grain, "Shape", "Surface texture", 0.25),
    // The end *colour* is the gradient, and it lives in the field above
    // these. There used to be a "Darken toward far end" knob beside it that
    // derived the colour from the fill, and two controls owning one value
    // could only fight: picking a colour made the knob read back a number
    // that meant nothing, and nudging the knob then computed a fresh colour
    // over the one you picked. A shortcut is not worth a control that
    // undoes you.
    (Knob::GradAngle, "Gradient", "Direction", 1.0),
    (Knob::GradStart, "Gradient", "Fade starts at", 1.0),
    (Knob::GradEnd, "Gradient", "Fade ends at", 1.0),
    (Knob::GradRadial, "Gradient", "Radial", 1.0),
    // Strength is the alpha of a *pure white* (or pure black) rim laid over
    // the fill, so 1.0 wipes the surface out entirely and everything usable
    // lived in the first twentieth of the slider. These maxima put the band
    // that reads as lit across the whole travel; the number still means
    // what it says — 5% is 5% white — there is just no longer 95% of a
    // slider spent on values nobody can use.
    (Knob::BevelTop, "Edge light", "Highlight along top", 0.2),
    (Knob::BevelBottom, "Edge light", "Shadow along bottom", 0.2),
    // And reach is in logical px, which has to serve a side panel as well
    // as a 44px card. Twelve was a third of a card and invisible on a
    // panel.
    (Knob::BevelSize, "Edge light", "How far it reaches in", 80.0),
    (Knob::ShadowDrop, "Drop shadow", "Offset down", 40.0),
    (Knob::ShadowBlur, "Drop shadow", "Softness", 120.0),
    (Knob::ShadowDark, "Drop shadow", "Strength", 1.0),
    (Knob::InnerDrop, "Inner shadow", "Offset down", 40.0),
    (Knob::InnerBlur, "Inner shadow", "Softness", 120.0),
    (Knob::InnerDark, "Inner shadow", "Strength", 1.0),
];

impl Knob {
    /// Whether this knob is a switch rather than an amount. A switch still
    /// rides a slider — every control here is one number — but it only ever
    /// reads back 0 or 1, so nothing may expect a value it was handed to
    /// come back unchanged.
    pub fn is_switch(self) -> bool {
        matches!(self, Knob::GradRadial)
    }
}

pub fn get(s: &Surface, k: Knob) -> f32 {
    match k {
        Knob::Radius => s.radius,
        Knob::Border => s.border,
        Knob::Grain => s.grain,
        Knob::GradAngle => s.grad[0],
        Knob::GradRadial => s.grad[1],
        Knob::GradStart => s.grad_span[0],
        Knob::GradEnd => s.grad_span[1],
        Knob::BevelTop => s.bevel[0],
        Knob::BevelBottom => s.bevel[1],
        Knob::BevelSize => s.bevel[2],
        Knob::ShadowDrop => s.shadow[0],
        Knob::ShadowBlur => s.shadow[1],
        Knob::ShadowDark => s.shadow[2],
        Knob::InnerDrop => s.inner[0],
        Knob::InnerBlur => s.inner[1],
        Knob::InnerDark => s.inner[2],
    }
}

pub fn set(s: &mut Surface, k: Knob, v: f32) {
    match k {
        Knob::Radius => s.radius = v,
        Knob::Border => s.border = v,
        Knob::Grain => s.grain = v,
        Knob::GradAngle => s.grad[0] = v,
        // Kept in order, so dragging one past the other pushes rather than
        // inverting the band — an end before its start is not a gradient.
        Knob::GradStart => s.grad_span = [v, s.grad_span[1].max(v)],
        Knob::GradEnd => s.grad_span = [s.grad_span[0].min(v), v],
        // A switch: anything past halfway is on, so a slider can drive it.
        Knob::GradRadial => s.grad[1] = (v > 0.5) as u32 as f32,
        Knob::BevelTop => s.bevel[0] = v,
        Knob::BevelBottom => s.bevel[1] = v,
        Knob::BevelSize => s.bevel[2] = v,
        Knob::ShadowDrop => s.shadow[0] = v,
        Knob::ShadowBlur => s.shadow[1] = v,
        Knob::ShadowDark => s.shadow[2] = v,
        Knob::InnerDrop => s.inner[0] = v,
        Knob::InnerBlur => s.inner[1] = v,
        Knob::InnerDark => s.inner[2] = v,
    }
}

pub fn format_value(knob: Knob, v: f32) -> String {
    // Asked of the knob rather than listed here, so a second switch is one
    // line in the table and nothing else.
    if knob.is_switch() {
        return match v > 0.5 {
            true => "On".into(),
            false => "Off".into(),
        };
    }
    match knob {
        // Turns are the storage; degrees are what anyone reads.
        Knob::GradAngle => format!("{}\u{00b0}", (v * 360.0).round()),
        // The 0..1 knobs read as percentages; the rest are logical px.
        Knob::GradStart
        | Knob::GradEnd
        | Knob::Grain
        | Knob::BevelTop
        | Knob::BevelBottom
        | Knob::ShadowDark
        | Knob::InnerDark => format!("{}%", (v * 100.0).round()),
        _ => format!("{v:.1}"),
    }
}
