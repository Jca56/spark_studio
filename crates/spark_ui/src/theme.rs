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

pub struct Theme {
    pub title: [f32; 4],
    pub toolbar: [f32; 4],
    pub panel: [f32; 4],
    pub timeline: [f32; 4],
    /// Panel borders — gold, for funsies (easy revert to 0x272727 grey).
    pub seam: [f32; 4],
    /// Raised-card background for list rows sitting on a panel.
    pub card: [f32; 4],
    /// Resting card edge — lighter than the card so rows read as separate
    /// objects across the gaps. Selection swaps it for gold.
    pub card_border: [f32; 4],
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
        toolbar: srgb(0x1a1a1a),
        panel: srgb(0x151515),
        timeline: srgb(0x101010),
        seam: srgb(0xd4a017),
        card: srgb(0x202020),
        card_border: srgb(0x3a3a3a),
        button_hover: srgb(0x2a2a2a),
        close_hover: srgb(0xc42b1c),
        icon: srgb(0xa2a2a2),
        icon_hover: srgb(0xf2f2f2),
        accent: srgb(0xc94df0),
        accent_bg: srgb(0x2b1a35),
        slider_track: srgb(0x242424),
        slider_thumb: srgb(0xededed),
        grad_purple: srgb(0x5b21b6),
        grad_gold: srgb(0xffc800),
        wave: srgb(0x2bbfae),
        playhead: srgb(0xffc800),
        red: srgb(0xf04545),
    }
}
