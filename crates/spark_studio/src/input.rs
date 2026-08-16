//! Mouse press/release dispatch: the File menu, title bar, toolbar, side
//! panels, then the canvas — first hit wins the click.

use spark_ui::TitleAction;
use winit::event_loop::ActiveEventLoop;
use winit::keyboard::{Key, NamedKey};

use crate::{AppEvent, Studio, inspector, layers, picker};

impl Studio {
    /// Results arriving from worker threads (file picker, audio analysis).
    pub(crate) fn app_event(&mut self, event: AppEvent) {
        match event {
            AppEvent::Picked(purpose, path) => {
                self.picker_busy = false;
                if let Some(path) = path {
                    let path_str = path.to_string_lossy().into_owned();
                    match purpose {
                        picker::Purpose::OpenComp => {
                            self.editor.load(&path_str);
                            self.current_file = path_str;
                            self.sync_audio();
                        }
                        picker::Purpose::SaveComp => {
                            let file = if path_str.ends_with(".spark") {
                                path_str
                            } else {
                                format!("{path_str}.spark")
                            };
                            self.editor.save(&file);
                            self.current_file = file;
                        }
                        picker::Purpose::ImportAudio => {
                            // The track belongs to the comp: remembered on
                            // save, reloaded on open.
                            self.editor.set_audio_path(Some(path_str));
                            self.import_audio(path);
                        }
                    }
                }
                self.request_redraw();
            }
            AppEvent::AudioLoaded(path, result) => {
                self.audio_loading = None;
                match result {
                    Ok(track) => {
                        println!(
                            "audio ready: {:.1}s, ~{:.0} BPM, {} curve samples",
                            track.duration,
                            track.beat.bpm,
                            track.curves.bass.len()
                        );
                        match spark_audio::Player::new(track.samples.clone()) {
                            Ok(p) => self.player = Some(p),
                            Err(e) => println!("playback unavailable: {e}"),
                        }
                        self.audio = Some(track);
                        self.audio_file = Some(path);
                    }
                    Err(e) => println!("audio import failed: {e}"),
                }
                self.request_redraw();
            }
        }
    }

    pub(crate) fn press(&mut self, event_loop: &ActiveEventLoop) {
        let (cx, cy) = (self.cursor_px.0 as f32, self.cursor_px.1 as f32);
        if let Some(buf) = self.rename.take() {
            // Clicking away from an active rename commits it.
            self.editor.rename_primary(buf);
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
                    (0, Some(0)) => self.spawn_picker(picker::Purpose::OpenComp),
                    (0, Some(1)) => self.editor.save(&self.current_file),
                    (0, Some(2)) => self.spawn_picker(picker::Purpose::SaveComp),
                    (0, Some(3)) => self.spawn_picker(picker::Purpose::ImportAudio),
                    (0, Some(4)) => event_loop.exit(),
                    (1, Some(0)) => self.view_black = !self.view_black,
                    (1, Some(1)) => self.editor.snap_grid = !self.editor.snap_grid,
                    (1, Some(2)) => self.editor.smart_guides = !self.editor.smart_guides,
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
        if self.audio.is_some()
            && let Some(layout) = self.layout()
        {
            let strip = crate::timeline::strip(layout.timeline, self.scale());
            if strip.button.contains(cx, cy) {
                self.toggle_play();
                self.request_redraw();
                return;
            }
            if let (Some(t01), Some(track)) = (crate::timeline::seek_t(&strip, cx, cy), &self.audio)
            {
                if let Some(p) = &self.player {
                    p.seek(t01 * track.duration);
                }
                self.request_redraw();
                return;
            }
        }
        if let Some(layout) = self.layout() {
            let rows = layers::rows(
                layout.right,
                self.scale(),
                self.editor.shapes(),
                self.editor.names(),
                self.editor.selection(),
                self.layers_scroll,
            );
            if let Some(i) = layers::hit(&rows, layout.right, cx, cy) {
                let ctrl = self.modifiers.control_key();
                let now = std::time::Instant::now();
                let double = !ctrl
                    && self
                        .last_layer_click
                        .take()
                        .is_some_and(|(li, t)| li == i && now.duration_since(t).as_millis() < 400);
                if double {
                    // Double-click on a row: rename it in place.
                    self.editor.select(Some(i));
                    self.rename = Some(self.editor.name(i).to_string());
                    self.layer_drag = None;
                    self.request_redraw();
                    return;
                }
                self.last_layer_click = Some((i, now));
                let changed = if ctrl {
                    self.editor.toggle_select(i)
                } else {
                    self.editor.select(Some(i))
                };
                if changed {
                    self.request_redraw();
                }
                if !ctrl {
                    self.layer_drag = Some(i);
                }
                return;
            }
            if let Some(props) = self.editor.selected_props() {
                let insp = inspector::build(layout.left, self.scale(), &props, self.insp_scroll);
                if let Some(hit) = insp.hit(cx, cy) {
                    let dirty = match hit {
                        inspector::Hit::Slider(prop, t) => {
                            self.slider_drag = Some(prop);
                            self.editor.set_prop(prop, inspector::value_for(prop, t))
                        }
                        inspector::Hit::Swatch(i) => self.editor.set_color_index(i),
                        inspector::Hit::Outline(on) => self.editor.set_outline(on),
                        inspector::Hit::Blend(on) => self.editor.set_additive(on),
                    };
                    if dirty {
                        self.request_redraw();
                    }
                    return;
                }
                if insp.card.contains(cx, cy) && insp.panel.contains(cx, cy) {
                    // A miss inside the settings card is not a deselect.
                    return;
                }
            }
        }
        let in_viewport = self.layout().is_some_and(|l| l.viewport.contains(cx, cy));
        if in_viewport {
            if self.editor.mouse_down(self.modifiers.control_key()) {
                self.request_redraw();
            }
        } else if self.editor.deselect() {
            // Empty chrome is a neutral surface — clicking it drops the
            // selection, same as empty canvas.
            self.request_redraw();
        }
    }

    pub(crate) fn release(&mut self, event_loop: &ActiveEventLoop) {
        let (cx, cy) = (self.cursor_px.0 as f32, self.cursor_px.1 as f32);
        self.editor.end_gesture();
        self.layer_drag = None;
        if self.slider_drag.take().is_some() {
            return;
        }
        if let Some(pressed) = self.title_pressed.take() {
            let hit = self.title_bar().and_then(|tb| tb.hit(cx, cy));
            if hit == Some(pressed) {
                if let Some(window) = &self.window {
                    match pressed {
                        TitleAction::Minimize => window.set_minimized(true),
                        TitleAction::Maximize => window.set_maximized(!window.is_maximized()),
                        TitleAction::Close => event_loop.exit(),
                    }
                }
            }
            self.request_redraw();
        } else if self.editor.mouse_up() {
            self.request_redraw();
        }
    }

    /// Keyboard while a layer rename is active: Enter commits, Esc cancels,
    /// everything else edits the buffer. Returns whether to redraw.
    pub(crate) fn rename_key(&mut self, key: &Key) -> bool {
        match key {
            Key::Named(NamedKey::Escape) => {
                self.rename = None;
                true
            }
            Key::Named(NamedKey::Enter) => {
                if let Some(buf) = self.rename.take() {
                    self.editor.rename_primary(buf);
                }
                true
            }
            Key::Named(NamedKey::Backspace) => {
                self.rename.as_mut().is_some_and(|b| b.pop().is_some())
            }
            Key::Named(NamedKey::Space) => self.rename_push(' '),
            Key::Character(s) => {
                let chars: Vec<char> = s.chars().collect();
                let mut dirty = false;
                for c in chars {
                    dirty |= self.rename_push(c);
                }
                dirty
            }
            _ => false,
        }
    }

    fn rename_push(&mut self, c: char) -> bool {
        self.rename.as_mut().is_some_and(|b| {
            if b.len() < 24 && (c.is_alphanumeric() || " -_.".contains(c)) {
                b.push(c);
                true
            } else {
                false
            }
        })
    }
}
