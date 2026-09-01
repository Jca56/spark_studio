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

/// One 0..255 sRGB channel as linear light.
fn to_linear(byte: u32) -> f32 {
    let c = (byte & 0xff) as f32 / 255.0;
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

/// The inverse — linear light back to an 0..255 sRGB channel.
fn to_byte(v: f32) -> u32 {
    let v = v.clamp(0.0, 1.0);
    let s = if v <= 0.0031308 {
        v * 12.92
    } else {
        1.055 * v.powf(1.0 / 2.4) - 0.055
    };
    (s * 255.0).round() as u32
}

/// Convert an 0xRRGGBB sRGB color to linear RGBA for the render pipeline.
/// Fully opaque — see [`srgba`] for a color that lets what is behind it
/// through.
pub fn srgb(hex: u32) -> [f32; 4] {
    [
        to_linear(hex >> 16),
        to_linear(hex >> 8),
        to_linear(hex),
        1.0,
    ]
}

/// Convert an 0xRRGGBBAA sRGB color to linear RGBA.
///
/// Alpha is **not** gamma-decoded, unlike the color channels: it is a
/// coverage fraction rather than an amount of light, and running it through
/// the sRGB curve would make a half-transparent surface composite at 21%
/// instead of the 50% the code says.
pub fn srgba(hex: u32) -> [f32; 4] {
    [
        to_linear(hex >> 24),
        to_linear(hex >> 16),
        to_linear(hex >> 8),
        (hex & 0xff) as f32 / 255.0,
    ]
}

/// Linear RGBA back to `RRGGBB` — the inverse of [`srgb`], for showing a
/// color as the code you'd have typed to get it.
///
/// An opaque color still prints six digits, so every code that ever worked
/// still reads the way it always did; only a transparent one grows the two
/// extra digits, which is itself how the panel says it is transparent.
pub fn hex_of(c: [f32; 4]) -> String {
    let rgb = format!(
        "{:02X}{:02X}{:02X}",
        to_byte(c[0]),
        to_byte(c[1]),
        to_byte(c[2])
    );
    if c[3] >= 1.0 {
        rgb
    } else {
        format!("{rgb}{:02X}", (c[3].clamp(0.0, 1.0) * 255.0).round() as u32)
    }
}

/// Parse `RRGGBB`, `RRGGBBAA`, `#RRGGBB`, `RGB` or `RGBA` into linear RGBA.
/// Anything else is `None` — a half-typed code just doesn't apply yet, which
/// is what lets the editor restyle itself while you are still typing.
pub fn from_hex(s: &str) -> Option<[f32; 4]> {
    let s = s.trim().trim_start_matches('#').trim_start_matches("0x");
    // Shorthand: each digit doubles, so `f0a` is `ff00aa` and `f0a8` is
    // `ff00aa88`.
    let doubled = |s: &str| -> Option<u32> {
        s.chars()
            .try_fold(0u32, |acc, c| Some((acc << 8) | (c.to_digit(16)? * 0x11)))
    };
    match s.len() {
        3 => Some(srgb(doubled(s)?)),
        4 => Some(srgba(doubled(s)?)),
        6 => Some(srgb(u32::from_str_radix(s, 16).ok()?)),
        8 => Some(srgba(u32::from_str_radix(s, 16).ok()?)),
        _ => None,
    }
}

/// Alva's grey ladder, plus the two accents — the rungs every chrome
/// surface in Spark is actually drawn from.
///
/// Offered as swatches whenever the picker is painting the editor's own
/// look. "I don't know any colour codes" is the correct response to a hex
/// field, and a palette of the shades this editor is *already* built from
/// is the answer to it: a click lands on a rung that is known to work
/// beside the others, instead of a number nobody can picture.
pub const LADDER: [u32; 10] = [
    0x0F0F19, 0x151515, 0x1B1B18, 0x2A2A2A, 0x414141, 0x504E4E, 0x555555, 0x888888, 0xFFC800,
    0xC94DF0,
];

/// The ladder as linear RGB, ready for a swatch row.
pub fn ladder() -> Vec<[f32; 3]> {
    LADDER
        .iter()
        .map(|&h| {
            let c = srgb(h);
            [c[0], c[1], c[2]]
        })
        .collect()
}

#[derive(Clone, Copy, Debug)]
pub struct Theme {
    // -- window regions ------------------------------------------------
    pub title: [f32; 4],
    pub toolbar: [f32; 4],
    pub panel: [f32; 4],
    pub timeline: [f32; 4],
    /// The status strip along the bottom. A rung *below* the panels rather
    /// than above them: it reads as the floor the window sits on, which is
    /// what closes the layout without a rule drawn across it.
    pub status: [f32; 4],
    /// The near-black behind everything, before any panel paints.
    pub void: [f32; 4],
    /// The gutter around the stage — the largest area of colour on screen.
    /// `View > Black Background` overrides it with pure black.
    pub gutter: [f32; 4],

    // -- surfaces ------------------------------------------------------
    /// A card sitting on a panel: a layer card, a timeline lane. Not
    /// "layer card" — the same box holds a lane, and naming it for one
    /// caller made the other look like a bug.
    pub card: [f32; 4],
    /// An effect's card, on the settings block. Its own colour because it
    /// borrowed the layer card's until 2026-08-18, which meant recolouring
    /// a layer card recoloured every effect on it too — the same bug
    /// buttons had.
    pub fx_card: [f32; 4],
    /// A card nested *inside* a card — the cog-expanded settings block.
    /// The z-order goes panel → card → inner card → control, and this is
    /// the rung that was missing: the settings drawer used to be painted by
    /// whatever the card behind it happened to be.
    pub card_inner: [f32; 4],
    /// A raised button: toolbar squares, the transport, the keyframe stamp.
    /// Borrowed `card` until 2026-08-18, which meant recolouring a layer
    /// card recoloured every button in the editor.
    pub button: [f32; 4],
    /// A panel floating over the chrome: menus, popups. Also borrowed
    /// `card`.
    pub popup: [f32; 4],
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
    /// A tinted backdrop the *primary* accent marks — selected text. Deep
    /// enough that a near-white glyph still reads on top of it.
    pub accent_bg: [f32; 4],
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
    /// A knob cap's face, lit from above: `knob_cap_hi` at the top edge
    /// shading to `knob_cap_lo` at the bottom (Lantern Mix's dial).
    pub knob_cap_hi: [f32; 4],
    pub knob_cap_lo: [f32; 4],
    /// The groove a knob's value arc rides in.
    pub knob_track: [f32; 4],
}

/// The theme Spark ships with.
pub fn default_theme() -> Theme {
    Theme {
        // Alva's ladder, 2026-08-17. Eight steps, base first:
        //   0F0F19 · 151515 · 1B1B18 · 2A2A2A · 414141 · 504E4E · 555555 · 888888
        // Deeper-set things step down it, raised things step up. The two
        // off-neutral rungs are deliberate: dead grey all the way up reads
        // as a rendering error rather than a choice.
        title: srgb(0x0f0f19),
        toolbar: srgb(0x1b1b18),
        panel: srgb(0x151515),
        timeline: srgb(0x151515),
        // Darker than the panels above it, so the strip reads as the floor
        // rather than as a band stuck to the bottom of the timeline.
        //
        // A new value, and deliberately: the ladder's only rung below
        // 151515 is 0F0F19, which is the one that leans blue on purpose,
        // and a blue cast is wrong for a strip that runs the full width of
        // the window. This is that rung with the blue taken out.
        status: srgb(0x0f0f0f),
        void: srgb(0x0f0f19),
        // The stage surround stays Spark's deep purple — it is the one
        // large area that is deliberately not part of the grey ladder.
        gutter: srgb(0x160d29),

        card: srgb(0x2a2a2a),
        // One rung *down* the ladder from the card it sits in, so a
        // settings block reads as recessed into its card rather than
        // stacked on top of it.
        card_inner: srgb(0x1b1b18),
        // A rung *below* the block it sits on, so an effect reads as sunk
        // into the settings rather than raised off them. It used to be the
        // layer card's 2a2a2a, the lightest surface in the panel, which
        // made a list of effects the loudest thing on the card.
        fx_card: srgb(0x151515),
        // Both start at exactly what they were borrowing, so splitting them
        // out changed no pixels — only what a restyle can reach.
        button: srgb(0x2a2a2a),
        popup: srgb(0x2a2a2a),
        card_border: srgb(0x555555),
        header: srgb(0x1b1b18),
        plate_edge: srgb(0x0f0f19),
        well: srgb(0x151515),
        well_deep: srgb(0x0f0f19),
        checker: [srgb(0x1b1b18), srgb(0x2a2a2a)],
        button_hover: srgb(0x414141),
        segment_on: srgb(0x504e4e),
        close_hover: srgb(0xc42b1c),

        // Text and icons keep their own contrast: the ladder tops out at
        // 888888, which is fine for a dimmed label but nowhere near enough
        // for one that has to be read across a room.
        text: srgb(0xf2f2f2),
        text_dim: srgb(0xb2b2b2),
        text_off: srgb(0x888888),

        icon: srgb(0xa2a2a2),
        icon_hover: srgb(0xf2f2f2),

        accent: srgb(0xffc800),
        accent_alt: srgb(0xc94df0),
        accent_alt_bg: srgb(0x2b1a35),
        accent_bg: srgb(0x6b4e00),
        // One gold: the seams match the accent — two golds side by side
        // read as a mistake (Alva spotted the darker panel borders).
        seam: srgb(0xffc800),
        wave: srgb(0x2bbfae),
        playhead: srgb(0xffc800),
        red: srgb(0xf04545),
        play: srgb(0x3fdc74),
        play_bg: srgb(0x1a4a2c),
        play_rest: srgb(0x414141),
        play_hover: srgb(0x555555),

        slider_track: srgb(0x414141),
        slider_thumb: srgb(0xededed),
        slider_fill: [srgb(0x5b21b6), srgb(0xffc800)],
        knob_cap_hi: srgb(0x323232),
        knob_cap_lo: srgb(0x151515),
        knob_track: srgb(0x484848),
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

    /// Alpha is coverage, not light. Running it through the sRGB curve
    /// would make `…80` composite at 21% instead of the 50% it says, and
    /// every transparent surface would read far more solid than its code.
    #[test]
    fn alpha_is_not_gamma_decoded() {
        let a = srgba(0x00000080)[3];
        assert!((a - 0.5019).abs() < 0.001, "0x80 alpha -> {a}");
        assert_eq!(srgba(0x123456ff)[3], 1.0, "ff is opaque");
        assert_eq!(srgba(0x12345600)[3], 0.0, "00 is invisible");
        // The colour half still is, so the two constructors agree on RGB.
        assert_eq!(srgba(0x808080ff)[..3], srgb(0x808080)[..3]);
    }

    /// Six digits still means opaque, and only a transparent colour grows
    /// the extra two — so every code that ever worked reads the same, and a
    /// see-through one says so on its face.
    #[test]
    fn only_a_transparent_colour_prints_eight_digits() {
        assert_eq!(hex_of(srgb(0x1B1B18)), "1B1B18");
        assert_eq!(hex_of(srgba(0x1B1B1880)), "1B1B1880");
    }

    #[test]
    fn transparency_round_trips_through_the_code() {
        for hex in [0x00000000u32, 0x1B1B1880, 0xFFC80040, 0x504E4EFE] {
            let c = srgba(hex);
            assert_eq!(from_hex(&hex_of(c)), Some(c), "{hex:08X}");
        }
    }

    /// Shorthand doubles each digit, alpha included.
    #[test]
    fn four_digit_shorthand_carries_alpha() {
        assert_eq!(from_hex("f0a8"), Some(srgba(0xff00aa88)));
        assert_eq!(from_hex("#f0a8"), Some(srgba(0xff00aa88)));
        // Lengths that aren't a colour stay unparsed rather than guessing:
        // a half-typed code must not apply.
        for s in ["", "1", "12", "12345", "1234567", "123456789", "ZZZZZZ"] {
            assert_eq!(from_hex(s), None, "{s:?} parsed");
        }
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
    ///
    /// The seam used to be its own dimmer gold, deliberately — until the
    /// glow-up put the two golds side by side and Alva spotted the darker
    /// panel borders immediately (2026-08-31). One gold now.
    #[test]
    fn the_accent_has_exactly_one_name() {
        let t = default_theme();
        assert_eq!(t.accent, t.playhead, "the playhead wears the accent");
        assert_eq!(t.accent, t.slider_fill[1], "so does the slider's far end");
        assert_eq!(t.accent, t.seam, "the seams wear the same gold");
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
        // The card's face leads with Lantern Mix's lift, so the fill is
        // the *lightened* theme colour — following the theme is the point.
        assert_eq!(
            surfaces().card.fill,
            crate::surface::lighten([1.0, 0.0, 0.0, 1.0], 0.06),
            "fill followed"
        );
        assert_eq!(
            surfaces().card.border_color,
            [0.0, 1.0, 0.0, 1.0],
            "and so did the border"
        );
        set_theme(default_theme());
        assert_eq!(
            surfaces().card.fill,
            crate::surface::lighten(default_theme().card, 0.06),
            "restored"
        );
    }
}
