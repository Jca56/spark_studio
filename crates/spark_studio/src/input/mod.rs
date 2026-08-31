//! Mouse press dispatch: the File menu, title bar, toolbar, side panels,
//! then the canvas — first hit wins the click. Release and the layer-card
//! dispatch live in `release`.

use winit::event_loop::ActiveEventLoop;

use crate::{Studio, colorhome, layers, picker};

mod release;
mod wheel;

impl Studio {
    pub(crate) fn press(&mut self, event_loop: &ActiveEventLoop) {
        let (cx, cy) = (self.cursor_px.0 as f32, self.cursor_px.1 as f32);
        if let Some(buf) = self.rename.take() {
            // Clicking away from an active rename commits it.
            self.commit_rename(buf);
            self.request_redraw();
        }
        if self.field_edit.is_some() {
            if self.field_box().is_some_and(|r| r.contains(cx, cy)) {
                // Inside its own box: place the caret and start selecting.
                // Committing here and reopening on release is what made a
                // second click flash the field off and on.
                let at = crate::textbox::index_at(&self.field_caret_xs, cx);
                if let Some((_, _, tb)) = &mut self.field_edit {
                    tb.place(at);
                }
                self.field_drag = true;
                self.request_redraw();
                return;
            }
            // Clicking away from a scrub field commits the typed value.
            self.commit_field_edit();
            self.request_redraw();
        }
        if self.material_edit.is_some() {
            // Same for a hex field — it must not keep the keyboard once
            // the click has landed somewhere else.
            self.material_edit = None;
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
                    (0, Some(7)) => self.spawn_picker(picker::Purpose::ImportMesh),
                    (0, Some(8)) => event_loop.exit(),
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
                    (1, Some(5)) => self.materials_open = !self.materials_open,
                    (1, Some(6)) => self.half_res_play = !self.half_res_play,
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
        if self.materials_open
            && let Some(layout) = self.layout()
            && layout.timeline.contains(cx, cy)
        {
            self.press_materials(cx, cy);
            return;
        }
        // The effects browser: press a row to start dragging that effect
        // onto a layer.
        if let Some(layout) = self.layout()
            && layout.left.contains(cx, cy)
        {
            let b = crate::browser::build(layout.left, self.scale());
            if let Some(kind) = crate::browser::hit(&b, cx, cy) {
                self.fx_drag = Some(kind);
                self.request_redraw();
            }
            return;
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
        if let Some(layout) = self.layout() {
            let scale = self.scale();
            let controls = crate::timeline::controls(layout.toolbar, scale, self.timeline_tab);
            if controls.play.contains(cx, cy) {
                self.toggle_play();
                self.request_redraw();
                return;
            }
            if let Some(k) = controls.tabs.iter().position(|b| b.contains(cx, cy)) {
                let want = crate::timeline::TAB_ORDER[k];
                if self.timeline_tab != want {
                    self.timeline_tab = want;
                    self.request_redraw();
                }
                return;
            }
            if controls.bpm.contains(cx, cy) {
                // Opens empty: you're replacing the tempo, not editing a
                // digit of it, and the number you're about to type is one
                // you already know.
                self.bpm_edit = Some(String::new());
                self.request_redraw();
                return;
            }
            if controls.snap.contains(cx, cy) {
                self.snap_playhead = !self.snap_playhead;
                println!(
                    "playhead snap {}",
                    if self.snap_playhead {
                        "on (1/4 bar)"
                    } else {
                        "off"
                    }
                );
                self.request_redraw();
                return;
            }
            let panel = crate::timeline::panel(layout.timeline, scale);
            // The hero Keyframe button lives in the Keys sidebar.
            if self.timeline_tab == crate::timeline::Tab::Keys && panel.stamp.contains(cx, cy) {
                if self.editor.stamp_key() {
                    self.request_redraw();
                }
                return;
            }
            if panel.ruler.contains(cx, cy) {
                if self.modifiers.shift_key() {
                    // Shift+drag brackets a loop; the click alone already
                    // loops the bar under the cursor.
                    let (beat, duration) = (self.grid(), self.duration());
                    let bar_s = 4.0 * 60.0 / beat.bpm.max(1.0);
                    let t = self.time_view.t_at(cx, panel.axis);
                    let anchor =
                        crate::timeline::bar_floor(t, &beat).clamp(beat.first_bar, duration);
                    self.loop_drag = Some(anchor);
                    self.loop_region = Some((anchor, (anchor + bar_s).min(duration)));
                    self.loop_on = true;
                    self.apply_loop();
                    self.request_redraw();
                } else {
                    self.seek_to_x(&panel, cx);
                }
                return;
            }
            if self.timeline_tab == crate::timeline::Tab::Keys {
                // React sliders inside whichever lane is cog-expanded.
                let rows = self.lane_rows(&panel, scale);
                let hit = rows.iter().find_map(|lr| {
                    lr.detail
                        .iter()
                        .find(|r| {
                            cx >= r.track.x
                                && cx <= r.track.x + r.track.w
                                && (cy - (r.track.y + r.track.h * 0.5)).abs() <= r.track.h * 2.5
                        })
                        .map(|r| (lr.owner, r.prop, r.track))
                });
                if let Some((owner, prop, track)) = hit {
                    // The sliders write to the selection, so claim the lane's
                    // shape first — otherwise you'd edit something else.
                    if let crate::anim::Owner::Shape(id) = owner
                        && let Some(i) = self.editor.index_of(id)
                        && !self.editor.selection().contains(&i)
                    {
                        self.editor.select(Some(i));
                    }
                    self.slider_drag = Some((crate::ScrubTarget::Shape, prop));
                    let t = spark_ui::Slider::t_at(track, cx);
                    if self.editor.set_prop(prop, crate::props::value_for(prop, t)) {
                        self.request_redraw();
                    }
                    return;
                }
                // The cog that opens a lane's settings.
                if let Some(lr) = rows
                    .iter()
                    .find(|lr| lr.cog.is_some_and(|c| c.contains(cx, cy)))
                {
                    self.lane_open = if self.lane_open == Some(lr.owner) {
                        None
                    } else {
                        Some(lr.owner)
                    };
                    self.request_redraw();
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
                    crate::lanes::LaneHit::Gutter(o) => {
                        // Clicking a folder lane's name grabs its contents,
                        // same as clicking the folder card.
                        let changed = match o {
                            crate::anim::Owner::Shape(id) => {
                                self.editor.select(self.editor.index_of(id))
                            }
                            crate::anim::Owner::Folder(id) => self.editor.select_folder(id),
                        };
                        if changed {
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
            // While the gradient's B endpoint is armed, colour edits route
            // there. Whether a given layer *has* a gradient to route into is
            // `set_current_color`'s call, per layer — the arming is one flag
            // for a selection that may be mixed.
            let to_b = self.grad_edit_b;
            let home = self.color_home(color_vp);
            if let Some(hit) = home.hit(cx, cy) {
                let dirty = match hit {
                    colorhome::ColorHit::Swatch(i) => {
                        // The chips are whatever palette the home offered:
                        // the neon set for a shape, the grey ladder while
                        // the chrome is being painted. Either way the chip
                        // you clicked is the colour you get.
                        let dirty = match self.chrome_target() {
                            Some(t) => {
                                let rgb = home.chips[i];
                                let a = crate::materials::color_of(t, self.material_pick)[3];
                                let c = [rgb[0], rgb[1], rgb[2], a];
                                crate::materials::set_color(t, self.material_pick, c);
                                true
                            }
                            None => self.editor.set_color_index(i, to_b),
                        };
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
                    colorhome::ColorHit::Dice => {
                        self.editor.random = !self.editor.random;
                        println!("dice: {}", if self.editor.random { "armed" } else { "off" });
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
                    colorhome::ColorHit::Alpha(a) => {
                        self.picker_drag = Some(crate::PickerDrag::Alpha);
                        self.apply_alpha(a);
                        true
                    }
                };
                if dirty {
                    self.request_redraw();
                }
                return;
            }
            // Past the playground and past the colour home: whatever this
            // click is, it is not about a chrome colour. The picker hands
            // itself back to the canvas, so it can never be left silently
            // painting the side panels while you think you are picking a
            // shape's colour.
            if self.material_target.take().is_some() {
                self.sync_picker();
                self.request_redraw();
            }
            if let Some(hit) = layers::hit(&cards, cards_vp, cx, cy) {
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
}

/// Linear shape color → display-space HSV, for seeding the picker.
pub(crate) fn hsv_of_linear(rgb: [f32; 3]) -> [f32; 3] {
    spark_ui::picker::rgb_to_hsv([
        spark_ui::picker::linear_to_srgb(rgb[0]),
        spark_ui::picker::linear_to_srgb(rgb[1]),
        spark_ui::picker::linear_to_srgb(rgb[2]),
    ])
}
