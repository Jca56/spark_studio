//! Timeline panel chrome: the lane-name sidebar, a bars/beats ruler along
//! the top of the time axis, alternating bar shading, and the zoomable
//! time view every element (keys, waveform, playhead, scrub) maps through.
//! The transport toolbar above the panel holds the Wave button, the
//! Arrange/Keys toggle, the active tab's own tools, and play. Time starts
//! at the first bar — the pickup before it isn't part of the choreography.

use spark_render::Viewport;
use spark_ui::{ICON_KEY, ICON_PAUSE, ICON_PLAY, UiRect, srgb, theme};

/// The left sidebar is just the lane-name box now; tabs, the keyframe
/// stamp, and play all live in the transport toolbar above the panel.
const GUTTER: f32 = 200.0;
const RULER_H: f32 = 30.0;

/// Bottom-panel tabs. Wave is the resting default; Arrange and Keys share
/// the rectangular toggle button.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tab {
    Wave,
    Arrange,
    Keys,
}

impl Tab {
    pub fn name(self) -> &'static str {
        match self {
            Tab::Wave => "Wave",
            Tab::Arrange => "Arrange",
            Tab::Keys => "Keys",
        }
    }

    /// The other half of the Arrange/Keys toggle pair.
    pub fn flipped(self) -> Tab {
        match self {
            Tab::Keys => Tab::Arrange,
            _ => Tab::Keys,
        }
    }
}

/// Solved panel geometry.
pub struct Panel {
    /// The lighter surface down the panel's left side.
    pub gutter: Viewport,
    /// The inset box the lane names live in.
    pub names_box: Viewport,
    /// The ruler row along the top of the time axis.
    pub ruler: Viewport,
    /// Where lane rows live.
    pub lanes: Viewport,
    /// Time-axis horizontal range (x, w).
    pub axis: (f32, f32),
    /// Vertical extent of the shaded axis area (ruler bottom → panel
    /// bottom); the playhead and bar shading span it.
    pub axis_y: (f32, f32),
}

pub fn panel(tl: Viewport, scale: f32) -> Panel {
    let pad = 12.0 * scale;
    let bottom = tl.y + tl.h - pad;
    let names_box = Viewport {
        x: tl.x + 8.0 * scale,
        y: tl.y + 8.0 * scale,
        w: (GUTTER - 16.0) * scale,
        h: (bottom - tl.y - 12.0 * scale).max(1.0),
    };
    let axis_x = tl.x + GUTTER * scale + 8.0 * scale;
    let axis_w = (tl.x + tl.w - pad - axis_x).max(1.0);
    let ruler = Viewport {
        x: axis_x,
        y: tl.y + 8.0 * scale,
        w: axis_w,
        h: RULER_H * scale,
    };
    let lanes_y = ruler.y + ruler.h + 6.0 * scale;
    Panel {
        gutter: Viewport {
            x: tl.x,
            y: tl.y + 3.0 * scale,
            w: GUTTER * scale,
            h: tl.h - 3.0 * scale,
        },
        names_box,
        ruler,
        lanes: Viewport {
            x: tl.x,
            y: lanes_y,
            w: tl.w,
            h: (bottom - lanes_y).max(0.0),
        },
        axis: (axis_x, axis_w),
        axis_y: (ruler.y + ruler.h, bottom),
    }
}

/// The transport toolbar's controls for the active tab: the square Wave
/// button and the Arrange/Keys toggle on the left, then that tab's own
/// tools (only Keys carries the keyframe stamp), play front and center.
pub struct Controls {
    /// Square button that jumps to the Wave tab.
    pub wave: Viewport,
    /// The rectangular Arrange/Keys toggle.
    pub tk: Viewport,
    /// Keyframe stamp — a Keys-tab tool, absent everywhere else.
    pub key: Option<Viewport>,
    pub play: Viewport,
}

pub fn controls(toolbar: Viewport, scale: f32, tab: Tab) -> Controls {
    let btn = toolbar.h - 16.0 * scale;
    let y = toolbar.y + 8.0 * scale;
    let wave = Viewport {
        x: toolbar.x + 12.0 * scale,
        y,
        w: btn,
        h: btn,
    };
    let tk = Viewport {
        x: wave.x + wave.w + 8.0 * scale,
        y,
        w: 150.0 * scale,
        h: btn,
    };
    let key = (tab == Tab::Keys).then_some(Viewport {
        x: tk.x + tk.w + 20.0 * scale,
        y,
        w: btn,
        h: btn,
    });
    let play_side = 58.0 * scale;
    let play = Viewport {
        x: toolbar.x + (toolbar.w - play_side) * 0.5,
        y: toolbar.y + (toolbar.h - play_side) * 0.5,
        w: play_side,
        h: play_side,
    };
    Controls {
        wave,
        tk,
        key,
        play,
    }
}

/// Toolbar controls: the Wave button and Arrange/Keys toggle (active =
/// purple card, the tool recipe), the active tab's tools, and a play
/// button you can actually see.
pub fn toolbar_rects(
    c: &Controls,
    scale: f32,
    playing: bool,
    hover_play: bool,
    hover_key: bool,
    tab: Tab,
) -> Vec<UiRect> {
    let t = theme();
    let mut out = Vec::new();
    out.push(UiRect::region_rounded(
        c.wave,
        if tab == Tab::Wave {
            t.accent_bg
        } else {
            t.card
        },
        10.0 * scale,
    ));
    // The wave glyph: five peak bars, teal while the tab is live.
    let col = if tab == Tab::Wave { t.wave } else { t.icon };
    let heights = [0.35, 0.72, 1.0, 0.5, 0.82];
    let bw = c.wave.w * 0.075;
    let gap = c.wave.w * 0.075;
    let x0 = c.wave.x + (c.wave.w - (bw * 5.0 + gap * 4.0)) * 0.5;
    let mid = c.wave.y + c.wave.h * 0.5;
    for (i, hf) in heights.into_iter().enumerate() {
        let bh = c.wave.h * 0.58 * hf;
        out.push(UiRect::region_rounded(
            Viewport {
                x: x0 + (bw + gap) * i as f32,
                y: mid - bh * 0.5,
                w: bw,
                h: bh,
            },
            col,
            bw * 0.5,
        ));
    }
    out.push(UiRect::region_rounded(
        c.tk,
        if tab == Tab::Wave {
            t.card
        } else {
            t.accent_bg
        },
        10.0 * scale,
    ));
    if let Some(key) = c.key {
        out.push(UiRect::region_rounded(
            key,
            if hover_key {
                srgb(0x3a3a3a)
            } else {
                srgb(0x2b2b2b)
            },
            12.0 * scale,
        ));
        out.push(UiRect::icon_sized(key, ICON_KEY, 0.0, t.playhead, 0.42));
    }
    let play_bg = if playing {
        // A dark green well under the lit pause glyph.
        srgb(0x1a4a2c)
    } else if hover_play {
        srgb(0x424242)
    } else {
        srgb(0x343434)
    };
    out.push(UiRect::region_rounded(c.play, play_bg, 14.0 * scale));
    out.push(UiRect::icon_sized(
        c.play,
        if playing { ICON_PAUSE } else { ICON_PLAY },
        2.5 * scale,
        srgb(0x3fdc74),
        0.34,
    ));
    out
}

/// The sidebar: lighter background and the inset lane-name box.
pub fn sidebar_rects(panel: &Panel, scale: f32) -> Vec<UiRect> {
    vec![
        UiRect::region(panel.gutter, srgb(0x1a1a1a)),
        UiRect::region_rounded(panel.names_box, srgb(0x121212), 10.0 * scale),
    ]
}

/// The Wave tab: one min/max teal column per ~2 logical px across the
/// axis, aggregated from the track's peak buckets and mapped through the
/// zoomable time view — the waveform zooms and pans with the ruler.
pub fn wave_rects(
    panel: &Panel,
    view: &TimeView,
    scale: f32,
    track: &spark_audio::Track,
) -> Vec<UiRect> {
    if track.peaks.is_empty() {
        return Vec::new();
    }
    let teal = theme().wave;
    let (y0, y1) = panel.axis_y;
    let mid = (y0 + y1) * 0.5;
    let half_h = ((y1 - y0) * 0.5 - 6.0 * scale).max(1.0);
    let (ax, aw) = panel.axis;
    let bucket_s = spark_audio::PEAK_BUCKET as f32 / spark_audio::SAMPLE_RATE as f32;
    let step = 2.0 * scale;
    let cols = (aw / step).max(1.0) as usize;
    let mut out = Vec::with_capacity(cols);
    for col in 0..cols {
        let ta = view.t_at(ax + col as f32 * step, panel.axis);
        if ta >= track.duration {
            break;
        }
        let tb = view.t_at(ax + (col + 1) as f32 * step, panel.axis);
        let a = ((ta / bucket_s) as usize).min(track.peaks.len() - 1);
        let b = ((tb / bucket_s).ceil() as usize).clamp(a + 1, track.peaks.len());
        let mut lo = 0.0f32;
        let mut hi = 0.0f32;
        for p in &track.peaks[a..b] {
            lo = lo.min(p[0]);
            hi = hi.max(p[1]);
        }
        out.push(UiRect::region(
            Viewport {
                x: ax + col as f32 * step,
                y: mid - hi * half_h,
                w: (step * 0.75).max(1.0),
                h: ((hi - lo) * half_h).max(1.0 * scale),
            },
            teal,
        ));
    }
    out
}

/// The visible slice of song time. It never reaches before `min` (the
/// first bar); zoom keeps the time under the cursor still.
#[derive(Clone, Copy)]
pub struct TimeView {
    pub t0: f32,
    pub t1: f32,
    pub min: f32,
}

impl TimeView {
    pub fn new(min: f32, duration: f32) -> Self {
        let min = min.clamp(0.0, (duration - 1.0).max(0.0));
        Self {
            t0: min,
            t1: duration.max(min + 1.0),
            min,
        }
    }

    pub fn span(&self) -> f32 {
        (self.t1 - self.t0).max(0.001)
    }

    pub fn x_of(&self, t: f32, axis: (f32, f32)) -> f32 {
        axis.0 + (t - self.t0) / self.span() * axis.1
    }

    pub fn t_at(&self, x: f32, axis: (f32, f32)) -> f32 {
        self.t0 + (x - axis.0) / axis.1.max(1.0) * self.span()
    }

    pub fn zoom(&mut self, factor: f32, pivot: f32, duration: f32) {
        let span = (self.span() * factor).clamp(0.5, (duration - self.min).max(1.0));
        let f = ((pivot - self.t0) / self.span()).clamp(0.0, 1.0);
        self.t0 = pivot - f * span;
        self.t1 = self.t0 + span;
        self.clamp(duration);
    }

    pub fn pan(&mut self, dt: f32, duration: f32) {
        let span = self.span();
        self.t0 += dt;
        self.t1 = self.t0 + span;
        self.clamp(duration);
    }

    fn clamp(&mut self, duration: f32) {
        let span = self.span().min((duration - self.min).max(1.0));
        if self.t1 > duration.max(1.0) {
            self.t1 = duration.max(1.0);
            self.t0 = self.t1 - span;
        }
        if self.t0 < self.min {
            self.t0 = self.min;
            self.t1 = self.t0 + span;
        }
    }
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
) -> Vec<UiRect> {
    let (y0, y1) = panel.axis_y;
    let (ax, aw) = panel.axis;
    let h = (y1 - y0).max(1.0);
    let bar_s = 4.0 * 60.0 / beat.bpm.max(1.0);
    let beat_s = bar_s * 0.25;
    let px_per_beat = beat_s / view.span() * aw;
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
        [1.0, 1.0, 1.0, 0.016],
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
                [1.0, 1.0, 1.0, 0.045],
            ));
        }
        // Quarter-note lines inside the bar, once beats have ~24px each.
        if px_per_beat >= 24.0 * scale {
            for q in 1..4 {
                let bt = t + beat_s * q as f32;
                if bt >= view.t0 && bt <= end {
                    out.push(UiRect::region(
                        Viewport {
                            x: view.x_of(bt, panel.axis) - 0.5 * scale,
                            y: y0,
                            w: 1.0 * scale,
                            h,
                        },
                        [1.0, 1.0, 1.0, 0.07],
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
    let mut out = vec![UiRect::region(
        Viewport {
            x: r.x,
            y: r.y + r.h - 1.5 * scale,
            w: r.w,
            h: 1.5 * scale,
        },
        [1.0, 1.0, 1.0, 0.10],
    )];
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

/// Bar-number labels for the ruler: (x, label), thinned so numbers never
/// crowd — every bar zoomed in, every 4/8/16 zoomed out.
pub fn ruler_marks(
    panel: &Panel,
    view: &TimeView,
    scale: f32,
    beat: &spark_audio::BeatGrid,
    duration: f32,
) -> Vec<(f32, String)> {
    let bar_s = 4.0 * 60.0 / beat.bpm.max(1.0);
    let px_per_bar = bar_s / view.span() * panel.axis.1;
    let every = [1i64, 2, 4, 8, 16, 32]
        .into_iter()
        .find(|n| px_per_bar * *n as f32 >= 72.0 * scale)
        .unwrap_or(64);
    let first = (((view.t0 - beat.first_bar) / bar_s).ceil() as i64).max(0);
    let mut out = Vec::new();
    let mut k = (first + every - 1) / every * every;
    loop {
        let time = beat.first_bar + k as f32 * bar_s;
        if time > view.t1 || time > duration {
            break;
        }
        out.push((
            view.x_of(time, panel.axis) + 6.0 * scale,
            format!("{}", k + 1),
        ));
        k += every;
    }
    out
}

/// Snap a time to the nearest bar line — loop edges live on bars.
pub fn bar_quantize(t: f32, beat: &spark_audio::BeatGrid) -> f32 {
    let bar_s = 4.0 * 60.0 / beat.bpm.max(1.0);
    beat.first_bar + ((t - beat.first_bar) / bar_s).round() * bar_s
}

/// The start of the bar containing `t`.
pub fn bar_floor(t: f32, beat: &spark_audio::BeatGrid) -> f32 {
    let bar_s = 4.0 * 60.0 / beat.bpm.max(1.0);
    beat.first_bar + ((t - beat.first_bar) / bar_s).floor().max(0.0) * bar_s
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
    if time < view.t0 || time > view.t1 {
        return None;
    }
    let x = view.x_of(time, panel.axis);
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
