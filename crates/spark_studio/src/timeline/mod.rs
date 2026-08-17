//! Timeline panel chrome: the lane-name sidebar, a bars/beats ruler along
//! the top of the time axis, alternating bar shading, and the zoomable
//! time view every element (keys, waveform, playhead, scrub) maps through.
//! The transport toolbar above the panel holds the Wave button, the
//! Arrange/Keys toggle, the active tab's own tools, and play. Time starts
//! at the first bar — the pickup before it isn't part of the choreography.

use spark_render::Viewport;
mod draw;

pub use draw::{
    loop_rects, playhead_rect, ruler_rects, shade_rects, sidebar_rects, toolbar_rects, wave_rects,
};

/// The sidebar is deliberately roomy: its right column holds the lane
/// names (aligned to the rows on the axis), and the left column is the
/// tools bay — the hero Keyframe button today, keyframe settings later.
const GUTTER: f32 = 520.0;
/// How much of the sidebar the lane-name column takes.
const NAMES_W: f32 = 250.0;
const RULER_H: f32 = 30.0;

/// Bottom-panel tabs. Wave is the resting default; Arrange and Keys share
/// the rectangular toggle button.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tab {
    Wave,
    Arrange,
    Keys,
}

/// Solved panel geometry.
pub struct Panel {
    /// The lighter surface down the panel's left side.
    pub gutter: Viewport,
    /// The sidebar's left column: the hero Keyframe button and, later, the
    /// keyframe settings and tools that go with it.
    pub tools: Viewport,
    /// The hero Add Keyframe button at the top of the tools bay.
    pub stamp: Viewport,
    /// The inset box the lane names live in — the sidebar's right column,
    /// butted against the axis so rows line up with their names.
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
        x: tl.x + (GUTTER - NAMES_W) * scale,
        y: tl.y + 8.0 * scale,
        w: (NAMES_W - 8.0) * scale,
        h: (bottom - tl.y - 12.0 * scale).max(1.0),
    };
    let tools = Viewport {
        x: tl.x + 10.0 * scale,
        y: tl.y + 8.0 * scale,
        w: (GUTTER - NAMES_W - 20.0) * scale,
        h: (bottom - tl.y - 12.0 * scale).max(1.0),
    };
    let stamp = Viewport {
        x: tools.x + 10.0 * scale,
        y: tools.y + 10.0 * scale,
        w: (190.0 * scale).min(tools.w - 20.0 * scale).max(1.0),
        h: 46.0 * scale,
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
        tools,
        stamp,
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
    /// One square button per tab: wave, arrange, keys.
    pub tabs: [Viewport; 3],
    /// Hairline between the view tabs and the mode toggles beside them.
    pub divider: Viewport,
    /// Playhead-snaps-to-beat toggle.
    pub snap: Viewport,
    pub play: Viewport,
}

/// The tab each toolbar square selects, in display order.
pub const TAB_ORDER: [Tab; 3] = [Tab::Wave, Tab::Arrange, Tab::Keys];

pub fn controls(toolbar: Viewport, scale: f32, _tab: Tab) -> Controls {
    let btn = toolbar.h - 16.0 * scale;
    let y = toolbar.y + 8.0 * scale;
    let x0 = toolbar.x + 12.0 * scale;
    let step = btn + 8.0 * scale;
    let tabs = [0, 1, 2].map(|i| Viewport {
        x: x0 + step * i as f32,
        y,
        w: btn,
        h: btn,
    });
    // Set apart from the tab trio — it's a mode, not a view.
    let divider = Viewport {
        x: x0 + step * 3.0 + 9.0 * scale,
        y: y + btn * 0.16,
        w: 2.0 * scale,
        h: btn * 0.68,
    };
    let snap = Viewport {
        x: divider.x + 19.0 * scale,
        y,
        w: btn,
        h: btn,
    };
    let play_side = 58.0 * scale;
    let play = Viewport {
        x: toolbar.x + (toolbar.w - play_side) * 0.5,
        y: toolbar.y + (toolbar.h - play_side) * 0.5,
        w: play_side,
        h: play_side,
    };
    Controls {
        tabs,
        divider,
        snap,
        play,
    }
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

    /// The resting view a track opens at: `bars` bars from the first bar.
    /// Wide enough to read a phrase, tight enough that quarter-note lines
    /// still come in — the zoom you actually work at.
    pub fn bars(beat: &spark_audio::BeatGrid, duration: f32, bars: f32) -> Self {
        let mut v = Self::new(beat.first_bar, duration);
        let span = (4.0 * 60.0 / beat.bpm.max(1.0) * bars).min(duration - v.min);
        if span > 0.5 {
            v.t1 = v.t0 + span;
        }
        v
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



#[cfg(test)]
mod tests {
    use super::*;
    use spark_audio::BeatGrid;

    fn grid(bpm: f32) -> BeatGrid {
        BeatGrid {
            bpm,
            first_bar: 0.0,
        }
    }

    #[test]
    fn the_resting_view_shows_sixteen_bars() {
        // 120 BPM: a bar is 2s, so 16 bars is 32s.
        let v = TimeView::bars(&grid(120.0), 300.0, 16.0);
        assert!((v.span() - 32.0).abs() < 0.01, "span was {}", v.span());
        assert_eq!(v.t0, 0.0);
    }

    #[test]
    fn a_short_track_just_shows_all_of_itself() {
        // 16 bars would overrun a 10s track — don't scroll past the end.
        let v = TimeView::bars(&grid(120.0), 10.0, 16.0);
        assert!(v.span() <= 10.01, "span was {}", v.span());
    }
}
