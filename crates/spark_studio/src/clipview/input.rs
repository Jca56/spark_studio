//! The clip view's gestures: the press, the right press, the drags,
//! the release, the wheel and Delete — everything the hand does to the
//! view. Split from `mod` so the state and the frame stay readable; the
//! same page the frame draws is what every gesture hit-tests.

use std::time::Instant;

use super::snap::{MAGNET_PX, snap_value};
use super::{Drag, DragKind, Hit, Sel, band_rect, beat_label, content_span, keyed};
use crate::Studio;
use crate::editor::KeySpan;
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
                let shift = self.modifiers.shift_key();
                if let (Some(d), Some(cv)) = (page.keys.get(k), self.clip_view.as_mut()) {
                    let this = (d.target, d.k);
                    if shift {
                        // Shift adds a key to the pick, or takes it out —
                        // no drag, the click is the whole gesture.
                        let mut set = cv.sel.as_ref().map(Sel::set).unwrap_or_default();
                        match set.iter().position(|e| *e == this) {
                            Some(at) => {
                                set.remove(at);
                            }
                            None => set.push(this),
                        }
                        cv.sel = (!set.is_empty()).then_some(Sel::Keys(set));
                    } else {
                        // A press on one of a picked set drags the set;
                        // anywhere else picks the one key and drags it.
                        let in_set = matches!(&cv.sel, Some(Sel::Keys(set)) if set.contains(&this));
                        let kind = if in_set {
                            DragKind::Set { anchor: this }
                        } else {
                            cv.sel = Some(Sel::Key {
                                target: d.target,
                                k: d.k,
                            });
                            DragKind::Key {
                                target: d.target,
                                k: d.k,
                            }
                        };
                        cv.drag = Some(Drag {
                            kind,
                            span: page.span,
                            grab_dt: t_cursor - d.t,
                            grab_dv: v_cursor - d.v,
                            moved: false,
                        });
                    }
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
            Some(Hit::Graph) if self.modifiers.shift_key() && !double => {
                // Shift-drag on the graph: a band, picking the keys it
                // covers as it grows.
                if let Some(cv) = self.clip_view.as_mut() {
                    cv.band = Some(([cx, cy], [cx, cy]));
                    cv.sel = None;
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
                let none = added.is_none();
                if let Some(cv) = self.clip_view.as_mut() {
                    cv.sel = added;
                }
                // The air is a scrub too — anywhere on the grid, through
                // the clip. A double-click's first press moves the
                // playhead to where the key then lands.
                if none {
                    self.clip_scrub_x(panel, cx);
                    self.timeline_scrub = true;
                }
            }
            Some(Hit::Strip) => {
                if let Some(cv) = self.clip_view.as_mut() {
                    cv.sel = None;
                }
                self.clip_scrub_x(panel, cx);
                self.timeline_scrub = true;
            }
            Some(Hit::LoopEnd) | None => {
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
        let (drag, t_cursor, c, band) = {
            let cv = self.clip_view.as_ref().expect("open");
            (cv.drag, cv.view.t_at(mx, panel.axis), cv.c, cv.band)
        };
        if let Some((from, _)) = band {
            let r = band_rect((from, [mx, my]));
            let picked = page.keys_in(r);
            if let Some(cv) = self.clip_view.as_mut() {
                cv.band = Some((from, [mx, my]));
                cv.sel = (!picked.is_empty()).then_some(Sel::Keys(picked));
            }
            return true;
        }
        let i = self.clip_view_clip().map(|(i, _)| i);
        if let (Some(d), Some(i)) = (drag, i) {
            // Ctrl frees the key from the grid on both axes.
            let free = self.modifiers.control_key();
            let t = if free {
                (t_cursor - d.grab_dt).max(0.0)
            } else {
                self.snap_local(t_cursor - d.grab_dt).max(0.0)
            };
            let dirty = match d.kind {
                DragKind::Key { target, k } => {
                    let raw = page.value_at(my) - d.grab_dv;
                    let v = if self.snap_playhead && !free {
                        // Magnets: the setting's floor, ceiling and zero,
                        // and every other key's value on this curve.
                        let mut magnets = page.magnets.clone();
                        magnets.extend(page.keys.iter().filter(|d| d.k != k).map(|d| d.v));
                        snap_value(raw, page.step, &magnets, page.px_per_unit(), MAGNET_PX * scale)
                    } else {
                        raw
                    };
                    self.editor.move_key(i, c, target, k, t, v)
                }
                DragKind::Set { anchor } => {
                    // The anchor goes where a lone key would; the rest of
                    // the set follows by the same offsets.
                    let set = self
                        .clip_view
                        .as_ref()
                        .and_then(|cv| cv.sel.as_ref())
                        .map(Sel::set)
                        .unwrap_or_default();
                    match page.keys.iter().find(|d| (d.target, d.k) == anchor) {
                        Some(a) => {
                            let raw = page.value_at(my) - d.grab_dv;
                            let v = if self.snap_playhead && !free {
                                let mut magnets = page.magnets.clone();
                                magnets.extend(
                                    page.keys
                                        .iter()
                                        .filter(|d| !set.contains(&(d.target, d.k)))
                                        .map(|d| d.v),
                                );
                                snap_value(raw, page.step, &magnets, page.px_per_unit(), MAGNET_PX * scale)
                            } else {
                                raw
                            };
                            self.editor.shift_keys(i, c, &set, t - a.t, v - a.v)
                        }
                        None => false,
                    }
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
        let Some(cv) = self.clip_view.as_mut() else {
            return false;
        };
        let band = cv.band.take().is_some();
        cv.drag.take().is_some() || band
    }

    /// Ctrl+A while the view is open: every key on the shown curve.
    pub(crate) fn clip_view_select_all(&mut self) -> bool {
        let Some(cv) = self.clip_view.as_ref() else {
            return false;
        };
        let Some(target) = cv.target else {
            return false;
        };
        let Some((_, clip)) = self.clip_view_clip() else {
            return false;
        };
        let n = clip.anim.track(target).map_or(0, |tr| tr.keys.len());
        let set: Vec<(crate::anim::Target, usize)> = (0..n).map(|k| (target, k)).collect();
        if let Some(cv) = self.clip_view.as_mut() {
            cv.sel = (!set.is_empty()).then_some(Sel::Keys(set));
        }
        true
    }

    /// Ctrl+X: the copy, then the delete — what Ctrl+C would take is
    /// what goes.
    pub(crate) fn clip_view_cut(&mut self) -> bool {
        let copied = self
            .clip_view
            .as_ref()
            .and_then(|cv| self.editor.index_of(cv.obj).map(|i| (i, cv.c, cv.sel.clone())));
        let Some((i, c, sel)) = copied else {
            return false;
        };
        let span = match &sel {
            Some(Sel::Key { target, k }) => KeySpan::Key(*target, *k),
            Some(Sel::Time(t)) => KeySpan::Moment(*t),
            Some(Sel::Keys(set)) => KeySpan::Set(set.clone()),
            None => KeySpan::Clip,
        };
        let n = self.editor.copy_keys(i, c, span);
        if n == 0 {
            self.export_note = Some("nothing to cut — no keys here".to_string());
            return true;
        }
        let gone = match sel {
            Some(Sel::Key { target, k }) => self.editor.delete_key(i, c, target, k),
            Some(Sel::Time(t)) => self.editor.delete_keys_at(i, c, t),
            Some(Sel::Keys(set)) => self.editor.delete_keys(i, c, &set),
            None => self.editor.clear_keys(i, c),
        };
        if gone && let Some(cv) = self.clip_view.as_mut() {
            cv.sel = None;
        }
        self.export_note = Some(match n {
            1 => "cut 1 key".to_string(),
            n => format!("cut {n} keys"),
        });
        true
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

    /// Ctrl+C while the view is open takes keys: the picked one, every
    /// key at the picked moment, or — nothing picked — the whole clip's.
    /// Always true: the strip says what was taken.
    pub(crate) fn clip_view_copy(&mut self) -> bool {
        let Some(cv) = self.clip_view.as_ref() else {
            return false;
        };
        let (c, sel) = (cv.c, cv.sel.clone());
        let Some((i, _)) = self.clip_view_clip() else {
            return false;
        };
        let span = match sel {
            Some(Sel::Key { target, k }) => KeySpan::Key(target, k),
            Some(Sel::Time(t)) => KeySpan::Moment(t),
            Some(Sel::Keys(set)) => KeySpan::Set(set),
            None => KeySpan::Clip,
        };
        let n = self.editor.copy_keys(i, c, span);
        self.export_note = Some(match n {
            0 => "nothing to copy — no keys here".to_string(),
            1 => "copied 1 key".to_string(),
            n => format!("copied {n} keys"),
        });
        true
    }

    /// Ctrl+V while the view is open: the copied keys land with their
    /// first on the playhead, in clip-local time — this clip's or any
    /// other object's. With the playhead outside the clip there is
    /// nowhere to land, and the strip says so. Always true.
    pub(crate) fn clip_view_paste(&mut self) -> bool {
        self.clip_view_paste_at(None, None)
    }

    /// The paste, in full: at local time `at` (the playhead's when
    /// `None`), onto `row`'s setting when one is named and the copy is
    /// of one setting (the menu's paste on a row), else by setting.
    pub(crate) fn clip_view_paste_at(&mut self, at: Option<f32>, row: Option<crate::anim::Target>) -> bool {
        let Some(cv) = self.clip_view.as_ref() else {
            return false;
        };
        let c = cv.c;
        let Some((i, clip)) = self.clip_view_clip() else {
            return false;
        };
        if self.editor.key_clip().is_none_or(|k| k.is_empty()) {
            self.export_note = Some("nothing copied yet — Ctrl+C takes keys".to_string());
            return true;
        }
        let at = match at {
            Some(at) => at,
            None => {
                let t = self.editor.time();
                if !clip.contains(t) {
                    self.export_note = Some("move the playhead into the clip to paste".to_string());
                    return true;
                }
                clip.local(t)
            }
        };
        let n = match row {
            Some(target) => self.editor.paste_keys_onto(i, c, at, target),
            None => self.editor.paste_keys(i, c, at),
        };
        self.export_note = Some(match n {
            0 => "nothing here to paste onto — the copied settings aren't on this object"
                .to_string(),
            1 => format!("pasted 1 key at {}", beat_label(at, self.grid().bpm)),
            n => format!("pasted {n} keys at {}", beat_label(at, self.grid().bpm)),
        });
        if n > 0 && let Some(cv) = self.clip_view.as_mut() {
            cv.sel = Some(Sel::Time(at));
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
        let (c, sel, shown) = (cv.c, cv.sel.clone(), cv.target);
        let Some((i, clip)) = self.clip_view_clip() else {
            return false;
        };
        let shown_unkeyed = shown.is_some_and(|t| !keyed(clip, t));
        let done = match sel {
            Some(Sel::Key { target, k }) => self.editor.delete_key(i, c, target, k),
            Some(Sel::Time(t)) => self.editor.delete_keys_at(i, c, t),
            Some(Sel::Keys(set)) => self.editor.delete_keys(i, c, &set),
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
