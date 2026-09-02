//! Cursor-move dispatch: every in-progress drag (canvas, handles, the
//! timeline scrub, the loop brace; clip moves live in `arrange::group`)
//! plus hover tracking. Split from main
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
                    HandleDrag::End(k) => {
                        dirty |= self.editor.drag_line_end(*k, cur);
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
                let panel = timeline::panel(layout.timeline, self.scale());
                if self.timeline_scrub {
                    if self.clip_view.is_some() {
                        // The clip's ruler: local time, through the clip.
                        self.clip_scrub_x(&panel, mx);
                    } else {
                        // Nothing scrubs left of the start (behind the
                        // sidebar); to the right there is no end.
                        let t = self
                            .snap_time(self.time_view.t_at(mx, panel.axis))
                            .max(0.0);
                        self.seek(t);
                    }
                    dirty = true;
                }
                // The clip view: a held diamond follows the cursor;
                // otherwise what's under it lights.
                if self.clip_view_moved(&panel, mx, my) {
                    dirty = true;
                }
                // A held track row rides the cursor up or down the list.
                if self.arrange_row_moved(my) {
                    dirty = true;
                }
                // A held volume box: up raises.
                if self.volume_moved(my) {
                    dirty = true;
                }
                // The loop brace: a fresh region growing, an edge, or
                // the whole thing sliding.
                if self.loop_moved(&panel, mx) {
                    dirty = true;
                }
                // A held clip: once the cursor has travelled, the body
                // moves the selection or an edge trims (see `arrange::group`).
                if self.clip_drag_moved(&panel, mx) {
                    dirty = true;
                }
            }
        }
        let hover = self.title_bar().and_then(|tb| tb.hit(mx, my));
        if hover != self.title_hover {
            self.title_hover = hover;
            dirty = true;
        }
        // The context menu: a held knob or picker follows the cursor;
        // otherwise what's under it lights. The same geometry the frame
        // draws, so nothing lights where it isn't clickable.
        if self.context_moved(mx, my) {
            dirty = true;
        }
        // The inspector: a scrub, a slider or the picker follows the
        // cursor; otherwise what's under it lights.
        if let Some(layout) = self.layout()
            && self.inspector_moved(layout.right, mx, my)
        {
            dirty = true;
        }
        // The left panel: a held effect row's ghost follows the cursor;
        // otherwise what's under it lights.
        if let Some(layout) = self.layout()
            && self.left_moved(layout.left, mx, my)
        {
            dirty = true;
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
