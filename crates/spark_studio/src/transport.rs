//! Transport control: the timeline's clock, play/pause, the loop region,
//! scrubbing, and the grid. Split from main so the event plumbing
//! stays readable.
//!
//! **The timeline has no end.** It runs from bar one to wherever the
//! last clip is and on past it — Ableton's arrangement, not a strip cut
//! to the song's length. Play runs until stopped; export picks its own
//! range (`Studio::export_range`). What used to be `duration()` is
//! [`OPEN_END`] wherever a bound was needed for its own sake.

use std::time::Instant;

use spark_audio::BeatGrid;

use crate::Studio;

/// Tempo a comp keeps time at before a track has been imported. Detection
/// replaces it the moment one lands, and the tempo field overrides both.
pub(crate) const SILENT_BPM: f32 = 120.0;

/// The timeline's far end — there isn't one. What every painter and
/// clamp that once took the comp's duration is handed now.
pub(crate) const OPEN_END: f32 = f32::INFINITY;

/// How close to a loop edge (logical px) a ruler press grabs it, and
/// how deep the brace's band runs from the ruler's top — a press inside
/// it slides the whole loop; below it scrubs.
const LOOP_GRAB: f32 = 8.0;
const LOOP_BAND: f32 = 12.0;

/// A drag on the loop brace: a fresh region growing from its anchor bar
/// (Shift), one edge, or the whole thing by where it was grabbed.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum LoopDrag {
    New(f32),
    Left,
    Right,
    Body(f32),
}

/// The clock a comp runs on while playing with no audio loaded.
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
}

/// Resolve a raw wall-clock reading against the loop region. Split out
/// from [`Studio::advance_clock`] because it's the part with the
/// decision in it, and nothing about it needs a window to test.
pub(crate) fn tick(raw: f32, cycle: Option<(f32, f32)>) -> Tick {
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
    Tick::Run(raw)
}

impl Studio {
    /// The timing every element on the timeline maps through — ruler, bar
    /// shading, quantization, the playhead.
    ///
    /// The tempo is the song's (or the one typed over it); the **phase**
    /// is the song's own pickup — where its first downbeat falls with
    /// the file at the top of the timeline — and it never follows the
    /// song's clip. **The grid does not move** (Alva, 2026-09-02: "the
    /// timeline shouldn't ever move!?"); the song's beats stay on it
    /// because moving the song snaps its first bar to the grid
    /// (`arrange::group`). Without a song the comp keeps a clock of its
    /// own, so choreography can start before the track arrives.
    pub(crate) fn grid(&self) -> BeatGrid {
        match &self.audio {
            Some(t) => t.beat,
            None => BeatGrid {
                bpm: self.editor.bpm_override().unwrap_or(SILENT_BPM),
                first_bar: 0.0,
            },
        }
    }

    /// Where the song's first bar sits on the timeline, through its
    /// clip — what a snapped move of the song lands on the grid.
    pub(crate) fn song_first_bar(&self) -> Option<f32> {
        let t = self.audio.as_ref()?;
        let c = self.audio_editor().song_clip()?;
        Some(c.start - c.offset + t.beat.first_bar)
    }

    /// Whether the transport is running, on either clock.
    pub(crate) fn playing(&self) -> bool {
        match &self.player {
            Some(p) => p.is_playing(),
            None => self.silent_play.is_some(),
        }
    }

    /// Space / the play button. With audio this drives the device stream;
    /// without it runs the comp on wall time, so a silent comp still
    /// plays back rather than only moving when the playhead is dragged.
    pub(crate) fn toggle_play(&mut self) -> bool {
        if let Some(p) = &self.player {
            p.toggle();
            return true;
        }
        self.silent_play = match self.silent_play {
            Some(_) => None,
            None => Some(SilentClock::started_at(self.editor.time())),
        };
        true
    }

    /// Move the playhead, on whichever clock is running. The single seek —
    /// a bare `editor.set_time` would leave a playing clock anchored to
    /// where the playhead *used* to be, and the next frame would snap back.
    pub(crate) fn seek(&mut self, t: f32) {
        let t = t.max(0.0);
        if let Some(p) = &self.player {
            p.seek(t);
        }
        if self.silent_play.is_some() {
            self.silent_play = Some(SilentClock::started_at(t));
        }
        self.editor.set_time(t);
    }

    /// Advance the silent clock into the editor's time. Called once per
    /// frame; a no-op while audio is loaded, since then the audio cursor
    /// is the clock and this one isn't running.
    pub(crate) fn advance_clock(&mut self) {
        let Some(clock) = &self.silent_play else {
            return;
        };
        let cycle = self.loop_on.then_some(self.loop_region).flatten();
        let t = match tick(clock.now(), cycle) {
            Tick::Run(t) => t,
            // Re-anchor only on a wrap. Re-anchoring every frame would be
            // simpler and would accumulate a frame's worth of f32 rounding
            // thousands of times across a take.
            Tick::Wrap(t) => {
                self.silent_play = Some(SilentClock::started_at(t));
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

    /// `L`, and the toolbar's loop button: the loop on or off. There is
    /// always a region to switch on — four bars from the view's start
    /// the first time — and switching it on never moves it (Alva,
    /// 2026-09-02: "it should just be on/off and when on I drag the
    /// edges, not make a new region").
    pub(crate) fn toggle_loop(&mut self) -> bool {
        if self.loop_region.is_none() {
            let beat = self.grid();
            let a = crate::timeline::bar_floor(self.time_view.t0.max(beat.first_bar), &beat);
            self.loop_region = Some((a, a + 4.0 * 4.0 * 60.0 / beat.bpm.max(1.0)));
        }
        self.loop_on = !self.loop_on;
        self.apply_loop();
        println!("loop {}", if self.loop_on { "on" } else { "off" });
        true
    }

    /// A press on the ruler that is the loop's: Shift brackets a fresh
    /// region from the bar under the cursor; near either edge of the
    /// brace grabs that edge; inside the brace's band grabs the whole
    /// thing to slide. False when the press is a scrub after all.
    pub(crate) fn loop_press(&mut self, panel: &crate::timeline::Panel, cx: f32, cy: f32) -> bool {
        let scale = self.scale();
        let beat = self.grid();
        let t = self.time_view.t_at(cx, panel.axis);
        if self.modifiers.shift_key() {
            let bar_s = 4.0 * 60.0 / beat.bpm.max(1.0);
            let anchor = crate::timeline::bar_floor(t, &beat).max(0.0);
            self.loop_drag = Some(LoopDrag::New(anchor));
            self.loop_region = Some((anchor, anchor + bar_s));
            self.loop_on = true;
            self.apply_loop();
            return true;
        }
        let Some((a, b)) = self.loop_region else {
            return false;
        };
        let grab = LOOP_GRAB * scale;
        let (xa, xb) = (
            self.time_view.x_of(a, panel.axis),
            self.time_view.x_of(b, panel.axis),
        );
        if (cx - xa).abs() <= grab {
            self.loop_drag = Some(LoopDrag::Left);
        } else if (cx - xb).abs() <= grab {
            self.loop_drag = Some(LoopDrag::Right);
        } else if cx > xa && cx < xb && cy <= panel.ruler.y + LOOP_BAND * scale {
            self.loop_drag = Some(LoopDrag::Body(t - a));
        } else {
            return false;
        }
        true
    }

    /// The cursor moved with the loop held. True when the region changed.
    pub(crate) fn loop_moved(&mut self, panel: &crate::timeline::Panel, mx: f32) -> bool {
        let Some(kind) = self.loop_drag else {
            return false;
        };
        let beat = self.grid();
        let raw = self.time_view.t_at(mx, panel.axis);
        let Some((a, b)) = self.loop_region else {
            return false;
        };
        let min = self.grid_div.step_s(beat.bpm).min(b - a).max(0.05);
        let next = match kind {
            LoopDrag::New(anchor) => {
                // Grow by whole bars around the anchor bar.
                let bar_s = 4.0 * 60.0 / beat.bpm.max(1.0);
                let end = crate::timeline::bar_quantize(raw, &beat);
                (end.min(anchor).max(0.0), end.max(anchor + bar_s))
            }
            LoopDrag::Left => (self.snap_time(raw).clamp(0.0, b - min), b),
            LoopDrag::Right => (a, self.snap_time(raw).max(a + min)),
            LoopDrag::Body(grab) => {
                let start = self.snap_time(raw - grab).max(0.0);
                (start, start + (b - a))
            }
        };
        if self.loop_region == Some(next) {
            return false;
        }
        self.loop_region = Some(next);
        self.apply_loop();
        true
    }

    /// Seek the playhead to the time under `x` and start a scrub drag. The
    /// first few px of the axis are its left edge — the start when the
    /// view starts there — so the top of the timeline is a click, not a
    /// pixel-hunt under a fine grid (Alva, 2026-09-01: "I can't place
    /// the playhead at the very first bar, it physically won't let me").
    pub(crate) fn seek_to_x(&mut self, panel: &crate::timeline::Panel, x: f32) {
        let edge = x <= panel.axis.0 + 10.0 * self.scale();
        let raw = if edge {
            self.time_view.t0
        } else {
            self.time_view.t_at(x, panel.axis)
        };
        // The edge is the start itself, not the nearest grid line — a
        // snap could round it just left of the view, and a playhead
        // there is a playhead you can't see.
        let t = if edge { raw } else { self.snap_time(raw) }.max(0.0);
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
    /// what was clicked; on the arrangement, a row or clip with a file
    /// behind it opens on that file (relink), anything else the
    /// timeline's own menu — the grid, and the loop's clearing.
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
            let scale = self.scale();
            let panel = crate::timeline::panel(layout.timeline, scale);
            let target = match self.source_at(&panel, scale, cx, cy) {
                Some(src) => crate::context::Target::Source(src),
                None => crate::context::Target::Timeline,
            };
            self.context_open([cx, cy], target);
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
    /// stops it — and nothing does. The timeline has no end to reach.
    #[test]
    fn a_free_clock_runs_and_keeps_running() {
        assert_eq!(tick(4.0, None), Tick::Run(4.0));
        assert_eq!(tick(119.9, None), Tick::Run(119.9));
        assert_eq!(tick(120.0, None), Tick::Run(120.0));
        assert_eq!(tick(5000.0, None), Tick::Run(5000.0));
    }

    /// The loop wraps by its own length, so the overshoot from a long frame
    /// is carried across rather than thrown away. Snapping to the region's
    /// start would walk the playhead off the beat every time a frame ran
    /// long — precisely what a loop is used to check.
    #[test]
    fn the_loop_wraps_by_its_length_not_to_its_start() {
        let cycle = Some((8.0, 12.0));
        assert_eq!(tick(11.9, cycle), Tick::Run(11.9));
        // Half a second past the end comes back half a second past the start.
        assert_eq!(tick(12.5, cycle), Tick::Wrap(8.5));
        // Even a frame that overshoots by more than the whole region.
        assert_eq!(tick(21.0, cycle), Tick::Wrap(9.0));
        // Exactly on the end wraps to exactly the start.
        assert_eq!(tick(12.0, cycle), Tick::Wrap(8.0));
    }

    /// A playhead parked before the region plays *into* it, matching what
    /// the audio player's callback does with a cursor outside the loop.
    #[test]
    fn a_playhead_before_the_loop_plays_into_it() {
        assert_eq!(tick(3.0, Some((8.0, 12.0))), Tick::Run(3.0));
    }

    /// A degenerate region is not a loop, and must not divide by zero.
    #[test]
    fn an_empty_loop_region_is_ignored() {
        assert_eq!(tick(30.0, Some((9.0, 9.0))), Tick::Run(30.0));
        assert_eq!(tick(30.0, Some((12.0, 8.0))), Tick::Run(30.0));
    }
}
