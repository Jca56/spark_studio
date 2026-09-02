//! The studio's half of the arrangement: the paired scene builder and
//! the press dispatch — sidebar rows select objects, eyes toggle,
//! folders collapse, volume boxes grab, clip bars grab (Shift adds to
//! the selection; a drag carries the whole selection — see `group`).

use super::{
    ArrHit, ArrangeScene, ClipDrag, ClipRef, RowDrag, RowDragView, RowKind, build, drop_dest,
    drop_slot, head_px, hit,
};
use crate::timeline::Panel;

/// Cursor travel before a press on a row head becomes a drag, logical px.
const ROW_DRAG_START: f32 = 6.0;

impl crate::Studio {
    /// The arrangement's layout, for hit-testing and drawing alike — the
    /// paired builder every panel needs.
    pub(crate) fn arrange_scene(
        &self,
        panel: &crate::timeline::Panel,
        scale: f32,
    ) -> ArrangeScene {
        build(
            panel,
            &self.time_view,
            scale,
            &self.editor,
            &self.subcomps,
            &self.selected_clips,
            self.lanes_scroll,
            &self.audio_tracks(),
            &self.mesh_missing,
            self.row_drag_view(panel, scale),
        )
    }

    /// The row drag as the frame draws it, once it has travelled: the
    /// row's offset and the slot the gold line marks.
    pub(crate) fn row_drag_view(&self, panel: &Panel, scale: f32) -> Option<RowDragView> {
        let d = self.row_drag.filter(|d| d.moved)?;
        let n_top = super::object_rows(&self.editor).len();
        let head = head_px(&self.audio_tracks(), scale);
        let my = self.cursor_px.1 as f32;
        Some(RowDragView {
            kind: d.kind,
            dy: d.dy,
            slot: drop_slot(panel, scale, self.lanes_scroll, my, n_top, head),
        })
    }

    /// The cursor moved with a row head held: the row follows it once it
    /// has travelled. True when the frame needs redrawing.
    pub(crate) fn arrange_row_moved(&mut self, my: f32) -> bool {
        let start = ROW_DRAG_START * self.scale();
        let Some(d) = self.row_drag.as_mut() else {
            return false;
        };
        d.dy = my - d.from_y;
        if d.dy.abs() >= start {
            d.moved = true;
        }
        d.moved
    }

    /// The button came up with a clip held. A drag that travelled has
    /// already moved or trimmed it; a press that never travelled was a
    /// click, and a click in a clip puts the playhead there — Ableton's
    /// own rule, and the answer to "let me scrub anywhere on the grid"
    /// when long clips leave no air to click in. True when a clip was
    /// held.
    pub(crate) fn arrange_clip_release(&mut self, cx: f32) -> bool {
        let Some(d) = self.clip_drag.take() else {
            return false;
        };
        if d.moved {
            return true;
        }
        let Some(layout) = self.layout() else {
            return true;
        };
        let panel = crate::timeline::panel(layout.timeline, self.scale());
        let t = self.snap_time(self.time_view.t_at(cx, panel.axis)).max(0.0);
        self.seek(t);
        self.request_redraw();
        true
    }

    /// The button came up with a row held: a drag that travelled lands
    /// the row (or the folder's whole run) at the gold line — one undo
    /// step. A press that never travelled was the click it already was.
    /// True when a row was held.
    pub(crate) fn arrange_row_release(&mut self) -> bool {
        let Some(d) = self.row_drag.take() else {
            return false;
        };
        if !d.moved {
            return true;
        }
        let Some(layout) = self.layout() else {
            return true;
        };
        let scale = self.scale();
        let panel = crate::timeline::panel(layout.timeline, scale);
        let n_top = super::object_rows(&self.editor).len();
        let head = head_px(&self.audio_tracks(), scale);
        let my = self.cursor_px.1 as f32;
        let slot = drop_slot(&panel, scale, self.lanes_scroll, my, n_top, head);
        let dest = drop_dest(&self.editor, slot);
        let n = self.editor.shapes().len();
        let moved = match d.kind {
            RowKind::Object(from) => {
                // `dest` is where it sits before the move; after the
                // removal everything past `from` shifts down one.
                let to = if dest > from { dest - 1 } else { dest };
                self.editor.move_layer(from, to)
            }
            RowKind::Folder(id) => {
                let hi = self
                    .editor
                    .folder_members(id)
                    .last()
                    .copied()
                    .unwrap_or(0);
                let target = if dest > hi { dest - 1 } else { dest };
                n > 0 && self.editor.move_folder(id, target.min(n - 1))
            }
            _ => false,
        };
        if moved {
            self.request_redraw();
        }
        true
    }

    /// A press on the arrangement: sidebar rows select objects (their
    /// track is the outliner), eyes toggle, folders collapse, a volume
    /// box starts its drag; clip bars grab (body moves, edges trim;
    /// Shift toggles the clip in the selection, and a drag carries the
    /// whole selection); double-click opens a comp clip's comp or an
    /// object clip's curve view. Returns whether the press was consumed
    /// — empty air falls through to the scrub.
    pub(crate) fn arrange_press(&mut self, panel: &Panel, scale: f32, cx: f32, cy: f32) -> bool {
        let over_lanes = panel.lanes.contains(cx, cy);
        let over_names = panel.names_box.contains(cx, cy);
        if !over_lanes && !over_names {
            return false;
        }
        let sc = self.arrange_scene(panel, scale);
        let Some(hit) = hit(&sc, cx, cy, scale) else {
            if !self.selected_clips.is_empty() {
                self.selected_clips.clear();
                self.request_redraw();
            }
            // Empty arrangement air scrubs — the caller's fallthrough.
            return false;
        };
        match hit {
            ArrHit::Disclose(id) => {
                if self.editor.toggle_folder_collapsed(id) {
                    self.request_redraw();
                }
            }
            ArrHit::Eye(RowKind::Object(i)) => {
                if self.editor.toggle_hidden(i) {
                    self.request_redraw();
                }
            }
            ArrHit::Eye(RowKind::Folder(id)) => {
                if self.editor.toggle_folder_hidden(id) {
                    self.request_redraw();
                }
            }
            ArrHit::Eye(_) => {}
            ArrHit::Volume(asset) => {
                self.volume_press(asset, cy);
                self.request_redraw();
            }
            ArrHit::Head(RowKind::Object(i)) => {
                if self.editor.select(Some(i)) {
                    self.request_redraw();
                }
                // A second click on the name opens the object's clip view —
                // the clip under the playhead, else its first (Alva,
                // 2026-09-01: "I've clicked the name first every single
                // time, then the clip itself").
                let obj = self.editor.shape_id(i);
                let now = std::time::Instant::now();
                let double = self
                    .last_head_click
                    .take()
                    .is_some_and(|(o, t0)| o == obj && now.duration_since(t0).as_millis() < 400);
                if double {
                    let c = self
                        .editor
                        .clip_at(i, self.editor.time())
                        .or_else(|| (!self.editor.obj_clips(i).is_empty()).then_some(0));
                    match c {
                        Some(c) => self.open_clip_view(obj, c),
                        None => self.export_note = Some("no clip to open".to_string()),
                    }
                    self.request_redraw();
                    return true;
                }
                self.last_head_click = Some((obj, now));
                // Held, the row can be dragged to a new place in the list.
                self.row_drag = Some(RowDrag {
                    kind: RowKind::Object(i),
                    from_y: cy,
                    dy: 0.0,
                    moved: false,
                });
            }
            ArrHit::Head(RowKind::Folder(id)) => {
                if self.editor.select_folder(id) {
                    self.request_redraw();
                }
                self.row_drag = Some(RowDrag {
                    kind: RowKind::Folder(id),
                    from_y: cy,
                    dy: 0.0,
                    moved: false,
                });
            }
            ArrHit::Head(_) => {}
            ArrHit::Clip(r, zone) => self.clip_press(panel, r, zone, cx),
        }
        true
    }

    /// A press on a clip bar: selection, the double-click that opens
    /// it, and the drag that may follow.
    fn clip_press(&mut self, panel: &Panel, r: ClipRef, zone: super::Zone, cx: f32) {
        // A second click on the same clip opens it: a comp clip's comp,
        // an object clip's curve view. Audio has nothing to open yet.
        let now = std::time::Instant::now();
        let double = self
            .last_clip_click
            .take()
            .is_some_and(|(pr, t0)| pr == r && now.duration_since(t0).as_millis() < 400);
        if double {
            match r {
                ClipRef::Comp(i) => {
                    self.open_clip_comp(i);
                    return;
                }
                ClipRef::Obj { obj, c } => {
                    self.open_clip_view(obj, c);
                    self.request_redraw();
                    return;
                }
                ClipRef::Audio(_) => {}
            }
        }
        self.last_clip_click = Some((r, now));
        if self.modifiers.shift_key() {
            // Shift toggles membership; a clip just dropped from the
            // selection isn't grabbed.
            match self.selected_clips.iter().position(|s| *s == r) {
                Some(at) => {
                    self.selected_clips.remove(at);
                    self.request_redraw();
                    return;
                }
                None => self.selected_clips.push(r),
            }
        } else if let Some(at) = self.selected_clips.iter().position(|s| *s == r) {
            // Already selected: the press grabs the whole selection, and
            // this clip becomes the primary.
            self.selected_clips.remove(at);
            self.selected_clips.push(r);
        } else {
            self.selected_clips = vec![r];
        }
        // Selecting a clip selects its object too — the track, the
        // canvas ants and the inspector agree on the thing.
        if let ClipRef::Obj { obj, .. } = r
            && let Some(i) = self.editor.index_of(obj)
            && !self.editor.selection().contains(&i)
        {
            self.editor.select(Some(i));
        }
        // The project's audio is read-only from inside a placed comp.
        let grabbable = !(matches!(r, ClipRef::Audio(_)) && self.in_comp());
        if let Some((s, _)) = self.clip_span_of(r).filter(|_| grabbable) {
            let t = self.time_view.t_at(cx, panel.axis);
            let group: Vec<(ClipRef, f32)> = self
                .selected_clips
                .iter()
                .filter(|g| !(matches!(g, ClipRef::Audio(_)) && self.in_comp()))
                .filter_map(|g| self.clip_span_of(*g).map(|(gs, _)| (*g, gs)))
                .collect();
            self.clip_drag = Some(ClipDrag {
                r,
                zone,
                grab_dt: t - s,
                press_x: cx,
                moved: false,
                group,
                orig: s,
            });
        }
        self.request_redraw();
    }
}
