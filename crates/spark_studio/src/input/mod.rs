//! Mouse press dispatch: the File menu, title bar, toolbar, then the
//! canvas — first hit wins the click. Release lives in `release`.

use winit::event_loop::ActiveEventLoop;

use crate::{Studio, picker};

mod release;
mod wheel;

impl Studio {
    pub(crate) fn press(&mut self, event_loop: &ActiveEventLoop) {
        // An export owns the document until it's done: a click that moved
        // a shape mid-render would land in the video. Esc cancels.
        if self.export.is_some() {
            return;
        }
        // The last export's result stays in the status strip until the
        // next thing happens.
        self.export_note = None;
        let (cx, cy) = (self.cursor_px.0 as f32, self.cursor_px.1 as f32);
        // A press anywhere but the inspector commits a field being typed
        // into; inside it, the inspector decides (a click in the field
        // places the caret).
        // The React popup, then the colour popup, while either is up:
        // clicks inside are theirs; a click elsewhere closes it and goes
        // on to whatever it hit.
        match self.react_press(cx, cy) {
            Some(true) => {
                self.request_redraw();
                return;
            }
            Some(false) => self.request_redraw(),
            None => {}
        }
        match self.popup_press(cx, cy) {
            Some(true) => {
                self.request_redraw();
                return;
            }
            Some(false) => self.request_redraw(),
            None => {}
        }
        let in_right = self.layout().is_some_and(|l| l.right.contains(cx, cy));
        if !in_right && self.inspector_commit() {
            self.request_redraw();
        }
        // An open context menu owns the click: a rail button toggles its
        // tool and the menu stays; the panel keeps its own clicks;
        // anything else closes it. Swallowed either way.
        if self.context_press_left(cx, cy) {
            return;
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
                use crate::menu::{
                    ADD, CANVAS, FILE, FILE_EXIT, FILE_EXPORT, FILE_NEW_COMP, FILE_PLACE_COMP,
                    VIEW,
                };
                match (mi, item) {
                    (FILE, Some(0)) => {
                        if self.confirm_discard(crate::project::Discard::New) {
                            self.new_project();
                        }
                    }
                    (FILE, Some(1)) => {
                        if self.confirm_discard(crate::project::Discard::Open) {
                            self.spawn_picker(picker::Purpose::OpenComp);
                        }
                    }
                    (FILE, Some(2)) => {
                        let f = self.current_file.clone();
                        self.save_project(&f);
                    }
                    (FILE, Some(3)) => self.spawn_picker(picker::Purpose::SaveComp),
                    (FILE, Some(4)) => self.spawn_picker(picker::Purpose::ImportAudio),
                    (FILE, Some(5)) => self.spawn_picker(picker::Purpose::ImportSound),
                    (FILE, Some(6)) => self.spawn_picker(picker::Purpose::SaveShape),
                    (FILE, Some(7)) => self.spawn_picker(picker::Purpose::ImportShape),
                    (FILE, Some(8)) => self.spawn_picker(picker::Purpose::ImportMesh),
                    (FILE, Some(FILE_NEW_COMP)) => self.new_comp(),
                    (FILE, Some(FILE_PLACE_COMP)) => {
                        self.spawn_picker(picker::Purpose::PlaceComp)
                    }
                    (FILE, Some(FILE_EXPORT)) => self.spawn_picker(picker::Purpose::ExportVideo),
                    (FILE, Some(FILE_EXIT)) => {
                        if self.confirm_discard(crate::project::Discard::Quit) {
                            event_loop.exit();
                        }
                    }
                    (CANVAS, Some(k)) => {
                        if let Some((_, size)) = crate::menu::CANVAS_PRESETS.get(k) {
                            self.set_canvas(*size);
                        }
                    }
                    (ADD, Some(k)) => {
                        // The lights first, then the built-in meshes,
                        // which arrive the way an import does.
                        let lights = spark_render::LIGHT_KINDS.len();
                        if k < lights {
                            self.editor.add_light(spark_render::LightKind::from_index(k));
                        } else if let Some(path) = crate::primitives::PATHS.get(k - lights) {
                            self.import_mesh(std::path::PathBuf::from(path));
                        }
                    }
                    (VIEW, Some(0)) => self.view_black = !self.view_black,
                    (VIEW, Some(1)) => self.editor.snap_grid = !self.editor.snap_grid,
                    (VIEW, Some(2)) => self.editor.smart_guides = !self.editor.smart_guides,
                    (VIEW, Some(i @ (3 | 4))) => {
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
                    (VIEW, Some(5)) => self.half_res_play = !self.half_res_play,
                    (VIEW, Some(6)) => {
                        self.toggle_fly();
                    }
                    (VIEW, Some(7)) => self.floor = !self.floor,
                    _ => {}
                }
                return;
            }
        }
        // Inside a comp, the status strip's `project > comp` is the way
        // back — the centre of the strip, where the name lives now.
        if !self.comp_stack.is_empty()
            && let Some(layout) = self.layout()
            && layout.status.contains(cx, cy)
            && (cx - (layout.status.x + layout.status.w * 0.5)).abs() < 250.0 * self.scale()
        {
            self.leave_comp();
            return;
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
        // The left panel's tabs and effect rows take their clicks; its
        // air stays the neutral surface it was.
        if let Some(layout) = self.layout()
            && layout.left.contains(cx, cy)
            && self.left_press(layout.left, cx, cy)
        {
            self.request_redraw();
            return;
        }
        // The right panel is the inspector's: its widgets take the click,
        // and its air is not a deselect — a miss beside a field must not
        // drop the thing the field is for.
        if let Some(layout) = self.layout()
            && layout.right.contains(cx, cy)
        {
            if self.inspector_press(layout.right, cx, cy) {
                self.request_redraw();
            }
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
        if let Some(layout) = self.layout() {
            let scale = self.scale();
            let controls = crate::timeline::controls(layout.toolbar, scale);
            if controls.play.contains(cx, cy) {
                self.toggle_play();
                self.request_redraw();
                return;
            }
            if controls.loop_btn.contains(cx, cy) {
                self.toggle_loop();
                self.request_redraw();
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
            // The zoom cluster at the toolbar's right end.
            let step = 1.25f32;
            let zoom_hit = if controls.zoom_minus.contains(cx, cy) {
                self.canvas_view
                    .zoom_step(1.0 / step, layout.viewport, self.editor.canvas());
                true
            } else if controls.zoom_plus.contains(cx, cy) {
                self.canvas_view
                    .zoom_step(step, layout.viewport, self.editor.canvas());
                true
            } else if controls.zoom_pct.contains(cx, cy) {
                self.canvas_view.reset(self.editor.canvas());
                true
            } else {
                false
            };
            if zoom_hit {
                self.request_redraw();
                return;
            }
            if controls.snap.contains(cx, cy) {
                self.snap_playhead = !self.snap_playhead;
                println!(
                    "playhead snap {}",
                    if self.snap_playhead {
                        format!("on ({} bar)", self.grid_div.label())
                    } else {
                        "off".to_string()
                    }
                );
                self.request_redraw();
                return;
            }
            if controls.wave.contains(cx, cy) {
                self.wave_overlay = !self.wave_overlay;
                println!(
                    "waveform overlay {}",
                    if self.wave_overlay { "on" } else { "off" }
                );
                self.request_redraw();
                return;
            }
            let panel = crate::timeline::panel(layout.timeline, scale);
            // The hero Keyframe button in the sidebar's tools bay.
            if panel.stamp.contains(cx, cy) {
                if self.stamp() {
                    self.request_redraw();
                }
                return;
            }
            // The clip view, while it's up, owns the whole panel: its
            // ruler scrubs the song through the clip, its rows and
            // diamonds take the click, and nothing reaches the
            // arrangement underneath.
            if self.clip_view_press(&panel, scale, cx, cy) {
                return;
            }
            if panel.ruler.contains(cx, cy) {
                // The brace's edges and band are the loop's; Shift
                // brackets a fresh one; the rest of the ruler scrubs.
                if self.loop_press(&panel, cx, cy) {
                    self.request_redraw();
                } else {
                    self.seek_to_x(&panel, cx);
                }
                return;
            }
            if self.arrange_press(&panel, scale, cx, cy) {
                return;
            }
            if panel.lanes.contains(cx, cy) && cx >= panel.axis.0 {
                // Empty arrangement air below the ruler scrubs.
                self.seek_to_x(&panel, cx);
                return;
            }
        }
        let in_viewport = self.layout().is_some_and(|l| l.viewport.contains(cx, cy));
        if in_viewport {
            // The 3D gizmo floats above everything: its parts are small
            // and deliberate, so a hit on one is never a miss on a shape.
            if let Some(layout) = self.layout()
                && let Some(g) = self.gizmo(&layout)
                && let Some(res) = self.gpu.as_ref().map(|g| g.size())
                && let Some(part) = g.hit(&self.camera(), &self.framing(&layout), res, [cx, cy])
            {
                // An arrow drag locks to the other objects' edges when
                // smart guides are on — the 3D half of that toggle.
                let snap = match part {
                    crate::gizmo::Part::Arrow(axis) if self.editor.smart_guides => {
                        crate::align::AxisSnap::build(
                            &self.editor,
                            &self.meshes,
                            axis,
                            g.px(crate::align::SNAP_PX),
                        )
                    }
                    _ => None,
                };
                self.gizmo_drag =
                    g.begin(part, &self.camera(), &self.framing(&layout), res, [cx, cy], snap);
                self.request_redraw();
                return;
            }
            // Transform handles float above the shapes — in the comp
            // viewer; the fly view has only the gizmo.
            if self.fly.is_none()
                && let Some(layout) = self.layout()
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
                    crate::handles::HandleHit::End(k) => crate::HandleDrag::End(k),
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
                    self.request_redraw();
                }
                return;
            }
            if self.fly.is_some()
                && self.editor.tool() == crate::editor::Tool::Select
                && !self.editor.hit_at_cursor()
            {
                // Empty space in the fly view: a drag looks around, and a
                // still click drops the selection on release — so a look
                // never costs you the selection.
                self.look_press();
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

