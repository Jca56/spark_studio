//! The color home: the always-visible section at the top of the right
//! panel — color has exactly one home. It paints the selection (or the
//! gradient endpoint armed on its layer card); with nothing selected it
//! sets the draw color for the next shape. Pure layout + hit testing.

use spark_render::Viewport;
use spark_ui::picker::hsv_to_rgb;
use spark_ui::{ColorPicker, Layout, Swatches};

use crate::editor::PALETTE;
use crate::{Studio, layers};

impl Studio {
    /// The right panel's regions and laid-out layer cards.
    pub(crate) fn right_panel(&self, layout: &Layout) -> (Viewport, Viewport, layers::Cards) {
        let scale = self.scale();
        let (color_vp, cards_vp) = split(
            layout.right,
            scale,
            self.picker_hsv.is_some(),
            self.chrome_target().is_some(),
        );
        let cards = layers::rows(
            cards_vp,
            scale,
            &self.editor,
            self.card_open,
            self.card_tab,
            self.layers_scroll,
        );
        (color_vp, cards_vp, cards)
    }

    /// The box of the scrub field currently being typed into — on a layer
    /// card or a folder header, which behave identically.
    pub(crate) fn field_box(&self) -> Option<spark_render::Viewport> {
        let (target, prop, _) = self.field_edit.as_ref()?;
        let key = match *target {
            crate::ScrubTarget::Shape => {
                crate::layers::EditField::Shape(self.editor.primary()?, *prop)
            }
            crate::ScrubTarget::Folder(id) => crate::layers::EditField::Folder(id, *prop),
        };
        let layout = self.layout()?;
        let (_, _, cards) = self.right_panel(&layout);
        cards.focused_field(key).map(|f| f.rect)
    }

    /// The color home always shows the *current color* — never the
    /// selection's. Selecting a shape doesn't move it; the eyedropper does.
    /// That way the color you lined up survives clicking around the stack.
    pub(crate) fn color_home(&self, region: Viewport) -> ColorHome {
        build_for(
            region,
            self.scale(),
            self.picker_hsv,
            self.chrome_target().map(|t| (t, self.material_pick)),
            (self.editor.color(), self.editor.palette_match()),
        )
    }

    /// The playground colour the picker is painting, if any.
    ///
    /// Gated on the playground being open, so closing it hands the picker
    /// straight back to the canvas without anything else having to
    /// remember to let go.
    pub(crate) fn chrome_target(&self) -> Option<crate::materials::Edit> {
        self.materials_open
            .then_some(self.material_target)
            .flatten()
    }
}

/// Split the right panel: the color section on top (taller while the
/// picker is open), layer cards below.
pub fn split(right: Viewport, scale: f32, picker_open: bool, chrome: bool) -> (Viewport, Viewport) {
    // Painting the editor's own chrome adds two rows: a caption naming what
    // the picker has hold of, and an alpha slider — a chrome colour has
    // transparency and a shape colour does not, so the control appears only
    // where it tells the truth.
    // The same allowance either way: the picker can be closed while a
    // chrome colour is still held, and the alpha track has to stay inside
    // the section in that case too.
    let extra = if chrome { 60.0 } else { 0.0 };
    let h = (if picker_open { 346.0 } else { 110.0 } + extra) * scale;
    let h = h.min(right.h);
    (
        Viewport {
            x: right.x,
            y: right.y,
            w: right.w,
            h,
        },
        Viewport {
            x: right.x,
            y: right.y + h,
            w: right.w,
            h: (right.h - h).max(1.0),
        },
    )
}

pub struct ColorHome {
    pub region: Viewport,
    pub swatches: Swatches,
    /// The colours the chips stand for: the neon palette while a shape is
    /// being painted, Alva's grey ladder while the chrome is.
    pub chips: Vec<[f32; 3]>,
    /// What the picker currently has hold of, when that is not the obvious
    /// answer. `None` means the selection, the way it always has.
    pub caption: Option<String>,
    /// Alpha track and its value, only where alpha means anything.
    pub alpha: Option<(Viewport, f32)>,
    /// Palette entry to ring as selected, if the active color matches one.
    pub palette: Option<usize>,
    /// The current-color bar; clicking it opens/closes the picker.
    pub custom: Viewport,
    /// The active color (linear) the bar previews.
    pub custom_rgb: [f32; 3],
    /// Open picker: geometry plus its H/S/V and hex readout position.
    pub picker: Option<(ColorPicker, [f32; 3], [f32; 2])>,
}

/// The colour home aimed at whatever the picker currently has hold of: a
/// playground colour, or the shape selection.
///
/// A free function because `Studio::render` already holds `&mut` borrows of
/// its own gpu and text fields and so cannot call a `&self` method — the
/// choice has to be expressible from field access alone.
pub fn build_for(
    region: Viewport,
    scale: f32,
    picker_hsv: Option<[f32; 3]>,
    chrome: Option<(crate::materials::Edit, usize)>,
    shape: ([f32; 3], Option<usize>),
) -> ColorHome {
    match chrome {
        Some((t, pick)) => {
            let c = crate::materials::color_of(t, pick);
            build(
                region,
                scale,
                [c[0], c[1], c[2]],
                None,
                picker_hsv,
                Some((
                    crate::materials::label_of(t, pick),
                    c[3],
                    spark_ui::ladder(),
                )),
            )
        }
        None => build(region, scale, shape.0, shape.1, picker_hsv, None),
    }
}

pub fn build(
    region: Viewport,
    scale: f32,
    active_rgb: [f32; 3],
    palette: Option<usize>,
    picker_hsv: Option<[f32; 3]>,
    chrome: Option<(String, f32, Vec<[f32; 3]>)>,
) -> ColorHome {
    let pad = 14.0 * scale;
    let content_w = (region.w - pad * 2.0).max(1.0);
    let mut y = region.y + 12.0 * scale;
    let caption = chrome.as_ref().map(|(name, ..)| name.clone());
    if caption.is_some() {
        y += 30.0 * scale;
    }
    let chips: Vec<[f32; 3]> = match &chrome {
        Some((_, _, c)) => c.clone(),
        None => PALETTE.to_vec(),
    };
    let n = chips.len();
    let side = 40.0 * scale;
    let gap = ((content_w - side * n as f32) / (n - 1) as f32).max(6.0 * scale);
    let swatches = Swatches::new(region.x + pad, y, side, gap, n);
    y += side + 12.0 * scale;
    let custom = Viewport {
        x: region.x + pad,
        y,
        w: content_w,
        h: 28.0 * scale,
    };
    y += 40.0 * scale;
    let picker = picker_hsv.map(|hsv| {
        let p = ColorPicker::new(region.x + pad, y, content_w, 190.0 * scale, scale);
        y += 200.0 * scale;
        (p, hsv, [region.x + pad, y])
    });
    // Below whatever came last — the picker when it is open, the current
    // colour bar when it is not.
    let alpha = chrome.as_ref().map(|&(_, a, _)| {
        (
            Viewport {
                x: region.x + pad,
                y: y + 10.0 * scale,
                w: content_w,
                h: 14.0 * scale,
            },
            a,
        )
    });
    ColorHome {
        region,
        swatches,
        chips,
        caption,
        alpha,
        palette,
        custom,
        custom_rgb: active_rgb,
        picker,
    }
}

pub enum ColorHit {
    Swatch(usize),
    /// The current-color bar: open/close the picker.
    Custom,
    /// A click in the HSV square: (saturation, value).
    Sv(f32, f32),
    /// A click on the hue bar.
    Hue(f32),
    /// A click on the alpha track: how opaque, 0..1.
    Alpha(f32),
}

impl ColorHome {
    pub fn hit(&self, px: f32, py: f32) -> Option<ColorHit> {
        if !self.region.contains(px, py) {
            return None;
        }
        if let Some(i) = self.swatches.hit(px, py) {
            return Some(ColorHit::Swatch(i));
        }
        if self.custom.contains(px, py) {
            return Some(ColorHit::Custom);
        }
        if let Some((p, _, _)) = &self.picker {
            if let Some((s, v)) = p.hit_sv(px, py) {
                return Some(ColorHit::Sv(s, v));
            }
            if let Some(h) = p.hit_hue(px, py) {
                return Some(ColorHit::Hue(h));
            }
        }
        if let Some((track, _)) = &self.alpha
            && alpha_grab(*track).contains(px, py)
        {
            return Some(ColorHit::Alpha(spark_ui::Slider::t_at(*track, px)));
        }
        None
    }
}

/// An alpha track is thin; its grab box is fattened so it can be hit
/// without aiming, the same way the playground's knobs are.
pub fn alpha_grab(track: Viewport) -> Viewport {
    Viewport {
        y: track.y - track.h,
        h: track.h * 3.0,
        ..track
    }
}

/// sRGB hex for the picker readout.
pub fn hex_of(hsv: [f32; 3]) -> String {
    let rgb = hsv_to_rgb(hsv[0], hsv[1], hsv[2]);
    format!(
        "#{:02X}{:02X}{:02X}",
        (rgb[0] * 255.0).round() as u8,
        (rgb[1] * 255.0).round() as u8,
        (rgb[2] * 255.0).round() as u8
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::materials::Edit;

    fn right() -> Viewport {
        Viewport {
            x: 3000.0,
            y: 60.0,
            w: 420.0,
            h: 1800.0,
        }
    }

    fn chrome(a: f32) -> Option<(String, f32, Vec<[f32; 3]>)> {
        Some(("Side panels".into(), a, spark_ui::ladder()))
    }

    /// Every control the colour home offers has to be inside the section
    /// the layout gave it — otherwise it draws over the layer cards, or off
    /// the bottom of the panel where nobody can reach it.
    ///
    /// The case that caught this: the picker can be *closed* while a chrome
    /// colour is still held, which left the alpha track hanging 36px below
    /// the section it belongs to.
    #[test]
    fn every_control_stays_inside_the_section() {
        for scale in [1.0f32, 1.4] {
            for open in [false, true] {
                let hsv = open.then_some([0.5, 0.5, 0.5]);
                let (vp, cards) = split(right(), scale, open, true);
                let h = build(vp, scale, [0.3; 3], None, hsv, chrome(0.5));
                let (track, _) = h.alpha.expect("chrome colours have an alpha track");
                assert!(
                    track.y + track.h <= vp.y + vp.h,
                    "scale {scale}, picker {open}: alpha escapes by {}",
                    track.y + track.h - (vp.y + vp.h)
                );
                assert!(track.x >= vp.x && track.x + track.w <= vp.x + vp.w);
                // And the section must not eat the cards below it.
                assert!(cards.h > 0.0 && cards.y >= vp.y + vp.h - 0.5);
            }
        }
    }

    /// Alpha belongs only where alpha means something. A shape's colour is
    /// `[r, g, b, intensity]` with no alpha channel at all, so offering the
    /// control there would be a lie about what Spark can do.
    #[test]
    fn a_shape_colour_gets_no_alpha_track() {
        let (vp, _) = split(right(), 1.0, true, false);
        let h = build(vp, 1.0, [0.3; 3], Some(2), Some([0.1, 0.2, 0.3]), None);
        assert!(h.alpha.is_none(), "shapes cannot fade yet");
        assert!(h.caption.is_none(), "and the picker means the selection");
        assert_eq!(h.chips.len(), crate::editor::PALETTE.len(), "neon chips");
    }

    /// Painting the chrome swaps the chips for the ladder the editor is
    /// actually built from, and says out loud what it has hold of.
    #[test]
    fn painting_the_chrome_offers_the_ladder() {
        let (vp, _) = split(right(), 1.0, true, true);
        let h = build(vp, 1.0, [0.3; 3], None, Some([0.1, 0.2, 0.3]), chrome(1.0));
        assert_eq!(h.chips.len(), spark_ui::LADDER.len());
        assert_eq!(h.caption.as_deref(), Some("Side panels"));
        for (i, chip) in h.chips.iter().enumerate() {
            let want = spark_ui::srgb(spark_ui::LADDER[i]);
            assert_eq!(*chip, [want[0], want[1], want[2]], "rung {i}");
        }
    }

    /// The alpha track is thin, so its grab box is fattened — a control you
    /// have to aim at is a control that gets missed.
    #[test]
    fn the_alpha_track_is_reachable() {
        let (vp, _) = split(right(), 1.0, true, true);
        let h = build(vp, 1.0, [0.3; 3], None, Some([0.1, 0.2, 0.3]), chrome(0.5));
        let (track, _) = h.alpha.unwrap();
        let mid = (track.x + track.w * 0.5, track.y + track.h * 0.5);
        assert!(matches!(h.hit(mid.0, mid.1), Some(ColorHit::Alpha(_))));
        // A few px above the hairline still counts.
        assert!(matches!(
            h.hit(mid.0, track.y - track.h * 0.5),
            Some(ColorHit::Alpha(_))
        ));
    }

    /// A playground colour is read and written through one pair of
    /// functions, so the picker, the swatches and a typed code can never
    /// disagree about where a colour lives.
    #[test]
    fn a_playground_colour_round_trips() {
        let _skin = crate::materials::tests::own_the_skin();
        let start = spark_ui::surfaces();
        let was = spark_ui::theme();
        let want = spark_ui::srgba(0x336699C0);

        crate::materials::set_color(Edit::Slot(0), 0, want);
        assert_eq!(crate::materials::color_of(Edit::Slot(0), 0), want);

        crate::materials::set_color(Edit::GradEnd, 4, want);
        assert_eq!(crate::materials::color_of(Edit::GradEnd, 4), want);

        spark_ui::set_theme(was);
        spark_ui::set_surfaces(start);
    }
}
