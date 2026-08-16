//! Mouse press/release dispatch: the File menu, title bar, toolbar, side
//! panels, then the canvas — first hit wins the click.

use spark_ui::TitleAction;
use winit::event_loop::ActiveEventLoop;

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
        if let Some(m) = self.file_menu() {
            if m.hit_anchor(cx, cy) {
                self.menu_open = !self.menu_open;
                self.menu_hover = None;
                self.request_redraw();
                return;
            }
            if self.menu_open {
                // An open menu owns the click: act on a row, close on
                // anything else, swallow it either way.
                let item = m.hit_item(cx, cy);
                self.menu_open = false;
                self.request_redraw();
                match item {
                    Some(0) => self.spawn_picker(picker::Purpose::OpenComp),
                    Some(1) => self.editor.save(&self.current_file),
                    Some(2) => self.spawn_picker(picker::Purpose::SaveComp),
                    Some(3) => self.spawn_picker(picker::Purpose::ImportAudio),
                    Some(4) => event_loop.exit(),
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
                self.editor.selection(),
            );
            if let Some(i) = layers::hit(&rows, cx, cy) {
                if self.editor.select(Some(i)) {
                    self.request_redraw();
                }
                self.layer_drag = Some(i);
                return;
            }
            if let Some(props) = self.editor.selected_props() {
                let insp = inspector::build(layout.left, self.scale(), &props);
                if let Some(hit) = insp.hit(cx, cy) {
                    let dirty = match hit {
                        inspector::Hit::Slider(prop, t) => {
                            self.slider_drag = Some(prop);
                            self.editor.set_prop(prop, inspector::value_for(prop, t))
                        }
                        inspector::Hit::Swatch(i) => self.editor.set_color_index(i),
                        inspector::Hit::Outline(on) => self.editor.set_outline(on),
                    };
                    if dirty {
                        self.request_redraw();
                    }
                    return;
                }
            }
        }
        let in_viewport = self.layout().is_some_and(|l| l.viewport.contains(cx, cy));
        if in_viewport {
            if self.editor.mouse_down() {
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
}
