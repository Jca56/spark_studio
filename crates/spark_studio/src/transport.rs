//! Transport control: the timeline's clock, play/pause, the loop region,
//! playhead-vs-keyframe keyboard moves (`,` `.` jump, arrow nudges),
//! scrubbing, and lane hit testing. Split from main so the event plumbing
//! stays readable.

use std::time::Instant;

use spark_audio::BeatGrid;

use crate::Studio;

/// Tempo a comp keeps time at before a track has been imported. Detection
/// replaces it the moment one lands, and the tempo field overrides both.
pub(crate) const SILENT_BPM: f32 = 120.0;

/// How long a comp is with no track to measure: two minutes, or sixty bars
/// at [`SILENT_BPM`]. Long enough to choreograph against, and the song
/// replaces it outright rather than being fitted into it.
pub(crate) const SILENT_DURATION: f32 = 120.0;

/// The clock a comp runs on while playing with no track loaded.
///
/// With audio, the transport clock *is* the audio callback's cursor, so
/// picture and sound can't drift. With no audio there's no cursor to read,
/// so playback runs on wall time: remember where the playhead was and when
/// play was pressed, and the position is the sum. Every seek re-anchors it,
/// which is what keeps scrubbing mid-playback from snapping back.
pub(crate) struct SilentClock {
    from: Instant,
    at: f32,
}

impl SilentClock {
    fn started_at(t: f32) -> Self {
        Self {
            from: Instant::now(),
            at: t,
        }
    }

    fn now(&self) -> f32 {
        self.at + self.from.elapsed().as_secs_f32()
    }
}

/// Where a free-running clock reading `raw` actually lands.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) enum Tick {
    /// Still running, at this time.
    Run(f32),
    /// Cycled back into the loop region; the clock re-anchors here.
    Wrap(f32),
    /// Reached the end of the comp — the transport stops here.
    End(f32),
}

/// Resolve a raw wall-clock reading against the loop region and the comp's
/// end. Split out from [`Studio::advance_clock`] because it's the part with
/// the decisions in it, and nothing about it needs a window to test.
pub(crate) fn tick(raw: f32, cycle: Option<(f32, f32)>, end: f32) -> Tick {
    if let Some((a, b)) = cycle
        && b > a
        && raw >= b
    {
        // Wrap by the loop's *length* rather than snapping to its start: a
        // long frame would otherwise lose its overshoot, and a dropped frame
        // must not nudge the choreography off the grid. A playhead parked
        // before the region plays into it normally, which is what the audio
        // player does too.
        return Tick::Wrap(a + (raw - a) % (b - a));
    }
    if raw >= end {
        return Tick::End(end);
    }
    Tick::Run(raw)
}

impl Studio {
    /// The timing every element on the timeline maps through — ruler, bar
    /// shading, quantization, the playhead.
    ///
    /// A loaded track owns it. Without one the comp still keeps a clock, so
    /// the Keys tab, the lanes and the playhead all exist before a song is
    /// imported: choreography can start on a blank comp and the track can
    /// arrive afterwards. Gating this on `audio` meant no audio, no
    /// animating anything at all.
    pub(crate) fn grid(&self) -> BeatGrid {
        match &self.audio {
            Some(t) => t.beat,
            None => BeatGrid {
                bpm: self.editor.bpm_override().unwrap_or(SILENT_BPM),
                first_bar: 0.0,
            },
        }
    }

    /// How much time the timeline spans — the track's length, or the silent
    /// comp's default.
    pub(crate) fn duration(&self) -> f32 {
        match &self.audio {
            Some(t) => t.duration,
            None => SILENT_DURATION,
        }
    }

    /// Whether the transport is running, on either clock.
    pub(crate) fn playing(&self) -> bool {
        match &self.player {
            Some(p) => p.is_playing(),
            None => self.silent_play.is_some(),
        }
    }

    /// Space / the play button. With a track this drives the audio stream;
    /// without one it runs the comp on wall time, so a silent comp still
    /// plays back rather than only moving when the playhead is dragged.
    pub(crate) fn toggle_play(&mut self) -> bool {
        if let Some(p) = &self.player {
            p.toggle();
            return true;
        }
        self.silent_play = match self.silent_play {
            Some(_) => None,
            // Pressing play at the very end restarts from the top, the same
            // as the audio player does.
            None => {
                let t = self.editor.time();
                let start = if t >= self.duration() - 0.001 {
                    self.grid().first_bar
                } else {
                    t
                };
                Some(SilentClock::started_at(start))
            }
        };
        true
    }

    /// Move the playhead, on whichever clock is running. The single seek —
    /// a bare `editor.set_time` would leave a playing clock anchored to
    /// where the playhead *used* to be, and the next frame would snap back.
    pub(crate) fn seek(&mut self, t: f32) {
        if let Some(p) = &self.player {
            p.seek(t);
        }
        if self.silent_play.is_some() {
            self.silent_play = Some(SilentClock::started_at(t));
        }
        self.editor.set_time(t);
    }

    /// Advance the silent clock into the editor's time. Called once per
    /// frame; a no-op while a track is loaded, since then the audio cursor
    /// is the clock and this one isn't running.
    pub(crate) fn advance_clock(&mut self) {
        let Some(clock) = &self.silent_play else {
            return;
        };
        let cycle = self.loop_on.then_some(self.loop_region).flatten();
        let t = match tick(clock.now(), cycle, self.duration()) {
            Tick::Run(t) => t,
            // Re-anchor only on a wrap. Re-anchoring every frame would be
            // simpler and would accumulate a frame's worth of f32 rounding
            // thousands of times across a take.
            Tick::Wrap(t) => {
                self.silent_play = Some(SilentClock::started_at(t));
                t
            }
            Tick::End(t) => {
                self.silent_play = None;
                t
            }
        };
        self.editor.set_time(t);
    }

    /// Push the loop region into the player (or clear it). The silent clock
    /// reads `loop_region` directly, so it needs nothing here.
    pub(crate) fn apply_loop(&mut self) {
        if let Some(p) = &self.player {
            match (self.loop_region, self.loop_on) {
                (Some((a, b)), true) => p.set_loop(a, b),
                _ => p.clear_loop(),
            }
        }
    }

    /// `L`: toggle the loop region on/off.
    pub(crate) fn toggle_loop(&mut self) -> bool {
        if self.loop_region.is_none() {
            println!("no loop region — Shift+drag the ruler to set one");
            return false;
        }
        self.loop_on = !self.loop_on;
        self.apply_loop();
        println!("loop {}", if self.loop_on { "on" } else { "off" });
        true
    }

    /// Seek the playhead to the time under `x` and start a scrub drag.
    pub(crate) fn seek_to_x(&mut self, panel: &crate::timeline::Panel, x: f32) {
        let raw = self.time_view.t_at(x, panel.axis);
        let t = self
            .snap_time(raw)
            .clamp(self.grid().first_bar, self.duration());
        self.seek(t);
        self.timeline_scrub = true;
        self.request_redraw();
    }

    /// Grid quantization, while playhead snap is on — the step is the
    /// grid picked in the timeline's menu (a beat to start).
    pub(crate) fn snap_time(&self, t: f32) -> f32 {
        if !self.snap_playhead {
            return t;
        }
        let grid = self.grid();
        self.grid_div.snap(t, grid.first_bar, grid.bpm)
    }

    /// Right-click on the bottom panel: in the clip view, the menu on
    /// what was clicked; on the arrangement, the timeline's own menu —
    /// the grid, and the loop's clearing.
    pub(crate) fn right_press(&mut self) {
        if self.export.is_some() {
            return;
        }
        let (cx, cy) = (self.cursor_px.0 as f32, self.cursor_px.1 as f32);
        if self.clip_view_right_press(cx, cy) {
            return;
        }
        if let Some(layout) = self.layout()
            && layout.timeline.contains(cx, cy)
        {
            self.context_open([cx, cy], crate::context::Target::Timeline);
        }
    }

    /// The timeline menu's Clear loop. True when there was one.
    pub(crate) fn clear_loop(&mut self) -> bool {
        if self.loop_region.take().is_none() {
            return false;
        }
        self.loop_on = false;
        self.apply_loop();
        println!("loop cleared");
        self.request_redraw();
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A silent comp plays: the clock just runs forward until something
    /// stops it. Before this, play did nothing at all without a track.
    #[test]
    fn a_free_clock_runs_until_the_end() {
        assert_eq!(tick(4.0, None, 120.0), Tick::Run(4.0));
        assert_eq!(tick(119.9, None, 120.0), Tick::Run(119.9));
        // It stops *at* the end rather than sailing past it.
        assert_eq!(tick(120.0, None, 120.0), Tick::End(120.0));
        assert_eq!(tick(500.0, None, 120.0), Tick::End(120.0));
    }

    /// The loop wraps by its own length, so the overshoot from a long frame
    /// is carried across rather than thrown away. Snapping to the region's
    /// start would walk the playhead off the beat every time a frame ran
    /// long — precisely what a loop is used to check.
    #[test]
    fn the_loop_wraps_by_its_length_not_to_its_start() {
        let cycle = Some((8.0, 12.0));
        assert_eq!(tick(11.9, cycle, 120.0), Tick::Run(11.9));
        // Half a second past the end comes back half a second past the start.
        assert_eq!(tick(12.5, cycle, 120.0), Tick::Wrap(8.5));
        // Even a frame that overshoots by more than the whole region.
        assert_eq!(tick(21.0, cycle, 120.0), Tick::Wrap(9.0));
        // Exactly on the end wraps to exactly the start.
        assert_eq!(tick(12.0, cycle, 120.0), Tick::Wrap(8.0));
    }

    /// A playhead parked before the region plays *into* it, matching what
    /// the audio player's callback does with a cursor outside the loop.
    #[test]
    fn a_playhead_before_the_loop_plays_into_it() {
        assert_eq!(tick(3.0, Some((8.0, 12.0)), 120.0), Tick::Run(3.0));
    }

    /// A degenerate region is not a loop, and must not divide by zero.
    #[test]
    fn an_empty_loop_region_is_ignored() {
        assert_eq!(tick(30.0, Some((9.0, 9.0)), 120.0), Tick::Run(30.0));
        assert_eq!(tick(30.0, Some((12.0, 8.0)), 120.0), Tick::Run(30.0));
    }

    /// The loop outranks the comp end while it's cycling — a region set
    /// before the end keeps playing rather than stopping the transport.
    #[test]
    fn a_cycling_loop_never_reaches_the_end() {
        for step in 0..200 {
            let raw = 8.0 + step as f32 * 0.75;
            assert!(
                !matches!(tick(raw, Some((8.0, 12.0)), 120.0), Tick::End(_)),
                "the transport stopped mid-loop at {raw}"
            );
        }
    }
}
