//! Spark's look: dark true-grey chrome (no blue bias), colorful accents.
//! Explicitly NOT the Lantern warm-brown — Spark has its own identity.

/// Convert an 0xRRGGBB sRGB color to linear RGBA for the render pipeline.
pub fn srgb(hex: u32) -> [f32; 4] {
    let channel = |shift: u32| {
        let c = ((hex >> shift) & 0xff) as f32 / 255.0;
        if c <= 0.04045 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    };
    [channel(16), channel(8), channel(0), 1.0]
}

/// **The elevation ladder.** Spark's chrome is all greys, so "this sits on
/// top of that" has to come from contrast alone — and contrast that small
/// isn't contrast. Every step below is ~12–16 sRGB units, which reads as a
/// real height change; anything tighter reads as one flat colour.
///
/// Nothing outside this file should invent a grey. Pick the rung that
/// matches how *deep in the stack* the surface is:
///
/// ```text
///   well     inputs, slider tracks — sunken *below* their container
///   sunken   recessed regions (the timeline pit)
///   panel    a base panel on the window
///   raised   a section sitting on a panel
///   card     a card sitting on a section
///   control  a button sitting on a card
/// ```
pub struct Theme {
    pub title: [f32; 4],
    pub toolbar: [f32; 4],
    pub panel: [f32; 4],
    pub timeline: [f32; 4],
    /// Sunken: an input well, below whatever contains it.
    pub well: [f32; 4],
    /// Recessed region — a pit in the chrome.
    pub sunken: [f32; 4],
    /// A section sitting on a panel.
    pub raised: [f32; 4],
    /// Panel borders — gold, for funsies (easy revert to 0x272727 grey).
    pub seam: [f32; 4],
    /// A card sitting on a section.
    pub card: [f32; 4],
    /// Resting card edge — lighter than the card so rows read as separate
    /// objects across the gaps. Selection swaps it for gold.
    pub card_border: [f32; 4],
    /// The near-black seam that separates stacked surfaces.
    pub edge: [f32; 4],
    /// A button sitting on a card.
    pub control: [f32; 4],
    pub button_hover: [f32; 4],
    pub close_hover: [f32; 4],
    pub icon: [f32; 4],
    pub icon_hover: [f32; 4],
    pub accent: [f32; 4],
    pub accent_bg: [f32; 4],
    pub slider_track: [f32; 4],
    pub slider_thumb: [f32; 4],
    /// Gradient endpoints: Alva's purple → Lantern gold.
    pub grad_purple: [f32; 4],
    pub grad_gold: [f32; 4],
    /// Waveform strip — teal, because not everything is purple.
    pub wave: [f32; 4],
    /// Playhead — gold, unmissable over the teal.
    pub playhead: [f32; 4],
    /// Arrange + snapping accent. The one loud color the UI had spare.
    pub red: [f32; 4],
}

pub fn theme() -> Theme {
    Theme {
        title: srgb(0x0d0d0d),
        toolbar: srgb(0x202020),
        panel: srgb(0x151515),
        timeline: srgb(0x101010),
        well: srgb(0x0d0d0d),
        sunken: srgb(0x101010),
        raised: srgb(0x202020),
        seam: srgb(0xd4a017),
        card: srgb(0x2c2c2c),
        card_border: srgb(0x454545),
        edge: srgb(0x080808),
        control: srgb(0x3a3a3a),
        button_hover: srgb(0x4a4a4a),
        close_hover: srgb(0xc42b1c),
        icon: srgb(0xa2a2a2),
        icon_hover: srgb(0xf2f2f2),
        accent: srgb(0xc94df0),
        accent_bg: srgb(0x2b1a35),
        slider_track: srgb(0x141414),
        slider_thumb: srgb(0xededed),
        grad_purple: srgb(0x5b21b6),
        grad_gold: srgb(0xffc800),
        wave: srgb(0x2bbfae),
        playhead: srgb(0xffc800),
        red: srgb(0xf04545),
    }
}
