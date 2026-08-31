//! Mouse release, and the layer-card click dispatch that presses hand off
//! to. Split from `input` so the press path stays readable.

use spark_ui::TitleAction;
use winit::event_loop::ActiveEventLoop;

use crate::{ScrubTarget, Studio, layers};

impl Studio {
    /// A click landed on a layer card. Touching any control claims that
    /// card's shape first, so edits always target what you're touching.
    pub(super) fn card_hit(&mut self, hit: layers::CardHit, cards: &layers::Cards) {
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
                let showing =
                    self.card_open == Some(i) && self.card_tab == layers::CardTab::Settings;
                self.card_open = (!showing).then_some(i);
                self.card_tab = layers::CardTab::Settings;
                self.request_redraw();
            }
            layers::CardHit::Eye(i) => {
                if self.editor.toggle_hidden(i) {
                    self.request_redraw();
                }
            }
            layers::CardHit::Scrub(i, prop) => {
                ensure(self, i);
                self.scrub_drag = Some((ScrubTarget::Shape, prop, self.cursor_px.1, false));
                self.request_redraw();
            }
            layers::CardHit::FolderScrub(id, prop) => {
                self.scrub_drag = Some((ScrubTarget::Folder(id), prop, self.cursor_px.1, false));
                self.request_redraw();
            }
            layers::CardHit::Slider(i, prop, t) => {
                ensure(self, i);
                self.slider_drag = Some((ScrubTarget::Shape, prop));
                let canvas = self.editor.canvas();
                if self
                    .editor
                    .set_prop(prop, crate::props::value_for(prop, t, canvas))
                {
                    self.request_redraw();
                }
            }
            layers::CardHit::FolderSlider(id, prop, t) => {
                self.slider_drag = Some((ScrubTarget::Folder(id), prop));
                let v = crate::props::value_for(prop, t, self.editor.canvas());
                if self.editor.set_folder_prop(id, prop, v) {
                    self.request_redraw();
                }
            }
            layers::CardHit::Outline(i, on) => {
                ensure(self, i);
                if self.editor.set_outline(on) {
                    self.request_redraw();
                }
            }
            layers::CardHit::LightKind(i, k) => {
                ensure(self, i);
                if self.editor.set_light_kind(k) {
                    self.request_redraw();
                }
            }
            layers::CardHit::Form(i, form) => {
                ensure(self, i);
                if self.editor.set_star_form(form) {
                    self.request_redraw();
                }
            }
            layers::CardHit::Blend(i, on) => {
                ensure(self, i);
                if self.editor.set_additive(on) {
                    self.request_redraw();
                }
            }
            layers::CardHit::FolderDisclose(id) => {
                if self.editor.toggle_folder_collapsed(id) {
                    self.request_redraw();
                }
            }
            layers::CardHit::FolderEye(id) => {
                if self.editor.toggle_folder_hidden(id) {
                    self.request_redraw();
                }
            }
            layers::CardHit::FolderHead(id) => {
                // Clicking a folder grabs its contents, so Delete and the
                // canvas transforms act on the whole thing. Double-click
                // renames the folder itself.
                let now = std::time::Instant::now();
                let double = self
                    .last_folder_click
                    .take()
                    .is_some_and(|(fi, t)| fi == id && now.duration_since(t).as_millis() < 400);
                if double {
                    self.rename_folder = Some(id);
                    self.rename = Some(
                        self.editor
                            .folder(id)
                            .map(|f| f.name.clone())
                            .unwrap_or_default(),
                    );
                    self.request_redraw();
                    return;
                }
                self.last_folder_click = Some((id, now));
                if self.editor.select_folder(id) {
                    self.request_redraw();
                }
                self.folder_drag = Some(id);
            }
            layers::CardHit::FxTab(i) => {
                ensure(self, i);
                // Same button-is-a-toggle rule the cog follows: the tab you
                // are already looking at closes the card.
                let showing =
                    self.card_open == Some(i) && self.card_tab == layers::CardTab::Effects;
                self.card_open = (!showing).then_some(i);
                self.card_tab = layers::CardTab::Effects;
                self.request_redraw();
            }
            layers::CardHit::FxToggle(i, id) => {
                if self.editor.toggle_effect(i, id) {
                    self.request_redraw();
                }
            }
            layers::CardHit::FxRemove(i, id) => {
                if self.editor.remove_effect(i, id) {
                    self.request_redraw();
                }
            }
            layers::CardHit::FxSlider(i, id, param, t) => {
                ensure(self, i);
                let v = self
                    .editor
                    .fx_of(i)
                    .find(id)
                    .and_then(|e| e.kind.params().get(param as usize))
                    .map(|s| s.min + t * (s.max - s.min));
                if let Some(v) = v {
                    self.fx_slider_drag = Some((i, id, param));
                    if self.editor.set_effect_param(i, id, param, v) {
                        self.request_redraw();
                    }
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
        if let Some(kind) = self.fx_drag.take() {
            // Dropped on a card: that layer gets the effect. Dropped
            // anywhere else: nothing, which is what a cancelled drag means.
            let target = self.fx_drop.take();
            if let Some(i) = target
                && self.editor.add_effect_to(i, kind)
            {
                // Show what just landed.
                self.editor.select(Some(i));
                self.card_open = Some(i);
                self.card_tab = layers::CardTab::Effects;
            }
            self.request_redraw();
            return;
        }
        if self.field_drag {
            // A selection drag inside the open field; the field stays open.
            self.field_drag = false;
            return;
        }
        if self.look_release() {
            // A look in the fly view, or the click on empty space that
            // dropped the selection: either way the release is spent.
            self.request_redraw();
            return;
        }
        self.material_drag = None;
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
        if self.gizmo_drag.take().is_some() {
            self.request_redraw();
        }
        self.editor.end_gesture();
        self.layer_drag = None;
        self.folder_drag = None;
        self.handle_drag = None;
        self.picker_drag = None;
        self.timeline_scrub = false;
        self.key_drag = None;
        self.clip_drag = None;
        self.loop_drag = None;
        self.panel_resize = false;
        if let Some((target, prop, _, moved)) = self.scrub_drag.take()
            && !moved
        {
            // A clean click (no drag) opens the field for typing.
            use crate::editor::Prop;
            let shown = match target {
                ScrubTarget::Folder(id) => self.editor.folder(id).map(|f| match prop {
                    Prop::Rotation => format!("{:.0}", f.rotation.to_degrees()),
                    Prop::Scale => format!("{:.2}", f.scale),
                    Prop::Y => format!("{:.0}", f.y),
                    _ => format!("{:.0}", f.x),
                }),
                ScrubTarget::Shape => self.editor.selected_props().map(|p| match prop {
                    Prop::X => format!("{:.0}", p.x),
                    Prop::Y => format!("{:.0}", p.y),
                    Prop::Rotation => format!("{:.0}", p.rotation.to_degrees()),
                    Prop::Z => format!("{:.0}", p.z),
                    Prop::Tilt => format!("{:.0}", p.tilt.to_degrees()),
                    Prop::Turn => format!("{:.0}", p.turn.to_degrees()),
                    Prop::Width => format!("{:.0}", p.w.unwrap_or(0.0)),
                    Prop::Height => format!("{:.0}", p.h.unwrap_or(0.0)),
                    Prop::Depth => format!("{:.0}", p.d.unwrap_or(0.0)),
                    _ => format!("{:.0}", p.size),
                }),
            };
            if let Some(shown) = shown {
                // Opens with the value selected, so typing replaces it.
                self.field_edit =
                    Some((target, prop, crate::textbox::TextBox::selecting_all(shown)));
                self.request_redraw();
            }
        }
        if self.slider_drag.take().is_some() || self.fx_slider_drag.take().is_some() {
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
                    TitleAction::Close => {
                        if self.confirm_discard(crate::project::Discard::Quit) {
                            event_loop.exit();
                        }
                    }
                }
            }
            self.request_redraw();
        } else if self.editor.mouse_up() {
            self.request_redraw();
        }
    }
}
