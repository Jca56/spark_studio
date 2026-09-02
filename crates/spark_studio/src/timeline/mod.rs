//! Timeline panel chrome: the track sidebar (the outliner's home), a
//! bars/beats ruler along the top of the time axis, alternating bar
//! shading, and the zoomable time view every element (clips, waveform,
//! playhead, scrub) maps through. The transport toolbar above the panel
//! holds snap, tempo, play and the canvas zoom cluster. Time starts at
//! the first bar — the pickup before it isn't part of the choreography.

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
    // The panel runs flush to its own bottom edge. It used to stop 12px
    // short, from back when that edge was the window's and the margin was
    // the only thing keeping content off it — the status strip closes the
    // panel now, so the margin was just a dead gap above the floor.
    let bottom = tl.y + tl.h;
    let top = tl.y + 8.0 * scale;
    let sidebar_h = (bottom - top).max(1.0);
    let names_box = Viewport {
        x: tl.x + (GUTTER - NAMES_W) * scale,
        y: top,
        w: (NAMES_W - 8.0) * scale,
        h: sidebar_h,
    };
    let tools = Viewport {
        x: tl.x + 10.0 * scale,
        y: top,
        w: (GUTTER - NAMES_W - 20.0) * scale,
        h: sidebar_h,
    };
    // Square, and centred across the bay — it's the one control up there.
    let side = (66.0 * scale).min(tools.w - 20.0 * scale).max(1.0);
    let stamp = Viewport {
        x: tools.x + (tools.w - side) * 0.5,
        y: tools.y + 12.0 * scale,
        w: side,
        h: side,
    };
    let axis_x = tl.x + GUTTER * scale + 8.0 * scale;
    let axis_w = (tl.x + tl.w - pad - axis_x).max(1.0);
    let ruler = Viewport {
        x: axis_x,
        y: top,
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

/// The transport toolbar's controls: snap and tempo left of play, play
/// front and center, the canvas zoom cluster at the right end.
pub struct Controls {
    /// Playhead-snaps-to-grid toggle.
    pub snap: Viewport,
    /// The waveform-overlay toggle, beside it: the song's waveform laid
    /// faintly across the whole grid, a guide behind the clips.
    pub wave: Viewport,
    /// The tempo field, left of play. Detection is a guess and this is
    /// where the person who made the track says otherwise.
    pub bpm: Viewport,
    pub play: Viewport,
    /// Canvas zoom at the toolbar's right end: - / + steppers and the
    /// readout button (shows the live percentage, refits to 100% on
    /// click). Moved here from the old right-panel zoom bar (2026-08-31).
    pub zoom_minus: Viewport,
    pub zoom_plus: Viewport,
    pub zoom_pct: Viewport,
}

pub fn controls(toolbar: Viewport, scale: f32) -> Controls {
    let btn = toolbar.h - 16.0 * scale;
    let y = toolbar.y + 8.0 * scale;
    let x0 = toolbar.x + 12.0 * scale;
    let snap = Viewport {
        x: x0,
        y,
        w: btn,
        h: btn,
    };
    let wave = Viewport {
        x: x0 + btn + 8.0 * scale,
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
    // Immediately left of play, and wide enough for a three-digit tempo at
    // a size Alva can read from across the room.
    let bpm_w = 150.0 * scale;
    let bpm = Viewport {
        x: play.x - bpm_w - 22.0 * scale,
        y,
        w: bpm_w,
        h: btn,
    };
    // The zoom cluster, right-aligned: minus, plus, then the readout at
    // the toolbar's far end — the same three buttons the old zoom bar had.
    let zoom_pct = Viewport {
        x: toolbar.x + toolbar.w - 12.0 * scale - 130.0 * scale,
        y,
        w: 130.0 * scale,
        h: btn,
    };
    let zoom_plus = Viewport {
        x: zoom_pct.x - 14.0 * scale - btn,
        y,
        w: btn,
        h: btn,
    };
    let zoom_minus = Viewport {
        x: zoom_plus.x - 8.0 * scale - btn,
        y,
        w: btn,
        h: btn,
    };
    Controls {
        snap,
        wave,
        bpm,
        play,
        zoom_minus,
        zoom_plus,
        zoom_pct,
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

/// The grid: what a scrub, a dragged clip and a dragged key snap to,
/// and the lines the lanes draw inside each bar — a bar, or a fraction
/// of one (Alva, 2026-09-01: "changing the grid between 1/16 1/8 1/4
/// 1/2 1 — that would be huge"). Picked in the timeline's right-click
/// menu; a beat (1/4) is where it starts, the snap's old fixed step.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Grid {
    Bar,
    Half,
    #[default]
    Quarter,
    Eighth,
    Sixteenth,
}

impl Grid {
    /// Finest first — the order the menu's switch shows them.
    pub const ALL: [Grid; 5] = [
        Grid::Sixteenth,
        Grid::Eighth,
        Grid::Quarter,
        Grid::Half,
        Grid::Bar,
    ];
    pub const LABELS: [&'static str; 5] = ["1/16", "1/8", "1/4", "1/2", "1"];

    pub fn label(self) -> &'static str {
        Self::LABELS[self.index()]
    }

    /// Its place in [`Grid::ALL`].
    pub fn index(self) -> usize {
        Self::ALL.iter().position(|g| *g == self).unwrap_or(2)
    }

    /// How many steps make a bar.
    pub fn per_bar(self) -> f32 {
        match self {
            Grid::Bar => 1.0,
            Grid::Half => 2.0,
            Grid::Quarter => 4.0,
            Grid::Eighth => 8.0,
            Grid::Sixteenth => 16.0,
        }
    }

    /// The grid for a `per_bar` count read off a file, if it is one.
    pub fn from_per_bar(n: u32) -> Option<Grid> {
        Self::ALL.into_iter().find(|g| g.per_bar() as u32 == n)
    }

    /// One step, in seconds, at `bpm` (four beats to the bar).
    pub fn step_s(self, bpm: f32) -> f32 {
        4.0 * 60.0 / bpm.max(1.0) / self.per_bar()
    }

    /// `t` snapped to the nearest step, counting from `first_bar`.
    pub fn snap(self, t: f32, first_bar: f32, bpm: f32) -> f32 {
        let step = self.step_s(bpm);
        first_bar + ((t - first_bar) / step).round() * step
    }
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

    /// A grid step is the bar's fraction; snapping rounds to it from the
    /// first bar, so 1/16 at 120 bpm is an eighth of a second.
    #[test]
    fn the_grid_steps_by_its_fraction_of_a_bar() {
        assert_eq!(Grid::default(), Grid::Quarter, "a beat, the old fixed step");
        assert!((Grid::Sixteenth.step_s(120.0) - 0.125).abs() < 1e-6);
        assert!((Grid::Bar.step_s(120.0) - 2.0).abs() < 1e-6);
        assert!((Grid::Eighth.snap(1.3, 0.5, 120.0) - 1.25).abs() < 1e-6);
        assert!((Grid::Half.snap(1.3, 0.0, 120.0) - 1.0).abs() < 1e-6);
        for (g, l) in Grid::ALL.iter().zip(Grid::LABELS) {
            assert_eq!(g.label(), l);
            assert_eq!(Grid::ALL[g.index()], *g);
            assert_eq!(Grid::from_per_bar(g.per_bar() as u32), Some(*g));
        }
        assert_eq!(Grid::from_per_bar(3), None);
    }

    fn grid(bpm: f32) -> BeatGrid {
        BeatGrid {
            bpm,
            first_bar: 0.0,
        }
    }

    /// The beat grid and the sidebar's two wells run flush to the bottom of
    /// the panel. The status strip is what closes the layout now, so a
    /// margin here only opened a dead gap between the grid and the floor.
    #[test]
    fn the_panel_reaches_its_own_bottom_edge() {
        for scale in [1.0f32, 1.4] {
            let tl = Viewport {
                x: 0.0,
                y: 300.0,
                w: 3000.0,
                h: 500.0,
            };
            let p = panel(tl, scale);
            let floor = tl.y + tl.h;
            let flush = |name: &str, edge: f32| {
                assert!(
                    (edge - floor).abs() < 0.5,
                    "scale {scale}: {name} stops {} px short",
                    (floor - edge) / scale
                );
            };
            flush("the beat grid", p.axis_y.1);
            flush("the lane region", p.lanes.y + p.lanes.h);
            flush("the tools bay", p.tools.y + p.tools.h);
            flush("the lane-name box", p.names_box.y + p.names_box.h);
            // Still a panel, not an inverted one.
            assert!(p.tools.h > 0.0 && p.names_box.h > 0.0);
        }
    }

    #[test]
    fn the_resting_view_shows_sixteen_bars() {
        // 120 BPM: a bar is 2s, so 16 bars is 32s.
        let v = TimeView::bars(&grid(120.0), 300.0, 16.0);
        assert!((v.span() - 32.0).abs() < 0.01, "span was {}", v.span());
        assert_eq!(v.t0, 0.0);
    }

    /// The tempo field sits left of play without touching it, and clear of
    /// the tab buttons on the other side — nobody who can run this can look
    /// at the toolbar to check.
    #[test]
    fn the_tempo_field_fits_between_the_tabs_and_play() {
        let bar = Viewport {
            x: 0.0,
            y: 0.0,
            w: 1600.0,
            h: 64.0,
        };
        for scale in [1.0f32, 1.4] {
            let c = controls(bar, scale);
            assert!(
                c.bpm.x + c.bpm.w < c.play.x,
                "scale {scale}: tempo field overlaps play"
            );
            let snap_end = c.snap.x + c.snap.w;
            assert!(c.wave.x > snap_end, "scale {scale}: the wave button sits on snap");
            assert!(c.bpm.x > c.wave.x + c.wave.w, "scale {scale}: tempo field hits the wave button");
            assert!(c.bpm.w > 90.0 * scale, "too narrow to read a tempo in");
        }
    }

    /// The zoom cluster sits at the toolbar's right end: inside the bar,
    /// in order minus → plus → readout, and clear of the play button in
    /// the middle — nobody who can run this can look at the toolbar.
    #[test]
    fn the_zoom_cluster_fits_the_toolbars_right_end() {
        let bar = Viewport {
            x: 0.0,
            y: 0.0,
            w: 1600.0,
            h: 64.0,
        };
        for scale in [1.0f32, 1.4] {
            let c = controls(bar, scale);
            assert!(
                c.zoom_pct.x + c.zoom_pct.w <= bar.x + bar.w,
                "scale {scale}: the readout runs off the window"
            );
            assert!(
                c.zoom_minus.x + c.zoom_minus.w < c.zoom_plus.x
                    && c.zoom_plus.x + c.zoom_plus.w < c.zoom_pct.x,
                "scale {scale}: the cluster is out of order"
            );
            assert!(
                c.zoom_minus.x > c.play.x + c.play.w,
                "scale {scale}: the zoom cluster hits play"
            );
            for (name, b) in [("minus", c.zoom_minus), ("plus", c.zoom_plus)] {
                assert!(
                    b.y >= bar.y && b.y + b.h <= bar.y + bar.h,
                    "scale {scale}: {name} escapes the bar"
                );
            }
        }
    }

    /// A comp with no track still keeps a clock, so its resting view has to
    /// be a real window. It used to open at `TimeView::new(0.0, 1.0)` — a
    /// one-second span — because the timeline didn't exist without audio.
    #[test]
    fn a_silent_comp_opens_on_sixteen_bars() {
        use crate::transport::{SILENT_BPM, SILENT_DURATION};
        let v = TimeView::bars(&grid(SILENT_BPM), SILENT_DURATION, 16.0);
        // 120 BPM: a bar is 2s, so 16 bars is 32s.
        assert!((v.span() - 32.0).abs() < 0.01, "span was {}", v.span());
        assert!(v.t1 < SILENT_DURATION, "a window, not the whole comp");
    }

    #[test]
    fn a_short_track_just_shows_all_of_itself() {
        // 16 bars would overrun a 10s track — don't scroll past the end.
        let v = TimeView::bars(&grid(120.0), 10.0, 16.0);
        assert!(v.span() <= 10.01, "span was {}", v.span());
    }
}
