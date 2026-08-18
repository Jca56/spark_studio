//! Cursor-move dispatch: every in-progress drag (canvas, sliders, picker,
//! handles, layer reorder, timeline scrub, key retime) plus hover tracking.
//! Split from main so the event plumbing stays readable.

use crate::editor::Prop;
use crate::{HandleDrag, PickerDrag, ScrubTarget, Studio, colorhome, lanes, timeline};

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
                self.canvas_view.pan_px(
                    (px - ax) as f32,
                    (py - ay) as f32,
                    layout.viewport,
                    self.scale(),
                );
                self.canvas_pan = Some((px, py));
                dirty = true;
            }
            dirty |= self.editor.set_cursor(px, py, self.canvas_map(&layout));
            if let Some(kind) = self.picker_drag {
                let (color_vp, _) = colorhome::split(
                    layout.right,
                    self.scale(),
                    true,
                    self.chrome_target().is_some(),
                );
                let home = self.color_home(color_vp);
                match kind {
                    // Alpha is not part of H/S/V — it rides the colour
                    // itself — so it writes straight through rather than
                    // through the hue/square path.
                    PickerDrag::Alpha => {
                        if let Some((track, _)) = home.alpha {
                            self.apply_alpha(spark_ui::Slider::t_at(track, mx));
                            dirty = true;
                        }
                    }
                    PickerDrag::Sv | PickerDrag::Hue => {
                        if let Some((p, _, _)) = &home.picker {
                            if let Some(hsv) = &mut self.picker_hsv {
                                match kind {
                                    PickerDrag::Sv => {
                                        let (sv, v) = p.sv_at(mx, my);
                                        hsv[1] = sv;
                                        hsv[2] = v;
                                    }
                                    _ => hsv[0] = p.hue_at(my),
                                }
                            }
                            self.apply_picker();
                            dirty = true;
                        }
                    }
                }
            }
            if self.material_drag.is_some() && self.drag_material(mx) {
                dirty = true;
            }
            if self.field_drag {
                let at = crate::textbox::index_at(&self.field_caret_xs, mx);
                if let Some((_, _, tb)) = &mut self.field_edit {
                    tb.drag_to(at);
                }
                dirty = true;
            }
            if let Some((i, id, param)) = self.fx_slider_drag {
                let (_, _, cards) = self.right_panel(&layout);
                let track = cards.rows.iter().find(|lr| lr.index == i).and_then(|lr| {
                    lr.detail.as_ref()?.fx.iter().find_map(|r| {
                        r.params
                            .iter()
                            .find(|p| p.id == id && p.param == param)
                            .map(|p| p.track)
                    })
                });
                match track {
                    Some(track) => {
                        let t = spark_ui::Slider::t_at(track, mx);
                        let v = self
                            .editor
                            .fx_of(i)
                            .find(id)
                            .and_then(|e| e.kind.params().get(param as usize))
                            .map(|s| s.min + t * (s.max - s.min));
                        if let Some(v) = v
                            && self.editor.set_effect_param(i, id, param, v)
                        {
                            dirty = true;
                        }
                    }
                    None => self.fx_slider_drag = None,
                }
            }
            if let Some((target, prop)) = self.slider_drag {
                // React sliders live in the Keys sidebar; everything else
                // is on the open layer card — or, for a fade, on the folder
                // header, which is a row in the same list.
                let track = if let crate::ScrubTarget::Folder(id) = target {
                    let (_, _, cards) = self.right_panel(&layout);
                    cards
                        .folders
                        .iter()
                        .find(|f| f.id == id)
                        .map(|f| f.fade.track)
                } else if matches!(prop, Prop::ReactScale | Prop::ReactGlow | Prop::ReactBright) {
                    {
                        let panel = timeline::panel(layout.timeline, self.scale());
                        self.lane_rows(&panel, self.scale())
                            .into_iter()
                            .find_map(|lr| {
                                lr.detail
                                    .into_iter()
                                    .find(|r| r.prop == prop)
                                    .map(|r| r.track)
                            })
                    }
                } else {
                    let (_, _, cards) = self.right_panel(&layout);
                    cards.rows.iter().find_map(|lr| {
                        lr.detail
                            .as_ref()?
                            .sliders
                            .iter()
                            .find(|r| r.prop == prop)
                            .map(|r| r.track)
                    })
                };
                match track {
                    Some(track) => {
                        let v = crate::props::value_for(prop, spark_ui::Slider::t_at(track, mx));
                        match target {
                            crate::ScrubTarget::Folder(id) => {
                                self.editor.set_folder_prop(id, prop, v);
                            }
                            _ => {
                                self.editor.set_prop(prop, v);
                            }
                        }
                        dirty = true;
                    }
                    None => self.slider_drag = None,
                }
            }
            if let Some((target, prop, last_y, moved)) = self.scrub_drag {
                // Scrub fields: vertical drag nudges the value (up grows
                // it); a 3-logical-px dead zone keeps clean clicks free to
                // open the field for typing instead.
                let dyl = ((py - last_y) / self.scale() as f64) as f32;
                if !moved {
                    if dyl.abs() >= 3.0 {
                        self.scrub_drag = Some((target, prop, py, true));
                    }
                } else if dyl != 0.0 {
                    let moved_it = match target {
                        ScrubTarget::Folder(id) => {
                            // Folder scale is a multiplier around 1, so it
                            // wants a far finer step than a shape's radius.
                            self.editor.folder(id).cloned().is_some_and(|f| {
                                let (cur, step) = match prop {
                                    Prop::X => (f.x, 2.0),
                                    Prop::Y => (f.y, 2.0),
                                    Prop::Rotation => (f.rotation, 0.5f32.to_radians()),
                                    _ => (f.scale, 0.01),
                                };
                                // Rotation keeps counting past a full turn,
                                // the same as a shape's — see `props::fit`.
                                self.editor.set_folder_prop(id, prop, cur - dyl * step)
                            })
                        }
                        ScrubTarget::Shape => self
                            .editor
                            .selected_props()
                            .map(|p| {
                                let (cur, step) = match prop {
                                    Prop::X => (p.x, 2.0),
                                    Prop::Y => (p.y, 2.0),
                                    Prop::Rotation => (p.rotation, 0.5f32.to_radians()),
                                    _ => (p.size, 1.5),
                                };
                                self.editor
                                    .set_prop(prop, crate::props::fit(prop, cur - dyl * step))
                            })
                            .unwrap_or(false),
                    };
                    if moved_it {
                        dirty = true;
                    }
                    self.scrub_drag = Some((target, prop, py, true));
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
            if let Some(id) = self.folder_drag {
                let (_, cards_vp, cards) = self.right_panel(&layout);
                if cards_vp.contains(mx, my)
                    && let Some(to) = cards
                        .rows
                        .iter()
                        .find(|r| r.row.contains(mx, my))
                        .map(|r| r.index)
                        // Dropping on another header targets its lowest member,
                        // so folders slot above/below each other, never nest.
                        .or_else(|| {
                            cards
                                .folders
                                .iter()
                                .find(|f| f.id != id && f.row.contains(mx, my))
                                .and_then(|f| self.editor.folder_members(f.id).first().copied())
                        })
                    && self.editor.move_folder(id, to)
                {
                    dirty = true;
                }
            }
            if self.fx_drag.is_some() {
                let (_, cards_vp, cards) = self.right_panel(&layout);
                let over = cards_vp.contains(mx, my).then(|| {
                    cards
                        .rows
                        .iter()
                        .find(|r| r.row.contains(mx, my))
                        .map(|r| r.index)
                });
                let over = over.flatten();
                if over != self.fx_drop {
                    self.fx_drop = over;
                    dirty = true;
                }
            }
            if let Some(from) = self.layer_drag {
                // Dragging a card reorders the stack, and that is all it
                // does. Dropping onto a folder header used to file the layer
                // into it, and dropping onto a loose card used to pull it
                // back out — both were unreliable enough to be a hazard
                // rather than a feature, so they're withdrawn until they can
                // be built properly. Ctrl+Shift+N still makes a folder from
                // a selection, which is the route that works.
                let (_, cards_vp, cards) = self.right_panel(&layout);
                if cards_vp.contains(mx, my)
                    && let Some(to) = cards
                        .rows
                        .iter()
                        .find(|r| r.row.contains(mx, my))
                        .map(|r| r.index)
                    && self.editor.move_layer(from, to)
                {
                    self.layer_drag = Some(to);
                    dirty = true;
                }
            }
            if let Some(b) = &mut self.box_sel {
                b.x1 = mx;
                b.y1 = my;
                if (b.x1 - b.x0).abs() + (b.y1 - b.y0).abs() > 6.0 {
                    b.moved = true;
                }
                dirty = true;
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
                if let Some((gi, gt, copy)) = self.key_drag
                    && !self.selected_keys.is_empty()
                {
                    // The grabbed key lands on the 16th grid and the whole
                    // selection rides along; a move that would collide with
                    // keys outside the group is refused, not merged.
                    let raw = self.time_view.t_at(mx, panel.axis);
                    let to = lanes::quantize(raw, &beat).clamp(beat.first_bar, duration);
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
                    // A set spanning wider than the track has no legal room —
                    // an inverted clamp range would panic.
                    let (min_dt, max_dt) = (beat.first_bar - lo, duration - hi);
                    let dt = if min_dt > max_dt {
                        0.0
                    } else {
                        (to - gt).clamp(min_dt, max_dt)
                    };
                    if dt.abs() > crate::anim::KEY_EPS {
                        // Alt+drag: the first move peels off copies, and the
                        // rest of the drag retimes those copies.
                        let keys = self.selected_keys.clone();
                        let moved = if copy {
                            self.editor.copy_group(&keys, dt)
                        } else {
                            self.editor.retime_group(&keys, dt)
                        };
                        if moved {
                            for k in &mut self.selected_keys {
                                k.1 += dt;
                            }
                            self.key_drag = Some((gi, gt + dt, false));
                            dirty = true;
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
        let tool_hover = self.toolbar().and_then(|bar| bar.hit(mx, my));
        if tool_hover != self.tool_hover {
            self.tool_hover = tool_hover;
            dirty = true;
        }
        if let Some(layout) = self.layout() {
            {
                // Which card button is under the cursor. Reuses the click
                // hit test rather than re-deriving geometry, so a button can
                // never be clickable somewhere it doesn't light.
                let (_, cards_vp, cards) = self.right_panel(&layout);
                let ch = crate::layers::hit(&cards, cards_vp, mx, my).filter(|h| h.hoverable());
                if ch != self.card_hover {
                    self.card_hover = ch;
                    dirty = true;
                }
            }
            {
                // Which browser row is under the cursor.
                let b = crate::browser::build(layout.left, self.scale());
                let h = layout
                    .left
                    .contains(mx, my)
                    .then(|| crate::browser::hit(&b, mx, my))
                    .flatten();
                if h != self.fx_browser_hover {
                    self.fx_browser_hover = h;
                    dirty = true;
                }
            }
            let zb = crate::view::zoom_bar(layout.zoom, self.scale());
            let zh = [zb.minus, zb.plus, zb.pct]
                .iter()
                .position(|r| r.contains(mx, my))
                .map(|i| i as u8);
            if zh != self.zoom_hover {
                self.zoom_hover = zh;
                dirty = true;
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
                let c = timeline::controls(layout.toolbar, self.scale(), self.timeline_tab);
                let hover = c.play.contains(mx, my);
                if hover != self.transport_hover {
                    self.transport_hover = hover;
                    dirty = true;
                }
                let panel = timeline::panel(layout.timeline, self.scale());
                let key = self.timeline_tab == timeline::Tab::Keys && panel.stamp.contains(mx, my);
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
