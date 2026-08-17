//! Mouse press/release dispatch: the File menu, title bar, toolbar, side
//! panels, then the canvas — first hit wins the click.

use spark_ui::TitleAction;
use winit::event_loop::ActiveEventLoop;

use crate::{Studio, colorhome, layers, picker};

impl Studio {
    pub(crate) fn press(&mut self, event_loop: &ActiveEventLoop) {
        let (cx, cy) = (self.cursor_px.0 as f32, self.cursor_px.1 as f32);
        if let Some(buf) = self.rename.take() {
            // Clicking away from an active rename commits it.
            self.editor.rename_primary(buf);
            self.request_redraw();
        }
        if self.field_edit.is_some() {
            // Clicking away from a scrub field commits the typed value.
            self.commit_field_edit();
            self.request_redraw();
        }
        if let Some(menus) = self.menus() {
            if let Some(mi) = menus.iter().position(|m| m.hit_anchor(cx, cy)) {
                self.menu_open = if self.menu_open == Some(mi) {
                    None
                } else {
                    Some(mi)
                };
                self.menu_hover = None;
                self.request_redraw();
                return;
            }
            if let Some(mi) = self.menu_open.take() {
                // An open menu owns the click: act on a row, close on
                // anything else, swallow it either way.
                let item = menus[mi].hit_item(cx, cy);
                self.request_redraw();
                match (mi, item) {
                    (0, Some(0)) => self.new_project(),
                    (0, Some(1)) => self.spawn_picker(picker::Purpose::OpenComp),
                    (0, Some(2)) => self.editor.save(&self.current_file),
                    (0, Some(3)) => self.spawn_picker(picker::Purpose::SaveComp),
                    (0, Some(4)) => self.spawn_picker(picker::Purpose::ImportAudio),
                    (0, Some(5)) => self.spawn_picker(picker::Purpose::SaveShape),
                    (0, Some(6)) => self.spawn_picker(picker::Purpose::ImportShape),
                    (0, Some(7)) => event_loop.exit(),
                    (1, Some(0)) => self.view_black = !self.view_black,
                    (1, Some(1)) => self.editor.snap_grid = !self.editor.snap_grid,
                    (1, Some(2)) => self.editor.smart_guides = !self.editor.smart_guides,
                    (1, Some(i @ (3 | 4))) => {
                        // Pick that Spark cursor; picking it again goes
                        // back to the system arrow.
                        let pick = Some(i - 3);
                        self.cursor_choice = if self.cursor_choice == pick {
                            None
                        } else {
                            pick
                        };
                        self.apply_cursor();
                    }
                    _ => {}
                }
                return;
            }
        }
        if let Some(tb) = self.title_bar() {
            if let Some(action) = tb.hit(cx, cy) {
                self.title_pressed = Some(action);
                return;
            }
            if tb.in_drag_zone(cx, cy) {
                if let Some(window) = &self.window {
                    let _ = window.drag_window();
                }
                return;
            }
        }
        if let Some(tool) = self.toolbar().and_then(|bar| bar.hit(cx, cy)) {
            self.editor.choose_tool(tool);
            self.request_redraw();
            return;
        }
        // Grabbing the toolbar's top edge resizes the bottom panel — the
        // toolbar and timeline move as one block. Double-click snaps the
        // panel back to its default height.
        if let Some(layout) = self.layout()
            && (cy - layout.toolbar.y).abs() <= 6.0 * self.scale()
        {
            let now = std::time::Instant::now();
            if self.last_resize_click.take().is_some_and(|(t, h)| {
                // A drag between the clicks means a deliberate resize, not
                // a double-click — the height must not have moved.
                now.duration_since(t).as_millis() < 400 && (self.timeline_h - h).abs() < 1.0
            }) {
                self.timeline_h = crate::DEFAULT_TIMELINE_H;
                self.request_redraw();
            } else {
                self.last_resize_click = Some((now, self.timeline_h));
                self.panel_resize = true;
            }
            return;
        }
        // The zoom bar under the layers panel.
        if let Some(layout) = self.layout() {
            let zb = crate::view::zoom_bar(layout.zoom, self.scale());
            let step = 1.25f32;
            let hit = if zb.minus.contains(cx, cy) {
                self.canvas_view
                    .zoom_step(1.0 / step, layout.viewport, self.scale());
                true
            } else if zb.plus.contains(cx, cy) {
                self.canvas_view
                    .zoom_step(step, layout.viewport, self.scale());
                true
            } else if zb.pct.contains(cx, cy) {
                self.canvas_view.reset();
                true
            } else {
                false
            };
            if hit {
                self.request_redraw();
                return;
            }
        }
        // Any press rebuilds the key highlight; a hit on a key below keeps
        // or extends the previous set.
        let prev_keys = std::mem::take(&mut self.selected_keys);
        if !prev_keys.is_empty() {
            self.request_redraw();
        }
        if self.audio.is_some()
            && let Some(layout) = self.layout()
        {
            let scale = self.scale();
            let controls = crate::timeline::controls(layout.toolbar, scale, self.timeline_tab);
            if controls.play.contains(cx, cy) {
                self.toggle_play();
                self.request_redraw();
                return;
            }
            if controls.key.is_some_and(|k| k.contains(cx, cy)) {
                if self.editor.stamp_key() {
                    self.request_redraw();
                }
                return;
            }
            if controls.wave.contains(cx, cy) {
                if self.timeline_tab != crate::timeline::Tab::Wave {
                    self.timeline_tab = crate::timeline::Tab::Wave;
                    self.request_redraw();
                }
                return;
            }
            if controls.tk.contains(cx, cy) {
                // On its own tab the toggle flips Arrange <-> Keys; from
                // the Wave tab it returns to whichever it last showed.
                if self.timeline_tab == self.edit_tab {
                    self.edit_tab = self.edit_tab.flipped();
                }
                self.timeline_tab = self.edit_tab;
                self.request_redraw();
                return;
            }
            let panel = crate::timeline::panel(layout.timeline, scale);
            if panel.ruler.contains(cx, cy) {
                if self.modifiers.shift_key()
                    && let Some(track) = &self.audio
                {
                    // Shift+drag brackets a loop; the click alone already
                    // loops the bar under the cursor.
                    let bar_s = 4.0 * 60.0 / track.beat.bpm.max(1.0);
                    let t = self.time_view.t_at(cx, panel.axis);
                    let anchor = crate::timeline::bar_floor(t, &track.beat)
                        .clamp(track.beat.first_bar, track.duration);
                    self.loop_drag = Some(anchor);
                    self.loop_region = Some((anchor, (anchor + bar_s).min(track.duration)));
                    self.loop_on = true;
                    self.apply_loop();
                    self.request_redraw();
                } else {
                    self.seek_to_x(&panel, cx);
                }
                return;
            }
            if self.timeline_tab == crate::timeline::Tab::Keys
                && let Some(&pi) = self.editor.selection().last()
            {
                // The React sliders docked under the lane names.
                let rr = crate::lanes::react_rows(&panel, scale, self.editor.react(pi));
                if let Some(r) = rr.iter().find(|r| {
                    cx >= r.track.x
                        && cx <= r.track.x + r.track.w
                        && (cy - (r.track.y + r.track.h * 0.5)).abs() <= r.track.h * 2.5
                }) {
                    self.slider_drag = Some(r.prop);
                    let t = ((cx - r.track.x) / r.track.w).clamp(0.0, 1.0);
                    if self
                        .editor
                        .set_prop(r.prop, crate::props::value_for(r.prop, t))
                    {
                        self.request_redraw();
                    }
                    return;
                }
            }
            if let Some(hit) = self.lane_hit(cx, cy) {
                match hit {
                    crate::lanes::LaneHit::Key(i, t) => {
                        if self.modifiers.control_key() {
                            // Ctrl+click: smooth <-> linear (selection kept).
                            self.selected_keys = prev_keys;
                            if self.editor.toggle_ease_at(i, t) {
                                self.request_redraw();
                            }
                        } else if self.modifiers.shift_key() {
                            // Shift+click: toggle membership in the set.
                            self.selected_keys = prev_keys;
                            match self.selected_keys.iter().position(|&(si, st)| {
                                si == i && (st - t).abs() < crate::anim::KEY_EPS
                            }) {
                                Some(pos) => {
                                    self.selected_keys.remove(pos);
                                }
                                None => self.selected_keys.push((i, t)),
                            }
                            self.request_redraw();
                        } else {
                            // Click a member: keep the group and drag it all;
                            // otherwise select just this key. Alt peels off
                            // copies as the drag starts moving.
                            if crate::anim::key_list_has(&prev_keys, i, t) {
                                self.selected_keys = prev_keys;
                            } else {
                                self.selected_keys = vec![(i, t)];
                            }
                            self.key_drag = Some((i, t, self.modifiers.alt_key()));
                            self.request_redraw();
                        }
                    }
                    crate::lanes::LaneHit::Gutter(i) => {
                        if self.editor.select(Some(i)) {
                            self.request_redraw();
                        }
                    }
                    crate::lanes::LaneHit::Scrub => {
                        if self.modifiers.control_key() {
                            // Ctrl+drag rubber-bands a key selection
                            // (Ctrl+Shift+drag extends the current set).
                            self.box_sel = Some(crate::BoxSel {
                                x0: cx,
                                y0: cy,
                                x1: cx,
                                y1: cy,
                                moved: false,
                                prev: if self.modifiers.shift_key() {
                                    prev_keys
                                } else {
                                    Vec::new()
                                },
                            });
                        } else {
                            // Plain press/drag on the lanes scrubs.
                            self.seek_to_x(&panel, cx);
                        }
                    }
                }
                return;
            }
            if panel.lanes.contains(cx, cy) && cx >= panel.axis.0 {
                // Wave and Arrange have nothing grabbable below the ruler
                // yet, but their axis still scrubs.
                self.seek_to_x(&panel, cx);
                return;
            }
        }
        if let Some(layout) = self.layout() {
            let (color_vp, cards_vp, cards) = self.right_panel(&layout);
            // While the gradient's B endpoint is armed, color edits route
            // there (for gradient-enabled shapes).
            let to_b = self.grad_edit_b && self.editor.selected_props().is_some_and(|p| p.grad);
            let home = self.color_home(color_vp);
            if let Some(hit) = home.hit(cx, cy) {
                let dirty = match hit {
                    colorhome::ColorHit::Swatch(i) => {
                        let dirty = self.editor.set_color_index(i, to_b);
                        self.sync_picker();
                        dirty
                    }
                    colorhome::ColorHit::Custom => {
                        self.picker_hsv = match self.picker_hsv {
                            Some(_) => None,
                            None => Some(hsv_of_linear(home.custom_rgb)),
                        };
                        true
                    }
                    colorhome::ColorHit::Sv(sv, v) => {
                        if let Some(hsv) = &mut self.picker_hsv {
                            hsv[1] = sv;
                            hsv[2] = v;
                        }
                        self.picker_drag = Some(crate::PickerDrag::Sv);
                        self.apply_picker();
                        true
                    }
                    colorhome::ColorHit::Hue(h) => {
                        if let Some(hsv) = &mut self.picker_hsv {
                            hsv[0] = h;
                        }
                        self.picker_drag = Some(crate::PickerDrag::Hue);
                        self.apply_picker();
                        true
                    }
                };
                if dirty {
                    self.request_redraw();
                }
                return;
            }
            if let Some(hit) = layers::hit(&cards.rows, cards_vp, cx, cy) {
                self.card_hit(hit, &cards);
                return;
            }
            if layout.right.contains(cx, cy) {
                // A miss anywhere in the right panel — the gaps between
                // cards, dead space in the color home — is not a deselect.
                // The panel is the selection's home, not a neutral surface.
                return;
            }
        }
        let in_viewport = self.layout().is_some_and(|l| l.viewport.contains(cx, cy));
        if in_viewport {
            // Transform handles float above the shapes.
            if let Some(layout) = self.layout()
                && let Some(h) =
                    crate::handles::build(&self.editor, self.canvas_map(&layout), self.scale())
                && let Some(hit) = h.hit(cx, cy)
            {
                let cur = self.editor.cursor();
                let center = h.center;
                self.handle_drag = Some(match hit {
                    crate::handles::HandleHit::Corner => crate::HandleDrag::Scale {
                        center,
                        ref_dist: ((cur[0] - center[0]).powi(2) + (cur[1] - center[1]).powi(2))
                            .sqrt()
                            .max(0.5),
                    },
                    crate::handles::HandleHit::Width => crate::HandleDrag::Width,
                    crate::handles::HandleHit::Height => crate::HandleDrag::Height,
                    crate::handles::HandleHit::Vertex(k) => crate::HandleDrag::Vertex(k),
                    crate::handles::HandleHit::Rotate => crate::HandleDrag::Rotate {
                        center,
                        prev: (cur[1] - center[1]).atan2(cur[0] - center[0]),
                    },
                });
                return;
            }
            if self.modifiers.alt_key() {
                // Alt+click is the eyedropper: take the color under the
                // cursor without selecting or moving anything.
                if self.editor.eyedrop_at_cursor() {
                    self.sync_picker();
                    self.request_redraw();
                }
                return;
            }
            if self.editor.mouse_down(self.modifiers.control_key()) {
                self.request_redraw();
            }
        } else if self.editor.deselect() {
            // Empty chrome is a neutral surface — clicking it drops the
            // selection, same as empty canvas.
            self.request_redraw();
        }
    }

    /// A click landed on a layer card. Touching any control claims that
    /// card's shape first, so edits always target what you're touching.
    fn card_hit(&mut self, hit: layers::CardHit, cards: &layers::Cards) {
        let ensure = |s: &mut Self, i: usize| {
            if !s.editor.selection().contains(&i) {
                s.editor.select(Some(i));
            }
        };
        match hit {
            layers::CardHit::Head(i) => {
                let grouped = cards.rows.iter().any(|r| r.index == i && r.grouped);
                let ctrl = self.modifiers.control_key();
                let shift = self.modifiers.shift_key();
                let now = std::time::Instant::now();
                let double = !ctrl
                    && !shift
                    && self
                        .last_layer_click
                        .take()
                        .is_some_and(|(li, t)| li == i && now.duration_since(t).as_millis() < 400);
                if double {
                    // Double-click on the head: rename in place.
                    self.editor.select(Some(i));
                    self.rename = Some(self.editor.name(i).to_string());
                    self.layer_drag = None;
                    self.request_redraw();
                    return;
                }
                self.last_layer_click = Some((i, now));
                let changed = if ctrl {
                    self.editor.toggle_select(i)
                } else if shift {
                    self.editor.select_range(i)
                } else {
                    self.editor.select(Some(i))
                };
                if changed {
                    self.request_redraw();
                }
                if !ctrl && !shift && !grouped {
                    // Group cards don't drag-reorder (yet) — their members
                    // keep their own stack slots. Neither modifier click
                    // starts a drag: they're set-building, not moving.
                    self.layer_drag = Some(i);
                }
            }
            layers::CardHit::Cog(i) => {
                ensure(self, i);
                self.card_open = if self.card_open == Some(i) {
                    None
                } else {
                    Some(i)
                };
                self.request_redraw();
            }
            layers::CardHit::Eye(i) => {
                if self.editor.toggle_hidden(i) {
                    self.request_redraw();
                }
            }
            layers::CardHit::Scrub(i, prop) => {
                ensure(self, i);
                self.scrub_drag = Some((prop, self.cursor_px.1, false));
                self.request_redraw();
            }
            layers::CardHit::Slider(i, prop, t) => {
                ensure(self, i);
                self.slider_drag = Some(prop);
                if self.editor.set_prop(prop, crate::props::value_for(prop, t)) {
                    self.request_redraw();
                }
            }
            layers::CardHit::Outline(i, on) => {
                ensure(self, i);
                if self.editor.set_outline(on) {
                    self.request_redraw();
                }
            }
            layers::CardHit::Blend(i, on) => {
                ensure(self, i);
                if self.editor.set_additive(on) {
                    self.request_redraw();
                }
            }
            layers::CardHit::Gradient(i, on) => {
                ensure(self, i);
                if self.editor.set_gradient(on) {
                    self.request_redraw();
                }
            }
            layers::CardHit::Chip(i, b) => {
                ensure(self, i);
                self.grad_edit_b = b;
                // Arming an endpoint loads its color as the current one, so
                // the bar, the square and the chip all agree.
                if let Some(p) = self.editor.selected_props() {
                    self.editor.load_color(if b { p.rgb2 } else { p.rgb });
                    self.sync_picker();
                }
                self.request_redraw();
            }
        }
    }

    pub(crate) fn release(&mut self, event_loop: &ActiveEventLoop) {
        let (cx, cy) = (self.cursor_px.0 as f32, self.cursor_px.1 as f32);
        if let Some(b) = self.box_sel.take() {
            if b.moved {
                // Rubber band: everything inside joins the selection.
                let mut sel = self.keys_in_box(
                    b.x0.min(b.x1),
                    b.y0.min(b.y1),
                    b.x0.max(b.x1),
                    b.y0.max(b.y1),
                );
                for k in b.prev {
                    if !crate::anim::key_list_has(&sel, k.0, k.1) {
                        sel.push(k);
                    }
                }
                self.selected_keys = sel;
                self.request_redraw();
            } else if let Some(layout) = self.layout() {
                // A still click on empty lane space is just a seek.
                let panel = crate::timeline::panel(layout.timeline, self.scale());
                self.seek_to_x(&panel, b.x0);
            }
        }
        self.editor.end_gesture();
        self.layer_drag = None;
        self.handle_drag = None;
        self.picker_drag = None;
        self.timeline_scrub = false;
        self.key_drag = None;
        self.loop_drag = None;
        self.panel_resize = false;
        if let Some((prop, _, moved)) = self.scrub_drag.take()
            && !moved
        {
            // A clean click (no drag) opens the field for typing.
            if let Some(p) = self.editor.selected_props() {
                let v = match prop {
                    crate::editor::Prop::X => p.x,
                    crate::editor::Prop::Y => p.y,
                    crate::editor::Prop::Rotation => p.rotation.to_degrees(),
                    _ => p.size,
                };
                self.field_edit = Some((prop, format!("{v:.0}")));
                self.request_redraw();
            }
        }
        if self.slider_drag.take().is_some() {
            return;
        }
        if let Some(pressed) = self.title_pressed.take() {
            let hit = self.title_bar().and_then(|tb| tb.hit(cx, cy));
            if hit == Some(pressed)
                && let Some(window) = &self.window
            {
                match pressed {
                    TitleAction::Minimize => window.set_minimized(true),
                    TitleAction::Maximize => window.set_maximized(!window.is_maximized()),
                    TitleAction::Close => event_loop.exit(),
                }
            }
            self.request_redraw();
        } else if self.editor.mouse_up() {
            self.request_redraw();
        }
    }
}

/// Linear shape color → display-space HSV, for seeding the picker.
pub(crate) fn hsv_of_linear(rgb: [f32; 3]) -> [f32; 3] {
    spark_ui::picker::rgb_to_hsv([
        spark_ui::picker::linear_to_srgb(rgb[0]),
        spark_ui::picker::linear_to_srgb(rgb[1]),
        spark_ui::picker::linear_to_srgb(rgb[2]),
    ])
}
