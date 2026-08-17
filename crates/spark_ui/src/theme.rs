//! Spark's look: dark true-grey chrome (no blue bias), colorful accents.
//! Explicitly NOT the Lantern warm-brown — Spark has its own identity.
//!
//! Colors are named for **the job they do**, not for what they happen to
//! look like. `accent` is whatever marks a thing active; today that is gold,
//! and if it stops being gold the name still tells the truth. This matters
//! because the chrome once had three separate golds — `seam`, `playhead` and
//! `grad_gold`, the last two the same value under different names — and
//! `playhead` was doing duty as "selected" in eighteen places that had
//! nothing to do with the playhead. Nobody could have restyled that.
//!
//! Nothing outside this file invents a color — not one `srgb(0x…)` literal
//! survives anywhere else in the workspace, and it should stay that way. A
//! shade that isn't reachable from here is a shade a theme swap silently
//! misses, which is exactly how the editor ended up with its text colors
//! hardcoded three modules deep.

use std::sync::{LazyLock, RwLock};

use crate::surface::Surfaces;

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

/// Linear RGBA back to `RRGGBB` — the inverse of [`srgb`], for showing a
/// color as the code you'd have typed to get it.
pub fn hex_of(c: [f32; 4]) -> String {
    let channel = |v: f32| {
        let v = v.clamp(0.0, 1.0);
        let s = if v <= 0.0031308 {
            v * 12.92
        } else {
            1.055 * v.powf(1.0 / 2.4) - 0.055
        };
        (s * 255.0).round() as u32
    };
    format!(
        "{:02X}{:02X}{:02X}",
        channel(c[0]),
        channel(c[1]),
        channel(c[2])
    )
}

/// Parse `RRGGBB`, `#RRGGBB` or `RGB` into linear RGBA. Anything else is
/// `None` — a half-typed code just doesn't apply yet.
pub fn from_hex(s: &str) -> Option<[f32; 4]> {
    let s = s.trim().trim_start_matches('#').trim_start_matches("0x");
    let n = match s.len() {
        // Shorthand: each digit doubles, so `f0a` is `ff00aa`.
        3 => s.chars().try_fold(0u32, |acc, c| {
            let d = c.to_digit(16)?;
            Some(acc << 8 | d << 4 | d)
        })?,
        6 => u32::from_str_radix(s, 16).ok()?,
        _ => return None,
    };
    Some(srgb(n))
}

#[derive(Clone, Copy, Debug)]
pub struct Theme {
    // -- window regions ------------------------------------------------
    pub title: [f32; 4],
    pub toolbar: [f32; 4],
    pub panel: [f32; 4],
    pub timeline: [f32; 4],
    /// The near-black behind everything, before any panel paints.
    pub void: [f32; 4],
    /// The gutter around the stage — the largest area of colour on screen.
    /// `View > Black Background` overrides it with pure black.
    pub gutter: [f32; 4],

    // -- surfaces ------------------------------------------------------
    /// A card or plate sitting on a panel.
    pub card: [f32; 4],
    /// Resting card edge — lighter than the card so rows read as separate
    /// objects across the gaps. Selection swaps it for the accent.
    pub card_border: [f32; 4],
    /// A folder header: flatter than a card, so its members read as beneath.
    pub header: [f32; 4],
    /// The dark edge under a raised plate.
    pub plate_edge: [f32; 4],
    /// A sunken input: scrub fields, anything typed into.
    pub well: [f32; 4],
    /// A deeper well, for one already sitting inside a dark panel — the
    /// timeline's lane-name box.
    pub well_deep: [f32; 4],
    /// The editor-only transparency checkerboard under the stage.
    pub checker: [[f32; 4]; 2],
    /// The wash under a hovered control.
    pub button_hover: [f32; 4],
    /// The active half of a segmented toggle.
    pub segment_on: [f32; 4],
    pub close_hover: [f32; 4],

    // -- text ----------------------------------------------------------
    /// Primary label. Alva reads from a distance — keep the contrast high.
    pub text: [f32; 4],
    /// Secondary label: field names, counts, ruler marks.
    pub text_dim: [f32; 4],
    /// A hidden or disabled label.
    pub text_off: [f32; 4],

    // -- icons ---------------------------------------------------------
    pub icon: [f32; 4],
    pub icon_hover: [f32; 4],

    // -- accents, by role ----------------------------------------------
    /// **The** accent: selected, active, armed, keyed. Gold.
    pub accent: [f32; 4],
    /// The secondary accent. Purple. Used sparingly, by design.
    pub accent_alt: [f32; 4],
    /// A tinted backdrop behind a control the secondary accent marks.
    pub accent_alt_bg: [f32; 4],
    /// Panel seams — a dimmer gold than the accent, on purpose.
    pub seam: [f32; 4],
    /// Waveform strip — teal, because not everything is purple.
    pub wave: [f32; 4],
    /// The playhead line and the loop region it rules over.
    pub playhead: [f32; 4],
    /// Arrange + snapping. The one loud color the UI had spare.
    pub red: [f32; 4],
    /// Transport play: the glyph, the lit plate under a playing pause
    /// glyph, and the plate's resting and hovered greys.
    pub play: [f32; 4],
    pub play_bg: [f32; 4],
    pub play_rest: [f32; 4],
    pub play_hover: [f32; 4],

    // -- controls ------------------------------------------------------
    pub slider_track: [f32; 4],
    pub slider_thumb: [f32; 4],
    /// The slider's purple→gold fill ramp, in order.
    pub slider_fill: [[f32; 4]; 2],
}

/// The theme Spark ships with.
pub fn default_theme() -> Theme {
    Theme {
        title: srgb(0x0d0d0d),
        toolbar: srgb(0x1a1a1a),
        panel: srgb(0x151515),
        timeline: srgb(0x101010),
        void: srgb(0x0a0a0a),
        gutter: srgb(0x160d29),

        card: srgb(0x202020),
        card_border: srgb(0x3a3a3a),
        header: srgb(0x171717),
        plate_edge: srgb(0x0b0b0b),
        well: srgb(0x141414),
        well_deep: srgb(0x121212),
        checker: [srgb(0x191919), srgb(0x242424)],
        button_hover: srgb(0x2a2a2a),
        segment_on: srgb(0x3a3a3a),
        close_hover: srgb(0xc42b1c),

        text: srgb(0xf2f2f2),
        text_dim: srgb(0xb2b2b2),
        text_off: srgb(0x6a6a6a),

        icon: srgb(0xa2a2a2),
        icon_hover: srgb(0xf2f2f2),

        accent: srgb(0xffc800),
        accent_alt: srgb(0xc94df0),
        accent_alt_bg: srgb(0x2b1a35),
        seam: srgb(0xd4a017),
        wave: srgb(0x2bbfae),
        playhead: srgb(0xffc800),
        red: srgb(0xf04545),
        play: srgb(0x3fdc74),
        play_bg: srgb(0x1a4a2c),
        play_rest: srgb(0x343434),
        play_hover: srgb(0x424242),

        slider_track: srgb(0x242424),
        slider_thumb: srgb(0xededed),
        slider_fill: [srgb(0x5b21b6), srgb(0xffc800)],
    }
}

/// Colors plus the materials derived from them. One swap point.
struct Skin {
    theme: Theme,
    surfaces: Surfaces,
}

static SKIN: LazyLock<RwLock<Skin>> = LazyLock::new(|| {
    let theme = default_theme();
    RwLock::new(Skin {
        surfaces: Surfaces::from_theme(&theme),
        theme,
    })
});

/// The live palette.
///
/// Cheap on purpose: the sRGB→linear conversion runs once at startup rather
/// than on every call, because this is called from inside per-layer loops
/// and it used to redo sixty-odd `powf`s each time.
pub fn theme() -> Theme {
    SKIN.read().expect("theme lock").theme
}

/// The live materials — how a card, a well, a floating panel are painted.
pub fn surfaces() -> Surfaces {
    SKIN.read().expect("theme lock").surfaces
}

/// Swap the palette, rederiving every material from it. The editor reads
/// both fresh each redraw, so the next frame wears the new look — which is
/// the hook a live material editor needs.
pub fn set_theme(theme: Theme) {
    let mut skin = SKIN.write().expect("theme lock");
    skin.surfaces = Surfaces::from_theme(&theme);
    skin.theme = theme;
}

/// Override the materials without touching the palette — for tuning depth
/// and edges independently of color.
pub fn set_surfaces(surfaces: Surfaces) {
    SKIN.write().expect("theme lock").surfaces = surfaces;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn srgb_maps_the_endpoints() {
        assert_eq!(srgb(0x000000), [0.0, 0.0, 0.0, 1.0]);
        assert_eq!(srgb(0xffffff), [1.0, 1.0, 1.0, 1.0]);
        // Mid grey is well below 0.5 in linear light — the whole reason the
        // conversion exists.
        let mid = srgb(0x808080)[0];
        assert!((0.21..0.23).contains(&mid), "0x808080 -> {mid}");
    }

    /// The playground shows a colour as the code you would type to get it,
    /// so the round trip has to be exact or every swatch would drift a
    /// little each time it was read back and reapplied.
    #[test]
    fn hex_round_trips_exactly() {
        for code in [
            0x000000, 0xFFFFFF, 0x202020, 0x3A3A3A, 0xFFC800, 0xC94DF0, 0x0A0A0A, 0x808080,
        ] {
            let round = hex_of(srgb(code));
            assert_eq!(
                round,
                format!("{code:06X}"),
                "{code:06X} came back as {round}"
            );
        }
    }

    #[test]
    fn hex_parsing_takes_the_shapes_people_type() {
        let want = srgb(0xFF00AA);
        for form in [
            "FF00AA",
            "#FF00AA",
            "0xFF00AA",
            "ff00aa",
            " #ff00aa ",
            "F0A",
        ] {
            assert_eq!(from_hex(form), Some(want), "{form} did not parse");
        }
        for bad in ["", "#", "12345", "GGGGGG", "1234567", "nope"] {
            assert_eq!(from_hex(bad), None, "{bad:?} should not parse");
        }
    }

    #[test]
    fn channels_land_in_the_right_order() {
        let c = srgb(0xff0000);
        assert_eq!(
            [c[0] > 0.9, c[1] < 0.01, c[2] < 0.01, c[3] == 1.0],
            [true; 4]
        );
    }

    /// The whole point of the rename: one accent, reachable by one name.
    /// Reads the default rather than the live palette so it cannot race the
    /// swap test below — tests run in parallel and that one writes.
    #[test]
    fn the_accent_has_exactly_one_name() {
        let t = default_theme();
        assert_eq!(t.accent, t.playhead, "the playhead wears the accent");
        assert_eq!(t.accent, t.slider_fill[1], "so does the slider's far end");
        assert_ne!(t.accent, t.seam, "but the seam is its own, dimmer gold");
        assert_ne!(t.accent, t.accent_alt, "and the secondary is not gold");
    }

    /// The only test that touches the live skin, so it owns the global and
    /// puts it back. A palette swap has to carry into the materials too, or
    /// a recolor would leave every border wearing the old scheme.
    #[test]
    fn a_theme_swap_rederives_the_materials() {
        assert_eq!(theme().card, default_theme().card, "starts at the default");
        let mut t = default_theme();
        t.card = [1.0, 0.0, 0.0, 1.0];
        t.card_border = [0.0, 1.0, 0.0, 1.0];
        set_theme(t);
        assert_eq!(surfaces().card.fill, [1.0, 0.0, 0.0, 1.0], "fill followed");
        assert_eq!(
            surfaces().card.border_color,
            [0.0, 1.0, 0.0, 1.0],
            "and so did the border"
        );
        set_theme(default_theme());
        assert_eq!(surfaces().card.fill, default_theme().card, "restored");
    }
}
