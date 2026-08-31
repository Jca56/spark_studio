//! App-level keyboard dispatch: everything a key press can do (transport,
//! selection, clipboard, file shortcuts) plus the layer-rename field's
//! keyboard. Split from main so the event plumbing stays readable.

use winit::event_loop::ActiveEventLoop;
use winit::keyboard::{Key, NamedKey};

use crate::{Studio, lanes, picker, timeline};

impl Studio {
    pub(crate) fn key_input(&mut self, event_loop: &ActiveEventLoop, key: &Key) {
        if self.material_edit.is_some() {
            // A hex field owns the keyboard while it's up.
            if self.material_key(key) {
                self.request_redraw();
            }
            return;
        }
        if self.field_edit.is_some() {
            // A scrub field under edit owns the keyboard.
            if self.field_key(key) {
                self.request_redraw();
            }
            return;
        }
        if self.bpm_edit.is_some() {
            if self.bpm_key(key) {
                self.request_redraw();
            }
            return;
        }
        if self.rename.is_some() {
            // The rename field owns the keyboard while it's up.
            if self.rename_key(key) {
                self.request_redraw();
            }
            return;
        }
        let dirty = match key {
            Key::Named(NamedKey::Escape) => {
                if self.menu_open.take().is_some() || !self.selected_keys.is_empty() {
                    self.selected_keys.clear();
                    true
                } else {
                    self.editor.deselect()
                }
            }
            Key::Named(NamedKey::Delete) | Key::Named(NamedKey::Backspace) => {
                // Highlighted keyframes take the hit; shapes only
                // when no key is selected.
                if self.selected_keys.is_empty() {
                    self.editor.delete_selected()
                } else {
                    let keys = std::mem::take(&mut self.selected_keys);
                    self.editor.delete_keys_group(&keys)
                }
            }
            Key::Named(NamedKey::ArrowLeft) => self.jump_key(false),
            Key::Named(NamedKey::ArrowRight) => self.jump_key(true),
            Key::Named(NamedKey::Space) => self.toggle_play(),
            Key::Named(NamedKey::Tab) => self.toggle_orbit(),
            Key::Character(c) if c == " " => self.toggle_play(),
            Key::Character(c) => {
                let ctrl = self.modifiers.control_key();
                let key = c.to_lowercase();
                if ctrl && key == "q" {
                    event_loop.exit();
                    false
                } else if ctrl && key == "s" {
                    self.editor.save(&self.current_file);
                    false
                } else if ctrl && key == "o" {
                    self.spawn_picker(picker::Purpose::OpenComp);
                    false
                } else if ctrl && key == "0" {
                    // Ctrl+0: back to the resting canvas fit.
                    self.canvas_view.reset();
                    true
                } else if ctrl && key == "g" {
                    // Ctrl+G merges the selection; Shift dissolves it.
                    if self.modifiers.shift_key() {
                        self.editor.unmerge_selected()
                    } else {
                        self.editor.merge_selected()
                    }
                } else if ctrl && key == "n" && self.modifiers.shift_key() {
                    // Ctrl+Shift+N: wrap the selected layers in a folder.
                    self.editor.new_folder_from_selection()
                } else if !ctrl && key == "l" {
                    self.toggle_loop()
                } else if !ctrl && (key == "," || key == ".") {
                    self.nudge_key(if key == "." { 1.0 } else { -1.0 })
                } else if ctrl && key == "c" && !self.selected_keys.is_empty() {
                    // Selected lane keys claim Ctrl+C; the canvas
                    // style copy keeps it otherwise.
                    let keys = self.selected_keys.clone();
                    self.editor.copy_keys_multi(&keys)
                } else if ctrl && key == "v" && self.editor.has_key_clip() {
                    // Ctrl+V pastes once at the playhead (16th
                    // grid); Ctrl+Shift+V repeats bar-aligned,
                    // keeping the pattern's phase within its bar —
                    // to the loop region's end, or 4 bars without
                    // one — so repeats land exactly on the grid the
                    // source keys sat on.
                    let (beat, duration) = (self.grid(), self.duration());
                    let bases = if self.modifiers.shift_key() {
                        let bar_s = 4.0 * 60.0 / beat.bpm.max(1.0);
                        let (span, clip_base) = self.editor.key_clip_shape().unwrap_or((0.0, 0.0));
                        let period = (span / bar_s).ceil().max(1.0) * bar_s;
                        let phase = clip_base - timeline::bar_floor(clip_base, &beat);
                        let start = timeline::bar_floor(self.editor.time(), &beat) + phase;
                        let end = match (self.loop_on, self.loop_region) {
                            (true, Some((_, b))) if b > start => b,
                            _ => start + period * 4.0,
                        };
                        let mut bases = Vec::new();
                        let mut b = start.max(beat.first_bar);
                        while b < end - 0.001 && b <= duration {
                            bases.push(b);
                            b += period;
                        }
                        bases
                    } else {
                        vec![
                            lanes::quantize(self.editor.time(), &beat)
                                .clamp(beat.first_bar, duration),
                        ]
                    };
                    let max_t = duration;
                    match self.editor.paste_keys(&bases, max_t) {
                        Some(sel) => {
                            self.selected_keys = sel;
                            true
                        }
                        None => false,
                    }
                } else {
                    self.editor.char_key(&key, ctrl, self.modifiers.shift_key())
                }
            }
            _ => false,
        };
        if dirty {
            // `C` and `I` move the current color from outside the picker.
            self.sync_picker();
            self.request_redraw();
        }
    }

    /// Keyboard while a scrub field is being typed into. A real text field:
    /// the caret moves, Shift extends a selection, and typing replaces
    /// whatever is selected. Digits, minus and dot are the only characters a
    /// number box accepts. Enter commits, Esc cancels.
    fn field_key(&mut self, key: &Key) -> bool {
        let shift = self.modifiers.shift_key();
        if matches!(key, Key::Named(NamedKey::Escape)) {
            self.field_edit = None;
            return true;
        }
        if matches!(key, Key::Named(NamedKey::Enter)) {
            return self.commit_field_edit();
        }
        let Some((_, _, b)) = &mut self.field_edit else {
            return false;
        };
        match key {
            Key::Named(NamedKey::Backspace) => b.backspace(),
            Key::Named(NamedKey::Delete) => b.delete(),
            Key::Named(NamedKey::ArrowLeft) => {
                b.step(false, shift);
                true
            }
            Key::Named(NamedKey::ArrowRight) => {
                b.step(true, shift);
                true
            }
            Key::Named(NamedKey::Home) => {
                b.home(shift);
                true
            }
            Key::Named(NamedKey::End) => {
                b.end(shift);
                true
            }
            Key::Character(s) => {
                // Ctrl+A picks the whole value, like any other field.
                if self.modifiers.control_key() {
                    if s.to_lowercase() == "a" {
                        b.select_all();
                        return true;
                    }
                    return false;
                }
                let mut dirty = false;
                for c in s.chars() {
                    if b.text().len() < 12 && (c.is_ascii_digit() || c == '-' || c == '.') {
                        b.insert(c);
                        dirty = true;
                    }
                }
                dirty
            }
            _ => false,
        }
    }

    /// Keyboard while the transport's tempo field is up.
    fn bpm_key(&mut self, key: &Key) -> bool {
        match key {
            Key::Named(NamedKey::Escape) => {
                self.bpm_edit = None;
                true
            }
            Key::Named(NamedKey::Enter) => self.commit_bpm_edit(),
            Key::Named(NamedKey::Backspace) => {
                self.bpm_edit.as_mut().is_some_and(|b| b.pop().is_some())
            }
            Key::Character(s) => {
                let mut dirty = false;
                if let Some(b) = &mut self.bpm_edit {
                    for c in s.chars() {
                        if b.len() < 7 && (c.is_ascii_digit() || c == '.') {
                            b.push(c);
                            dirty = true;
                        }
                    }
                }
                dirty
            }
            _ => false,
        }
    }

    /// Apply a typed tempo. It overrides detection, rides the comp file, and
    /// keeps the downbeat where it was — the phase was found from the audio
    /// and retyping the tempo is no reason to throw it away.
    pub(crate) fn commit_bpm_edit(&mut self) -> bool {
        let Some(buf) = self.bpm_edit.take() else {
            return false;
        };
        let Ok(bpm) = buf.trim().parse::<f32>() else {
            return true;
        };
        if !(20.0..=400.0).contains(&bpm) {
            println!("BPM {bpm} is outside 20–400 — ignored");
            return true;
        }
        self.editor.set_bpm_override(Some(bpm));
        self.apply_bpm_override();
        println!("BPM set to {bpm}");
        true
    }

    /// Push the comp's tempo onto the loaded track, if there is one. Called
    /// when the number changes and again whenever a track finishes loading.
    pub(crate) fn apply_bpm_override(&mut self) {
        if let (Some(bpm), Some(track)) = (self.editor.bpm_override(), self.audio.as_mut()) {
            track.beat.bpm = bpm;
        }
    }

    /// Parse and apply a scrub field's typed value (rotation types in
    /// degrees), clamped to the property's range. Invalid input just
    /// closes the field.
    pub(crate) fn commit_field_edit(&mut self) -> bool {
        let Some((target, prop, buf)) = self.field_edit.take() else {
            return false;
        };
        let Ok(mut v) = buf.text().trim().parse::<f32>() else {
            return true;
        };
        // The three angles type in degrees.
        if matches!(
            prop,
            crate::editor::Prop::Rotation | crate::editor::Prop::Tilt | crate::editor::Prop::Turn
        ) {
            v = v.to_radians();
        }
        match target {
            crate::ScrubTarget::Folder(id) => {
                // Folder scale is a free multiplier; the shape ranges in
                // `fit` would clamp it to nonsense. Rotation is free too.
                self.editor.set_folder_prop(id, prop, v);
            }
            crate::ScrubTarget::Shape => {
                self.editor.set_prop(prop, crate::props::fit(prop, v));
            }
        }
        self.editor.end_gesture();
        true
    }

    /// Keyboard while a layer rename is active: Enter commits, Esc cancels,
    /// everything else edits the buffer. Returns whether to redraw.
    pub(crate) fn rename_key(&mut self, key: &Key) -> bool {
        match key {
            Key::Named(NamedKey::Escape) => {
                self.rename = None;
                self.rename_folder = None;
                true
            }
            Key::Named(NamedKey::Enter) => {
                if let Some(buf) = self.rename.take() {
                    self.commit_rename(buf);
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
