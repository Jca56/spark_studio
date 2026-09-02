//! Everything the timeline paints: the transport toolbar's buttons, the
//! sidebar surfaces, the waveform, bar shading, ruler ticks, the loop brace
//! and the playhead. Geometry comes from `super`; text is chrome's job.

use spark_render::Viewport;
use spark_ui::{ICON_KEY, ICON_PAUSE, ICON_PLAY, UiRect, surfaces, theme};

use super::{Controls, Panel, TimeView};

/// Toolbar controls: the snap toggle, the tempo field, a play button you
/// can actually see, and the canvas zoom cluster at the right end.
#[allow(clippy::too_many_arguments)]
pub fn toolbar_rects(
    c: &Controls,
    scale: f32,
    playing: bool,
    hover_play: bool,
    snap: bool,
    wave: bool,
    editing_bpm: bool,
    zoom_hover: Option<u8>,
) -> Vec<UiRect> {
    let t = theme();
    let mut out = Vec::new();
    // Every toolbar square is the same grey plate on a dark border — the
    // *glyph* carries the state, so the row reads as one set of controls.
    let plate = |out: &mut Vec<UiRect>, b: Viewport| out.push(surfaces().plate.rect(b, scale));
    // Snap: two grid lines with a marker locked between them.
    let sb = c.snap;
    plate(&mut out, sb);
    let scol = if snap { t.red } else { t.icon };
    let gh = sb.h * 0.5;
    let gy = sb.y + (sb.h - gh) * 0.5;
    let lw = 2.0 * scale;
    for off in [-0.19f32, 0.19] {
        out.push(UiRect::region(
            Viewport {
                x: sb.x + sb.w * (0.5 + off) - lw * 0.5,
                y: gy,
                w: lw,
                h: gh,
            },
            [scol[0], scol[1], scol[2], 0.75],
        ));
    }
    let m = sb.w * 0.20;
    out.push(UiRect::region_rounded(
        Viewport {
            x: sb.x + (sb.w - m) * 0.5,
            y: sb.y + (sb.h - m) * 0.5,
            w: m,
            h: m,
        },
        scol,
        2.5 * scale,
    ));
    // The waveform overlay: five bars of a waveform, teal while it is
    // laid across the grid.
    let wb = c.wave;
    plate(&mut out, wb);
    let wcol = if wave { t.wave } else { t.icon };
    let bar_w = wb.w * 0.09;
    let gap = wb.w * 0.05;
    let heights = [0.30f32, 0.62, 0.92, 0.48, 0.74];
    let total = heights.len() as f32 * bar_w + (heights.len() as f32 - 1.0) * gap;
    for (k, hgt) in heights.into_iter().enumerate() {
        let hh = wb.h * 0.5 * hgt;
        out.push(UiRect::region_rounded(
            Viewport {
                x: wb.x + (wb.w - total) * 0.5 + k as f32 * (bar_w + gap),
                y: wb.y + wb.h * 0.5 - hh,
                w: bar_w,
                h: hh * 2.0,
            },
            [wcol[0], wcol[1], wcol[2], if wave { 1.0 } else { 0.75 }],
            bar_w * 0.5,
        ));
    }
    // The tempo field: a sunken well, gold-ringed while it's being typed
    // into — the same language every other editable number in the app uses.
    let well = surfaces().well.at_radius(8.0);
    out.push(if editing_bpm {
        well.ringed(c.bpm, scale, 2.5, t.accent)
    } else {
        well.rect(c.bpm, scale)
    });
    // Play is a plate like its neighbours — same raised material, its
    // green glyph (and a dark green face while playing) carrying state.
    let play_plate = surfaces().plate.at_radius(14.0);
    out.push(if playing {
        play_plate.filled(t.play_bg).rect(c.play, scale)
    } else if hover_play {
        play_plate.filled(t.play_hover).rect(c.play, scale)
    } else {
        play_plate.rect(c.play, scale)
    });
    out.push(UiRect::icon_sized(
        c.play,
        if playing { ICON_PAUSE } else { ICON_PLAY },
        2.5 * scale,
        t.play,
        0.34,
    ));
    // The zoom cluster at the right end: - / + steppers and the readout
    // button — plates like the toolbar's other buttons, hover lightening
    // the face. Glyphs are plain bars; the percentage is text, chrome's job.
    for (i, r) in [c.zoom_minus, c.zoom_plus, c.zoom_pct].into_iter().enumerate() {
        let plate = surfaces().plate.at_radius(10.0);
        out.push(if zoom_hover == Some(i as u8) {
            plate.filled(t.button_hover).rect(r, scale)
        } else {
            plate.rect(r, scale)
        });
    }
    let len = c.zoom_minus.w * 0.44;
    let thick = 3.5 * scale;
    let hbar = |r: Viewport| Viewport {
        x: r.x + (r.w - len) * 0.5,
        y: r.y + (r.h - thick) * 0.5,
        w: len,
        h: thick,
    };
    out.push(UiRect::region_rounded(
        hbar(c.zoom_minus),
        t.icon_hover,
        thick * 0.5,
    ));
    out.push(UiRect::region_rounded(
        hbar(c.zoom_plus),
        t.icon_hover,
        thick * 0.5,
    ));
    out.push(UiRect::region_rounded(
        Viewport {
            x: c.zoom_plus.x + (c.zoom_plus.w - thick) * 0.5,
            y: c.zoom_plus.y + (c.zoom_plus.h - len) * 0.5,
            w: thick,
            h: len,
        },
        t.icon_hover,
        thick * 0.5,
    ));
    out
}

/// The sidebar: lighter background, the tools bay on the left, the inset
/// track-name box on the right. `stamp` is the hero Keyframe button.
pub fn sidebar_rects(panel: &Panel, scale: f32, hover_stamp: bool) -> Vec<UiRect> {
    let t = theme();
    let mut out = vec![
        // The gutter is ground like the axis: its own tint, fading down.
        surfaces().timeline.filled(t.toolbar).rect(panel.gutter, scale),
        surfaces().well.at_radius(10.0).rect(panel.tools, scale),
        surfaces()
            .well
            .filled(t.well_deep)
            .at_radius(10.0)
            .rect(panel.names_box, scale),
    ];
    let b = panel.stamp;
    out.push(
        surfaces()
            .plate
            .filled(if hover_stamp { t.button_hover } else { t.card })
            .rect(b, scale),
    );
    // Square button, no label — the glyph centres in it.
    out.push(UiRect::icon_sized(b, ICON_KEY, 0.0, t.accent, 0.42));
    out
}

/// A file's waveform: one min/max teal column per ~2 logical px across
/// `x_range` of the axis, aggregated from the file's `peaks` and mapped
/// through `time_at` — screen x to *file* time, `None` where the file
/// isn't playing. `band` is the vertical slice it fills — a clip's bar
/// on the arrangement, or the whole grid for the overlay.
#[allow(clippy::too_many_arguments)]
pub fn wave_rects(
    panel: &Panel,
    band: (f32, f32),
    scale: f32,
    peaks: &[[f32; 2]],
    file_len: f32,
    alpha: f32,
    x_range: (f32, f32),
    time_at: &dyn Fn(f32) -> Option<f32>,
) -> Vec<UiRect> {
    if peaks.is_empty() {
        return Vec::new();
    }
    let mut teal = theme().wave;
    teal[3] *= alpha;
    let (y0, y1) = band;
    let mid = (y0 + y1) * 0.5;
    let half_h = ((y1 - y0) * 0.5 - 3.0 * scale).max(1.0);
    let (ax, aw) = panel.axis;
    let (x0, x1) = (x_range.0.max(ax), x_range.1.min(ax + aw));
    if x1 <= x0 {
        return Vec::new();
    }
    let bucket_s = spark_audio::PEAK_BUCKET as f32 / spark_audio::SAMPLE_RATE as f32;
    let step = 2.0 * scale;
    // Columns on the axis's own grid, so a bar's waveform lines up with
    // the overlay's.
    let first = ((x0 - ax) / step).floor().max(0.0) as usize;
    let last = ((x1 - ax) / step).ceil() as usize;
    let mut out = Vec::with_capacity(last.saturating_sub(first));
    for col in first..last {
        let x = ax + col as f32 * step;
        if x < x0 || x + step > x1 + step * 0.5 {
            continue;
        }
        let Some(ta) = time_at(x) else {
            continue;
        };
        if ta >= file_len || ta < 0.0 {
            continue;
        }
        let tb = time_at(x + step).unwrap_or(ta).max(ta);
        let a = ((ta / bucket_s) as usize).min(peaks.len() - 1);
        let b = ((tb / bucket_s).ceil() as usize).clamp(a + 1, peaks.len());
        let mut lo = 0.0f32;
        let mut hi = 0.0f32;
        for p in &peaks[a..b] {
            lo = lo.min(p[0]);
            hi = hi.max(p[1]);
        }
        out.push(UiRect::region(
            Viewport {
                x,
                y: mid - hi * half_h,
                w: (step * 0.75).max(1.0),
                h: ((hi - lo) * half_h).max(1.0 * scale),
            },
            teal,
        ));
    }
    out
}

/// Alternating bar shading across the axis (odd bars a touch lighter),
/// quarter-note lines inside each bar once there's room for them, and an
/// unmissable seam at every phrase (4 bars).
pub fn shade_rects(
    panel: &Panel,
    view: &TimeView,
    scale: f32,
    beat: &spark_audio::BeatGrid,
    duration: f32,
    grid: super::Grid,
) -> Vec<UiRect> {
    let (y0, y1) = panel.axis_y;
    let (ax, aw) = panel.axis;
    let h = (y1 - y0).max(1.0);
    let bar_s = 4.0 * 60.0 / beat.bpm.max(1.0);
    // The lines inside a bar are the grid's own steps — what a drag
    // lands on — once they have room; beats (a shade brighter) keep
    // the bar readable under a fine grid.
    let steps = grid.per_bar() as i64;
    let step_s = bar_s / steps as f32;
    let px_per_step = step_s / view.span() * aw;
    let end = view.t1.min(duration);
    // Base wash lifts the whole axis off the panel black; odd bars go a
    // step lighter on top of it.
    let mut out = vec![UiRect::region(
        Viewport {
            x: ax,
            y: y0,
            w: aw,
            h,
        },
        [1.0, 1.0, 1.0, 0.010],
    )];
    let mut k = (((view.t0 - beat.first_bar) / bar_s).floor() as i64).max(0);
    loop {
        let t = beat.first_bar + k as f32 * bar_s;
        if t >= end {
            break;
        }
        let x0 = view.x_of(t, panel.axis).max(ax);
        let x1 = view.x_of((t + bar_s).min(end), panel.axis).min(ax + aw);
        if k % 2 == 1 && x1 > x0 {
            out.push(UiRect::region(
                Viewport {
                    x: x0,
                    y: y0,
                    w: x1 - x0,
                    h,
                },
                [1.0, 1.0, 1.0, 0.028],
            ));
        }
        if px_per_step >= 9.0 * scale {
            for q in 1..steps {
                let bt = t + step_s * q as f32;
                if bt >= view.t0 && bt <= end {
                    let on_beat = steps >= 4 && q % (steps / 4) == 0;
                    out.push(UiRect::region(
                        Viewport {
                            x: view.x_of(bt, panel.axis) - 0.5 * scale,
                            y: y0,
                            w: 1.0 * scale,
                            h,
                        },
                        [1.0, 1.0, 1.0, if on_beat { 0.07 } else { 0.035 }],
                    ));
                }
            }
        }
        if k % 4 == 0 && k > 0 {
            out.push(UiRect::region(
                Viewport {
                    x: view.x_of(t, panel.axis) - 1.0 * scale,
                    y: y0,
                    w: 2.0 * scale,
                    h,
                },
                [1.0, 1.0, 1.0, 0.28],
            ));
        }
        k += 1;
    }
    out
}

/// Ruler ticks: a hairline base, bar ticks, taller phrase ticks.
pub fn ruler_rects(
    panel: &Panel,
    view: &TimeView,
    scale: f32,
    beat: &spark_audio::BeatGrid,
    duration: f32,
) -> Vec<UiRect> {
    let r = panel.ruler;
    let mut out = vec![
        // The ruler is a recess cut into the ground, nearly black — the
        // bar numbers and the loop brace sit *in* it.
        surfaces()
            .well
            .filled(theme().well_deep)
            .at_radius(0.0)
            .edge(0.0, [0.0; 4])
            .rect(r, scale),
        UiRect::region(
            Viewport {
                x: r.x,
                y: r.y + r.h - 1.5 * scale,
                w: r.w,
                h: 1.5 * scale,
            },
            [1.0, 1.0, 1.0, 0.10],
        ),
    ];
    let bar_s = 4.0 * 60.0 / beat.bpm.max(1.0);
    let beat_s = bar_s * 0.25;
    // Beat ticks join the bar ticks once beats have room to breathe.
    let step_beats: i64 = if beat_s / view.span() * panel.axis.1 >= 24.0 * scale {
        1
    } else {
        4
    };
    let step_s = beat_s * step_beats as f32;
    let first = (((view.t0 - beat.first_bar) / step_s).ceil() as i64).max(0);
    let mut j = first;
    loop {
        let time = beat.first_bar + j as f32 * step_s;
        if time > view.t1 || time > duration {
            break;
        }
        let beats = j * step_beats;
        let (h, alpha) = if beats % 16 == 0 {
            (14.0, 0.55)
        } else if beats % 4 == 0 {
            (8.0, 0.30)
        } else {
            (4.5, 0.16)
        };
        out.push(UiRect::region(
            Viewport {
                x: view.x_of(time, panel.axis) - scale * 0.5,
                y: r.y + r.h - h * scale,
                w: 1.0 * scale,
                h: h * scale,
            },
            [1.0, 1.0, 1.0, alpha],
        ));
        j += 1;
    }
    out
}

/// The loop brace on the ruler: a gold band between the loop edges, dimmed
/// while the loop is toggled off.
pub fn loop_rects(
    panel: &Panel,
    view: &TimeView,
    scale: f32,
    region: (f32, f32),
    on: bool,
) -> Vec<UiRect> {
    let (a, b) = region;
    if b <= view.t0 || a >= view.t1 {
        return Vec::new();
    }
    let r = panel.ruler;
    let x0 = view.x_of(a.max(view.t0), panel.axis);
    let x1 = view.x_of(b.min(view.t1), panel.axis);
    let gold = theme().playhead;
    let alpha = if on { 0.35 } else { 0.14 };
    let mut out = vec![UiRect::region_rounded(
        Viewport {
            x: x0,
            y: r.y,
            w: (x1 - x0).max(2.0),
            h: 9.0 * scale,
        },
        [gold[0], gold[1], gold[2], alpha],
        3.0 * scale,
    )];
    for (t, x) in [(a, x0), (b, x1)] {
        if t >= view.t0 && t <= view.t1 {
            out.push(UiRect::region(
                Viewport {
                    x: x - 1.0 * scale,
                    y: r.y,
                    w: 2.0 * scale,
                    h: r.h,
                },
                [gold[0], gold[1], gold[2], if on { 0.8 } else { 0.35 }],
            ));
        }
    }
    out
}

/// The gold playhead line down the axis area; `None` while it's outside
/// the visible time window.
pub fn playhead_rect(panel: &Panel, view: &TimeView, scale: f32, time: f32) -> Option<UiRect> {
    // A clock quantised to audio frames lands a hair early; at the view's
    // left edge that hair must not hide the line.
    if time < view.t0 - 1.0e-3 || time > view.t1 {
        return None;
    }
    // At the axis's left edge (bar one, usually) the line sits just inside
    // it rather than half under the sidebar.
    let x = view.x_of(time, panel.axis).max(panel.axis.0 + 1.5 * scale);
    let y = panel.ruler.y;
    Some(UiRect::region(
        Viewport {
            x: x - 1.5 * scale,
            y,
            w: 3.0 * scale,
            h: (panel.axis_y.1 - y).max(1.0),
        },
        theme().playhead,
    ))
}
