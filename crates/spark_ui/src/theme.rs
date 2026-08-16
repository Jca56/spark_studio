//! Spark's look: dark charcoal chrome, colorful accents to come.
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
    pub seam: [f32; 4],
    pub button_hover: [f32; 4],
    pub close_hover: [f32; 4],
    pub icon: [f32; 4],
    pub icon_hover: [f32; 4],
}

pub fn theme() -> Theme {
    Theme {
        title: srgb(0x0d0e11),
        toolbar: srgb(0x191b1f),
        panel: srgb(0x141519),
        timeline: srgb(0x0f1013),
        seam: srgb(0x26292f),
        button_hover: srgb(0x272a31),
        close_hover: srgb(0xc42b1c),
        icon: srgb(0x9aa0aa),
        icon_hover: srgb(0xf2f4f8),
    }
}
