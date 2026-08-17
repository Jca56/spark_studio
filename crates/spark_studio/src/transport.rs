//! Transport control: play/pause, the loop region, playhead-vs-keyframe
//! keyboard moves (`,` `.` jump, arrow nudges), scrubbing, and lane hit
//! testing. Split from main so the event plumbing stays readable.

use crate::Studio;

impl Studio {
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
            .or_else(|| self.editor.primary())
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
        let Some(track) = &self.audio else {
            return false;
        };
        if self.selected_keys.is_empty() {
            return false;
        }
        let step = 60.0 / track.beat.bpm.max(1.0) / 4.0;
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
        let (min_dt, max_dt) = (track.beat.first_bar - lo, track.duration - hi);
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
        let Some(track) = &self.audio else { return };
        let t = self
            .time_view
            .t_at(x, panel.axis)
            .clamp(track.beat.first_bar, track.duration);
        if let Some(p) = &self.player {
            p.seek(t);
        }
        self.editor.set_time(t);
        self.timeline_scrub = true;
        self.request_redraw();
    }

    /// Whatever the cursor is over in the keyframe lanes (Keys tab only).
    pub(crate) fn lane_hit(&self, cx: f32, cy: f32) -> Option<crate::lanes::LaneHit> {
        if self.timeline_tab != crate::timeline::Tab::Keys {
            return None;
        }
        let layout = self.layout()?;
        self.audio.as_ref()?;
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
            self.editor.shapes(),
            self.editor.names(),
            self.editor.anim(),
            self.editor.selection(),
            self.lanes_scroll,
        )
    }

    /// Every key marker inside a physical-px rectangle (rubber band).
    pub(crate) fn keys_in_box(&self, x0: f32, y0: f32, x1: f32, y1: f32) -> Vec<(usize, f32)> {
        let Some(layout) = self.layout() else {
            return Vec::new();
        };
        if self.audio.is_none() {
            return Vec::new();
        }
        let scale = self.scale();
        let panel = crate::timeline::panel(layout.timeline, scale);
        let mut out = Vec::new();
        for lr in self.lane_rows(&panel, scale) {
            if lr.row.y + lr.row.h < y0 || lr.row.y > y1 {
                continue;
            }
            for &(t, x, _) in &lr.keys {
                if x >= x0 && x <= x1 {
                    out.push((lr.index, t));
                }
            }
        }
        out
    }

    /// Right-click: delete the keyframe under the cursor, or clear the
    /// loop region from the ruler.
    pub(crate) fn right_press(&mut self) {
        let (cx, cy) = (self.cursor_px.0 as f32, self.cursor_px.1 as f32);
        if self.audio.is_some()
            && let Some(layout) = self.layout()
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
