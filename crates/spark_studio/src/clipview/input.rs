//! The clip view's gestures: the press, the right press, the drags,
//! the release, the wheel and Delete — everything the hand does to the
//! view. Split from `mod` so the state and the frame stay readable; the
//! same page the frame draws is what every gesture hit-tests.

use std::time::Instant;

use super::{Drag, DragKind, Hit, Sel, content_span, keyed, target_label};
use crate::Studio;
use crate::timeline::Panel;

/// A second press this soon and this near the first is a double-click.
const DOUBLE_MS: u128 = 400;
const DOUBLE_PX: f32 = 8.0;

impl Studio {
    /// A left press on the bottom panel while the view is open. The
    /// loop brace's end starts its drag; the rest of the ruler scrubs;
    /// the breadcrumb goes back; a row shows its curve; a diamond picks
    /// its key (or moment) and starts a drag; a double-click on the
    /// graph adds a key on the line; the air drops the pick. True
    /// whenever the press was on the panel — nothing falls through to
    /// the arrangement underneath.
    pub(crate) fn clip_view_press(&mut self, panel: &Panel, scale: f32, cx: f32, cy: f32) -> bool {
        if self.clip_view.is_none() {
            return false;
        }
        let on_panel = panel.ruler.contains(cx, cy)
            || panel.lanes.contains(cx, cy)
            || panel.names_box.contains(cx, cy)
            || panel.gutter.contains(cx, cy);
        if !on_panel {
            return false;
        }
        let Some(page) = self.clip_view_page(panel, scale) else {
            return true;
        };
        let hit = page.hit(cx, cy);
        if hit == Some(Hit::LoopEnd) {
            if let Some(cv) = self.clip_view.as_mut() {
                cv.drag = Some(Drag {
                    kind: DragKind::Loop,
                    span: page.span,
                    grab_dt: 0.0,
                    grab_dv: 0.0,
                    moved: false,
                });
            }
            self.request_redraw();
            return true;
        }
        if panel.ruler.contains(cx, cy) {
            self.clip_scrub_x(panel, cx);
            self.timeline_scrub = true;
            self.request_redraw();
            return true;
        }
        let now = Instant::now();
        let (double, t_cursor, c) = {
            let cv = self.clip_view.as_ref().expect("open");
            let double = cv.last_press.is_some_and(|(t0, p)| {
                now.duration_since(t0).as_millis() < DOUBLE_MS
                    && (p[0] - cx).abs() < DOUBLE_PX * scale
                    && (p[1] - cy).abs() < DOUBLE_PX * scale
            });
            (double, cv.view.t_at(cx, panel.axis), cv.c)
        };
        let i = self.clip_view_clip().map(|(i, _)| i);
        match hit {
            Some(Hit::Back) => {
                self.close_clip_view();
                self.request_redraw();
                return true;
            }
            Some(Hit::Row(k)) => {
                if let (Some(row), Some(cv)) = (page.rows.get(k), self.clip_view.as_mut()) {
                    cv.target = Some(row.target);
                    cv.sel = None;
                }
            }
            Some(Hit::Key(k)) => {
                let v_cursor = page.value_at(cy);
                if let (Some(d), Some(cv)) = (page.keys.get(k), self.clip_view.as_mut()) {
                    cv.sel = Some(Sel::Key {
                        target: d.target,
                        k: d.k,
                    });
                    cv.drag = Some(Drag {
                        kind: DragKind::Key {
                            target: d.target,
                            k: d.k,
                        },
                        span: page.span,
                        grab_dt: t_cursor - d.t,
                        grab_dv: v_cursor - d.v,
                        moved: false,
                    });
                }
            }
            Some(Hit::StripKey(k)) => {
                if let (Some(d), Some(cv)) = (page.strip_dots.get(k), self.clip_view.as_mut()) {
                    cv.sel = Some(Sel::Time(d.t));
                    cv.drag = Some(Drag {
                        kind: DragKind::Time { t: d.t },
                        span: page.span,
                        grab_dt: t_cursor - d.t,
                        grab_dv: 0.0,
                        moved: false,
                    });
                }
            }
            Some(Hit::Graph) => {
                let added = match (double, page.target, i) {
                    (true, Some(target), Some(i)) => {
                        let t = self.snap_local(t_cursor).max(0.0);
                        self.editor
                            .add_key(i, c, target, t)
                            .map(|k| Sel::Key { target, k })
                    }
                    _ => None,
                };
                if let Some(cv) = self.clip_view.as_mut() {
                    cv.sel = added;
                }
            }
            Some(Hit::Strip) | Some(Hit::LoopEnd) | None => {
                if let Some(cv) = self.clip_view.as_mut() {
                    cv.sel = None;
                }
            }
        }
        if let Some(cv) = self.clip_view.as_mut() {
            cv.last_press = Some((now, [cx, cy]));
        }
        self.request_redraw();
        true
    }

    /// A right press while the view is open: on a key, flips its ease
    /// between smooth and linear. True when the press was on the panel.
    pub(crate) fn clip_view_right_press(&mut self, cx: f32, cy: f32) -> bool {
        if self.clip_view.is_none() {
            return false;
        }
        let Some(layout) = self.layout() else {
            return false;
        };
        if !layout.timeline.contains(cx, cy) {
            return false;
        }
        let scale = self.scale();
        let panel = crate::timeline::panel(layout.timeline, scale);
        let Some(page) = self.clip_view_page(&panel, scale) else {
            return true;
        };
        if let Some(Hit::Key(k)) = page.hit(cx, cy)
            && let Some(d) = page.keys.get(k)
            && let Some(i) = self.clip_view_clip().map(|(i, _)| i)
        {
            let c = self.clip_view.as_ref().map(|cv| cv.c).unwrap_or(0);
            if self.editor.toggle_key_ease(i, c, d.target, d.k) {
                self.export_note = Some(format!(
                    "{} key is {}",
                    target_label(d.target, &self.editor.shapes()[i], self.editor.fx_of(i)),
                    if d.linear { "smooth" } else { "linear" }
                ));
                self.request_redraw();
            }
        }
        true
    }

    /// The cursor moved: a held diamond follows it — a key in time and
    /// value, a moment in time, the loop's end in time — otherwise what's
    /// under the cursor lights. True when the frame needs redrawing.
    pub(crate) fn clip_view_moved(&mut self, panel: &Panel, mx: f32, my: f32) -> bool {
        if self.clip_view.is_none() {
            return false;
        }
        let scale = self.scale();
        let Some(page) = self.clip_view_page(panel, scale) else {
            return false;
        };
        let (drag, t_cursor, c) = {
            let cv = self.clip_view.as_ref().expect("open");
            (cv.drag, cv.view.t_at(mx, panel.axis), cv.c)
        };
        let i = self.clip_view_clip().map(|(i, _)| i);
        if let (Some(d), Some(i)) = (drag, i) {
            let t = self.snap_local(t_cursor - d.grab_dt).max(0.0);
            let dirty = match d.kind {
                DragKind::Key { target, k } => {
                    let v = page.value_at(my) - d.grab_dv;
                    self.editor.move_key(i, c, target, k, t, v)
                }
                DragKind::Loop => {
                    // At least a beat; snap rides the playhead-snap toggle.
                    let beat = 60.0 / self.grid().bpm.max(1.0);
                    let len = self.snap_local(t_cursor).max(beat);
                    self.editor.set_obj_clip_loop_len(i, c, len)
                }
                DragKind::Time { t: from } => match self.editor.retime_keys_at(i, c, from, t) {
                    Some(landed) => {
                        if let Some(cv) = self.clip_view.as_mut() {
                            if let Some(dr) = &mut cv.drag {
                                dr.kind = DragKind::Time { t: landed };
                            }
                            cv.sel = Some(Sel::Time(landed));
                        }
                        (landed - from).abs() > 1e-6
                    }
                    None => false,
                },
            };
            if let Some(dr) = self.clip_view.as_mut().and_then(|cv| cv.drag.as_mut()) {
                dr.moved = true;
            }
            return dirty;
        }
        let on_panel = panel.lanes.contains(mx, my)
            || panel.names_box.contains(mx, my)
            || panel.ruler.contains(mx, my);
        let over = if on_panel { page.hit(mx, my) } else { None };
        let Some(cv) = self.clip_view.as_mut() else {
            return false;
        };
        let dirty = over != cv.over;
        cv.over = over;
        dirty
    }

    /// The button came up: a drag ends (the gesture's undo step closes
    /// with the release). True when one was running.
    pub(crate) fn clip_view_release(&mut self) -> bool {
        self.clip_view
            .as_mut()
            .and_then(|cv| cv.drag.take())
            .is_some()
    }

    /// The wheel over the bottom panel while the view is open: Ctrl
    /// zooms local time at the cursor, Shift (or a plain wheel over the
    /// axis) pans it, a plain wheel over the sidebar scrolls the rows.
    pub(crate) fn clip_view_wheel(&mut self, panel: &Panel, cx: f32, cy: f32, dy: f32) -> bool {
        let Some((_, clip)) = self.clip_view_clip() else {
            return false;
        };
        let span = content_span(clip, self.editor.bar_s);
        let scale = self.scale();
        let ctrl = self.modifiers.control_key();
        let shift = self.modifiers.shift_key();
        let over_names = panel.names_box.contains(cx, cy) || panel.gutter.contains(cx, cy);
        let Some(cv) = self.clip_view.as_mut() else {
            return false;
        };
        if ctrl {
            let pivot = cv.view.t_at(cx, panel.axis);
            cv.view.zoom((1.0f32 / 1.18).powf(dy), pivot, span);
        } else if shift || !over_names {
            let dt = -dy * cv.view.span() * 0.10;
            cv.view.pan(dt, span);
        } else {
            cv.scroll = (cv.scroll - dy * 60.0 * scale).max(0.0);
        }
        true
    }

    /// Delete while the view is open acts on the pick: the key, or every
    /// key at the moment. With nothing picked, an unkeyed row (a listing
    /// and nothing more) comes off the list; a keyed one says to pick a
    /// key. Never reaches past the view for the clip or the object. True
    /// when something changed.
    pub(crate) fn clip_view_delete(&mut self) -> bool {
        let Some(cv) = self.clip_view.as_ref() else {
            return false;
        };
        let (c, sel, shown) = (cv.c, cv.sel, cv.target);
        let Some((i, clip)) = self.clip_view_clip() else {
            return false;
        };
        let shown_unkeyed = shown.is_some_and(|t| !keyed(clip, t));
        let done = match sel {
            Some(Sel::Key { target, k }) => self.editor.delete_key(i, c, target, k),
            Some(Sel::Time(t)) => self.editor.delete_keys_at(i, c, t),
            None => match (shown, shown_unkeyed) {
                (Some(t), true) => {
                    if let Some(cv) = self.clip_view.as_mut() {
                        cv.armed.retain(|a| *a != t);
                        cv.target = None;
                    }
                    true
                }
                _ => {
                    self.export_note = Some("pick a key to delete it".to_string());
                    false
                }
            },
        };
        if done && let Some(cv) = self.clip_view.as_mut() {
            cv.sel = None;
        }
        done
    }
}
