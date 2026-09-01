//! Cursor-move dispatch: every in-progress drag (canvas, handles, clip
//! moves and trims, timeline scrub) plus hover tracking. Split from main
//! so the event plumbing stays readable.

use crate::editor::Prop;
use crate::{HandleDrag, Studio, timeline};

impl Studio {
    pub(crate) fn cursor_moved(&mut self, px: f64, py: f64) {
        self.cursor_px = (px, py);
        let (mx, my) = (px as f32, py as f32);
        let mut dirty = false;
        if self.panel_resize
            && let Some(gpu) = &self.gpu
        {
            // The grab bar is the toolbar's top edge — toolbar and timeline
            // resize as one block, so the toolbar's height comes off the
            // cursor position before clamping; the viewport re-fits above.
            // The status strip sits below the timeline, so its height is
            // part of the distance to the window edge and not part of the
            // timeline.
            let (_, h) = gpu.size();
            let scale = self.scale();
            let tb_h = self.layout().map_or(0.0, |l| l.toolbar.h) / scale;
            let below = tb_h + spark_ui::Layout::STATUS_H;
            self.timeline_h =
                spark_ui::Layout::clamp_timeline_h(h, scale, (h as f32 - my) / scale - below);
            dirty = true;
        }
        if let Some(layout) = self.layout() {
            if let Some((ax, ay)) = self.canvas_pan {
                let (dx, dy) = ((px - ax) as f32, (py - ay) as f32);
                // Right- or middle-drag: in the fly view it pans the eye;
                // in the comp viewer it pans the canvas.
                if !self.pan_drag(dx, dy) {
                    self.canvas_view
                        .pan_px(dx, dy, layout.viewport, self.editor.canvas());
                }
                self.canvas_pan = Some((px, py));
                dirty = true;
            }
            if self.look_moved(px, py) {
                dirty = true;
            }
            if let Some(c) = self.cursor_canvas(px, py, &layout) {
                dirty |= self.editor.set_cursor_canvas(c);
            }
            if let Some(res) = self.gpu.as_ref().map(|g| g.size()) {
                let (camera, framing) = (self.camera(), self.framing(&layout));
                if let Some(gd) = &mut self.gizmo_drag {
                    dirty |= gd.update(&mut self.editor, &camera, &framing, res, [mx, my]);
                } else if self.handle_drag.is_none() {
                    // Light the part under the cursor.
                    let over = self
                        .gizmo(&layout)
                        .and_then(|g| g.hit(&camera, &framing, res, [mx, my]));
                    if over != self.gizmo_hover {
                        self.gizmo_hover = over;
                        dirty = true;
                    }
                }
            }
            if let Some(hd) = &mut self.handle_drag {
                let cur = self.editor.cursor();
                let group = self.editor.selection().len() > 1;
                match hd {
                    HandleDrag::Scale { center, ref_dist } => {
                        let d =
                            ((cur[0] - center[0]).powi(2) + (cur[1] - center[1]).powi(2)).sqrt();
                        if d > 0.5 && *ref_dist > 0.5 {
                            let f = (d / *ref_dist).clamp(0.5, 2.0);
                            let around = group.then_some(*center);
                            self.editor.scale_selection(f, around);
                            *ref_dist = d;
                            dirty = true;
                        }
                    }
                    HandleDrag::Width => {
                        if let Some(p) = self.editor.selected_props() {
                            let (sn, cs) = p.rotation.sin_cos();
                            let proj = ((cur[0] - p.x) * cs + (cur[1] - p.y) * sn).abs();
                            dirty |= self.editor.set_prop(Prop::Width, (proj * 2.0).max(6.0));
                        }
                    }
                    HandleDrag::Height => {
                        if let Some(p) = self.editor.selected_props() {
                            let (sn, cs) = p.rotation.sin_cos();
                            let proj = (-(cur[0] - p.x) * sn + (cur[1] - p.y) * cs).abs();
                            dirty |= self.editor.set_prop(Prop::Height, (proj * 2.0).max(6.0));
                        }
                    }
                    HandleDrag::Vertex(k) => {
                        dirty |= self.editor.drag_vertex(*k, cur);
                    }
                    HandleDrag::Rotate { center, prev } => {
                        let ang = (cur[1] - center[1]).atan2(cur[0] - center[0]);
                        let mut delta = ang - *prev;
                        while delta > std::f32::consts::PI {
                            delta -= std::f32::consts::TAU;
                        }
                        while delta < -std::f32::consts::PI {
                            delta += std::f32::consts::TAU;
                        }
                        let around = group.then_some(*center);
                        dirty |= self.editor.rotate_selection(delta, around);
                        *prev = ang;
                    }
                }
            }
            {
                let (beat, duration) = (self.grid(), self.duration());
                let panel = timeline::panel(layout.timeline, self.scale());
                if self.timeline_scrub {
                    // The choreography clock starts at bar 1 — nothing
                    // scrubs or lands left of it (behind the sidebar).
                    let t = self
                        .snap_time(self.time_view.t_at(mx, panel.axis))
                        .clamp(beat.first_bar, duration);
                    self.seek(t);
                    dirty = true;
                }
                if let Some(anchor) = self.loop_drag {
                    // Grow the loop by whole bars around the anchor bar.
                    let bar_s = 4.0 * 60.0 / beat.bpm.max(1.0);
                    let end = timeline::bar_quantize(self.time_view.t_at(mx, panel.axis), &beat);
                    let a = end.min(anchor).max(beat.first_bar);
                    let b = (end.max(anchor + bar_s)).min(duration);
                    if self.loop_region != Some((a, b)) {
                        self.loop_region = Some((a, b));
                        self.apply_loop();
                        dirty = true;
                    }
                }
                if let Some(d) = self.clip_drag {
                    // Body moves along the clip's own track; either edge
                    // trims. Snap rides the playhead-snap toggle.
                    let t_raw = self.time_view.t_at(mx, panel.axis);
                    match d.r {
                        crate::arrange::ClipRef::Obj { obj, c } => {
                            if let Some(i) = self.editor.index_of(obj)
                                && let Some(cl) = self.editor.obj_clips(i).get(c)
                            {
                                let (start, len) = match d.zone {
                                    crate::arrange::Zone::Move => {
                                        let s = self
                                            .snap_time(t_raw - d.grab_dt)
                                            .clamp(0.0, (duration - 0.05).max(0.0));
                                        (s, cl.len)
                                    }
                                    crate::arrange::Zone::Left => {
                                        let end = cl.end();
                                        let s = self.snap_time(t_raw).clamp(0.0, end - 0.05);
                                        (s, end - s)
                                    }
                                    crate::arrange::Zone::Right => {
                                        let end =
                                            self.snap_time(t_raw).clamp(cl.start + 0.05, duration);
                                        (cl.start, end - cl.start)
                                    }
                                };
                                if self.editor.set_obj_clip_span(i, c, start, len) {
                                    dirty = true;
                                }
                            }
                        }
                        crate::arrange::ClipRef::Comp(ci) => {
                            if let Some(&c) = self.editor.comp_clips().get(ci) {
                                let (start, len) = match d.zone {
                                    crate::arrange::Zone::Move => {
                                        let s = self
                                            .snap_time(t_raw - d.grab_dt)
                                            .clamp(0.0, (duration - 0.05).max(0.0));
                                        (s, c.len)
                                    }
                                    crate::arrange::Zone::Left => {
                                        let end = c.start + c.len;
                                        let s = self.snap_time(t_raw).clamp(0.0, end - 0.05);
                                        (s, end - s)
                                    }
                                    crate::arrange::Zone::Right => {
                                        let end =
                                            self.snap_time(t_raw).clamp(c.start + 0.05, duration);
                                        (c.start, end - c.start)
                                    }
                                };
                                if self.editor.set_clip_span(ci, c.track, start, len) {
                                    dirty = true;
                                }
                            }
                        }
                    }
                }
            }
        }
        let hover = self.title_bar().and_then(|tb| tb.hit(mx, my));
        if hover != self.title_hover {
            self.title_hover = hover;
            dirty = true;
        }
        if self.ctx_menu.is_some() {
            // Which context-rail button is under the cursor. The same
            // geometry the frame draws, so a button can never light where
            // it isn't clickable.
            let h = self
                .context()
                .and_then(|c| c.rail.iter().position(|(_, _, b)| b.contains(mx, my)));
            if h != self.ctx_hover {
                self.ctx_hover = h;
                dirty = true;
            }
        }
        if let Some(layout) = self.layout() {
            {
                let c = timeline::controls(layout.toolbar, self.scale());
                let zh = [c.zoom_minus, c.zoom_plus, c.zoom_pct]
                    .iter()
                    .position(|r| r.contains(mx, my))
                    .map(|i| i as u8);
                if zh != self.zoom_hover {
                    self.zoom_hover = zh;
                    dirty = true;
                }
            }
            // Row-resize cursor while over (or dragging) the resize bar —
            // the toolbar's top edge.
            let near = (my - layout.toolbar.y).abs() <= 6.0 * self.scale() || self.panel_resize;
            if near != self.resize_hover {
                self.resize_hover = near;
                if let Some(w) = &self.window {
                    w.set_cursor(if near {
                        winit::window::Cursor::Icon(winit::window::CursorIcon::RowResize)
                    } else {
                        // Back to the resting cursor — Spark or system,
                        // whichever the View toggle says.
                        self.base_cursor()
                    });
                }
            }
            {
                let c = timeline::controls(layout.toolbar, self.scale());
                let hover = c.play.contains(mx, my);
                if hover != self.transport_hover {
                    self.transport_hover = hover;
                    dirty = true;
                }
                let panel = timeline::panel(layout.timeline, self.scale());
                let key = panel.stamp.contains(mx, my);
                if key != self.key_hover {
                    self.key_hover = key;
                    dirty = true;
                }
            }
        }
        if let Some(menus) = self.menus() {
            let anchor_hover = menus.iter().position(|m| m.hit_anchor(mx, my));
            if anchor_hover != self.menu_anchor_hover {
                self.menu_anchor_hover = anchor_hover;
                dirty = true;
            }
            if let Some(mi) = self.menu_open {
                let hover = menus[mi].hit_item(mx, my);
                if hover != self.menu_hover {
                    self.menu_hover = hover;
                    dirty = true;
                }
            }
        }
        if dirty {
            self.request_redraw();
        }
    }
}
