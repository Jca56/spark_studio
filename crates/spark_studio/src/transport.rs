//! Transport control: the timeline's clock, play/pause, the loop region,
//! playhead-vs-keyframe keyboard moves (`,` `.` jump, arrow nudges),
//! scrubbing, and lane hit testing. Split from main so the event plumbing
//! stays readable.

use spark_audio::BeatGrid;

use crate::Studio;

/// Tempo a comp keeps time at before a track has been imported. Detection
/// replaces it the moment one lands, and the tempo field overrides both.
pub(crate) const SILENT_BPM: f32 = 120.0;

/// How long a comp is with no track to measure: two minutes, or sixty bars
/// at [`SILENT_BPM`]. Long enough to choreograph against, and the song
/// replaces it outright rather than being fitted into it.
pub(crate) const SILENT_DURATION: f32 = 120.0;

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

    pub(crate) fn toggle_play(&mut self) -> bool {
        match &self.player {
            Some(p) => {
                p.toggle();
                true
            }
            None => false,
        }
    }

    /// Push the loop region into the player (or clear it).
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

    /// `,` / `.`: jump the playhead to the previous/next keyframe of the
    /// selected key's shape (or the primary canvas selection).
    pub(crate) fn jump_key(&mut self, forward: bool) -> bool {
        let Some(i) = self
            .selected_keys
            .last()
            .map(|&(i, _)| i)
            .or_else(|| self.editor.primary().map(|i| self.editor.owner(i)))
        else {
            return false;
        };
        let Some(t) = self.editor.adjacent_key(i, self.editor.time(), forward) else {
            return false;
        };
        if let Some(p) = &self.player {
            p.seek(t);
        }
        self.editor.set_time(t);
        true
    }

    /// Left/Right arrows: slide the selected keyframes by a 16th note.
    pub(crate) fn nudge_key(&mut self, dir: f32) -> bool {
        if self.selected_keys.is_empty() {
            return false;
        }
        let (grid, duration) = (self.grid(), self.duration());
        let step = 60.0 / grid.bpm.max(1.0) / 4.0;
        let lo = self
            .selected_keys
            .iter()
            .map(|&(_, t)| t)
            .fold(f32::MAX, f32::min);
        let hi = self
            .selected_keys
            .iter()
            .map(|&(_, t)| t)
            .fold(f32::MIN, f32::max);
        // How far the whole set may slide before it leaves the track. A set
        // spanning wider than the track itself (keys left over from a longer
        // one) has no legal room at all — refuse rather than clamp, which
        // would panic on an inverted range.
        let (min_dt, max_dt) = (grid.first_bar - lo, duration - hi);
        if min_dt > max_dt {
            return false;
        }
        let dt = (dir * step).clamp(min_dt, max_dt).clamp(-step, step);
        let keys = self.selected_keys.clone();
        if self.editor.retime_group(&keys, dt) {
            for k in &mut self.selected_keys {
                k.1 += dt;
            }
            true
        } else {
            false
        }
    }

    /// Seek the playhead to the time under `x` and start a scrub drag.
    pub(crate) fn seek_to_x(&mut self, panel: &crate::timeline::Panel, x: f32) {
        let raw = self.time_view.t_at(x, panel.axis);
        let t = self
            .snap_time(raw)
            .clamp(self.grid().first_bar, self.duration());
        if let Some(p) = &self.player {
            p.seek(t);
        }
        self.editor.set_time(t);
        self.timeline_scrub = true;
        self.request_redraw();
    }

    /// Quarter-bar (one beat) quantization, while playhead snap is on.
    pub(crate) fn snap_time(&self, t: f32) -> f32 {
        if !self.snap_playhead {
            return t;
        }
        let grid = self.grid();
        let beat_s = 60.0 / grid.bpm.max(1.0);
        grid.first_bar + ((t - grid.first_bar) / beat_s).round() * beat_s
    }

    /// Whatever the cursor is over in the keyframe lanes (Keys tab only).
    pub(crate) fn lane_hit(&self, cx: f32, cy: f32) -> Option<crate::lanes::LaneHit> {
        if self.timeline_tab != crate::timeline::Tab::Keys {
            return None;
        }
        let layout = self.layout()?;
        let scale = self.scale();
        let panel = crate::timeline::panel(layout.timeline, scale);
        let rows = self.lane_rows(&panel, scale);
        crate::lanes::hit(&rows, &panel, scale, cx, cy)
    }

    pub(crate) fn lane_rows(
        &self,
        panel: &crate::timeline::Panel,
        scale: f32,
    ) -> Vec<crate::lanes::LaneRow> {
        crate::lanes::rows(
            panel,
            &self.time_view,
            scale,
            &self.editor,
            self.lane_open,
            self.lanes_scroll,
        )
    }

    /// Every key marker inside a physical-px rectangle (rubber band).
    pub(crate) fn keys_in_box(
        &self,
        x0: f32,
        y0: f32,
        x1: f32,
        y1: f32,
    ) -> Vec<(crate::anim::Owner, f32)> {
        let Some(layout) = self.layout() else {
            return Vec::new();
        };
        let scale = self.scale();
        let panel = crate::timeline::panel(layout.timeline, scale);
        let mut out = Vec::new();
        for lr in self.lane_rows(&panel, scale) {
            if lr.row.y + lr.row.h < y0 || lr.row.y > y1 {
                continue;
            }
            for &(t, x, _) in &lr.keys {
                if x >= x0 && x <= x1 {
                    out.push((lr.owner, t));
                }
            }
        }
        out
    }

    /// Right-click: delete the keyframe under the cursor, or clear the
    /// loop region from the ruler.
    pub(crate) fn right_press(&mut self) {
        let (cx, cy) = (self.cursor_px.0 as f32, self.cursor_px.1 as f32);
        // Right-click a folder header: dissolve it, leaving the layers.
        if let Some(layout) = self.layout() {
            let (_, _, cards) = self.right_panel(&layout);
            if let Some(f) = cards.folders.iter().find(|f| f.row.contains(cx, cy)) {
                let id = f.id;
                if self.editor.dissolve_folder(id) {
                    self.request_redraw();
                }
                return;
            }
        }
        if let Some(layout) = self.layout()
            && crate::timeline::panel(layout.timeline, self.scale())
                .ruler
                .contains(cx, cy)
        {
            if self.loop_region.take().is_some() {
                self.loop_on = false;
                self.apply_loop();
                println!("loop cleared");
                self.request_redraw();
            }
            return;
        }
        if let Some(crate::lanes::LaneHit::Key(i, t)) = self.lane_hit(cx, cy)
            && self.editor.delete_keys_at(i, t)
        {
            self.selected_keys
                .retain(|&(si, st)| !(si == i && (st - t).abs() < crate::anim::KEY_EPS));
            self.request_redraw();
        }
    }
}
