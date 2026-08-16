//! Inspector panel geometry: labeled slider rows, color swatches, and the
//! fill/outline toggle for the selected shape. Pure layout + value mapping —
//! rendering and drag state live in main.

use spark_render::{CANVAS_H, CANVAS_W, Viewport};
use spark_ui::picker::hsv_to_rgb;
use spark_ui::{ColorPicker, Segmented, Swatches};

use crate::editor::{PALETTE, Prop, Props};

pub struct Row {
    pub prop: Prop,
    pub label: &'static str,
    /// Top-left of the label text (physical px).
    pub label_pos: [f32; 2],
    pub track: Viewport,
    /// Normalized slider position, 0..1.
    pub t: f32,
    pub value: String,
}

/// The whole inspector, laid out for the current selection.
pub struct Inspector {
    /// The hosting panel region — hits outside it never count, so content
    /// scrolled out of view can't swallow clicks.
    pub panel: Viewport,
    /// The card behind everything — the settings read as a panel on the
    /// panel, and clicks inside it never fall through to deselect.
    pub card: Viewport,
    pub rows: Vec<Row>,
    pub color_label_pos: [f32; 2],
    pub swatches: Swatches,
    /// Palette entry to ring as selected, if the shape's color matches one.
    pub palette: Option<usize>,
    /// The current-color bar that opens the picker.
    pub custom: Viewport,
    /// The shape's color (linear) for the custom bar's fill.
    pub custom_rgb: [f32; 3],
    /// Open picker: geometry plus its H/S/V and hex readout position.
    pub picker: Option<(ColorPicker, [f32; 3], [f32; 2])>,
    /// Fill/outline toggle — absent for lines.
    pub mode: Option<ToggleRow>,
    /// Solid/Add compositing toggle — every shape has one.
    pub blend: ToggleRow,
}

/// A labeled two-way segmented toggle row.
pub struct ToggleRow {
    pub seg: Segmented,
    /// Whether the second segment is the active one.
    pub on: bool,
    pub label_pos: [f32; 2],
}

pub enum Hit {
    Slider(Prop, f32),
    Swatch(usize),
    /// The custom-color bar: open/close the picker.
    PickerToggle,
    /// A click in the HSV square: (saturation, value).
    PickerSv(f32, f32),
    /// A click on the hue bar.
    PickerHue(f32),
    Outline(bool),
    Blend(bool),
}

fn range(prop: Prop) -> (f32, f32) {
    match prop {
        Prop::X => (0.0, CANVAS_W),
        Prop::Y => (0.0, CANVAS_H),
        Prop::Rotation => (-std::f32::consts::PI, std::f32::consts::PI),
        Prop::Scale => (3.0, 900.0),
        Prop::Width => (6.0, CANVAS_W),
        Prop::Height => (6.0, CANVAS_H),
        Prop::Glow => (2.0, 300.0),
        Prop::Brightness => (0.05, 5.0),
        Prop::Sides => (3.0, 12.0),
        Prop::Thickness => (1.0, 30.0),
    }
}

/// Map a normalized slider position back to a property value.
pub fn value_for(prop: Prop, t: f32) -> f32 {
    let (min, max) = range(prop);
    min + t.clamp(0.0, 1.0) * (max - min)
}

/// `scroll` shifts all content up by that many physical px; the caller
/// scissors the panel so overflow clips instead of spilling. `picker_hsv`
/// is `Some` while the color picker is open.
pub fn build(
    panel: Viewport,
    scale: f32,
    props: &Props,
    scroll: f32,
    picker_hsv: Option<[f32; 3]>,
) -> Inspector {
    let pad = 16.0 * scale;
    let row_h = 76.0 * scale;
    let content_w = (panel.w - pad * 2.0).max(1.0);
    let mut y = panel.y + pad - scroll;

    let color_label_pos = [panel.x + pad, y];
    let n = PALETTE.len();
    let side = 44.0 * scale;
    let gap = ((content_w - side * n as f32) / (n - 1) as f32).max(8.0 * scale);
    let swatches = Swatches::new(panel.x + pad, y + 38.0 * scale, side, gap, n);
    y += (38.0 + 44.0 + 14.0) * scale;

    // The current-color bar; clicking it opens the picker below.
    let custom = Viewport {
        x: panel.x + pad,
        y,
        w: content_w,
        h: 30.0 * scale,
    };
    y += 42.0 * scale;
    let picker = picker_hsv.map(|hsv| {
        let p = ColorPicker::new(panel.x + pad, y, content_w, 190.0 * scale, scale);
        y += 200.0 * scale;
        let hex_pos = [panel.x + pad, y];
        y += 36.0 * scale;
        (p, hsv, hex_pos)
    });

    let mut rows = Vec::new();
    let mut push = |prop: Prop, label: &'static str, v: f32, value: String| {
        let (min, max) = range(prop);
        rows.push(Row {
            prop,
            label,
            label_pos: [panel.x + pad, y],
            track: Viewport {
                x: panel.x + pad,
                y: y + 42.0 * scale,
                w: content_w,
                h: 11.0 * scale,
            },
            t: ((v - min) / (max - min)).clamp(0.0, 1.0),
            value,
        });
        y += row_h;
    };
    push(Prop::X, "X", props.x, format!("{:.0}", props.x));
    push(Prop::Y, "Y", props.y, format!("{:.0}", props.y));
    push(
        Prop::Rotation,
        "Rotation",
        props.rotation,
        format!("{:.0}\u{b0}", props.rotation.to_degrees()),
    );
    push(
        Prop::Scale,
        "Scale",
        props.size,
        format!("{:.0}", props.size),
    );
    if let Some([w, h]) = props.box_size {
        push(Prop::Width, "Width", w, format!("{w:.0}"));
        push(Prop::Height, "Height", h, format!("{h:.0}"));
    }
    push(Prop::Glow, "Glow", props.glow, format!("{:.0}", props.glow));
    push(
        Prop::Brightness,
        "Brightness",
        props.brightness,
        format!("{:.1}", props.brightness),
    );
    if let Some(sides) = props.sides {
        push(Prop::Sides, "Sides", sides as f32, format!("{sides}"));
    }
    if let Some(th) = props.thickness {
        push(Prop::Thickness, "Thickness", th, format!("{th:.1}"));
    }

    let toggle_row = |y: f32, on: bool| ToggleRow {
        label_pos: [panel.x + pad, y],
        seg: Segmented::new(
            Viewport {
                x: panel.x + pad,
                y: y + 38.0 * scale,
                w: content_w,
                h: 52.0 * scale,
            },
            2,
            scale,
        ),
        on,
    };
    let mode = props.outline.map(|outline| {
        let t = toggle_row(y, outline);
        y += 104.0 * scale;
        t
    });
    let blend = toggle_row(y, props.additive);
    y += 104.0 * scale;

    let inset = 8.0 * scale;
    Inspector {
        panel,
        card: Viewport {
            x: panel.x + inset,
            y: panel.y + inset - scroll,
            w: (panel.w - inset * 2.0).max(1.0),
            h: (y - (panel.y - scroll)).max(1.0),
        },
        rows,
        color_label_pos,
        swatches,
        palette: props.palette,
        custom,
        custom_rgb: props.rgb,
        picker,
        mode,
        blend,
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

impl Inspector {
    /// Hit test everything: slider tracks (generous vertical grab zone),
    /// swatches, and the fill/outline toggle. Clicks outside the hosting
    /// panel never count, whatever the scroll position.
    pub fn hit(&self, px: f32, py: f32) -> Option<Hit> {
        if !self.panel.contains(px, py) {
            return None;
        }
        if let Some(row) = self.rows.iter().find(|r| {
            px >= r.track.x
                && px <= r.track.x + r.track.w
                && (py - (r.track.y + r.track.h * 0.5)).abs() <= r.track.h * 2.0
        }) {
            let t = ((px - row.track.x) / row.track.w).clamp(0.0, 1.0);
            return Some(Hit::Slider(row.prop, t));
        }
        if let Some(i) = self.swatches.hit(px, py) {
            return Some(Hit::Swatch(i));
        }
        if self.custom.contains(px, py) {
            return Some(Hit::PickerToggle);
        }
        if let Some((p, _, _)) = &self.picker {
            if let Some((s, v)) = p.hit_sv(px, py) {
                return Some(Hit::PickerSv(s, v));
            }
            if let Some(h) = p.hit_hue(px, py) {
                return Some(Hit::PickerHue(h));
            }
        }
        if let Some(mode) = &self.mode
            && let Some(i) = mode.seg.hit(px, py)
        {
            return Some(Hit::Outline(i == 1));
        }
        if let Some(i) = self.blend.seg.hit(px, py) {
            return Some(Hit::Blend(i == 1));
        }
        None
    }
}
