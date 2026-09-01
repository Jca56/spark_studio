//! App-level keyboard dispatch: everything a key press can do (transport,
//! selection, clipboard, file shortcuts) plus the layer-rename field's
//! keyboard. Split from main so the event plumbing stays readable.

use winit::event_loop::ActiveEventLoop;
use winit::keyboard::{Key, NamedKey};

use crate::{Studio, picker};

impl Studio {
    pub(crate) fn key_input(&mut self, event_loop: &ActiveEventLoop, key: &Key) {
        if self.export.is_some() {
            // The keyboard is Esc and nothing else while a video renders.
            if matches!(key, Key::Named(NamedKey::Escape)) {
                self.cancel_export();
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
        // A field in the inspector being typed into owns the keyboard —
        // except that `K` on a *number* field commits it and stamps: the
        // click that listed the setting in the clip view also opened its
        // field, and the next thing Alva did was press K.
        if self.inspector_typing() {
            let k_on_number = matches!(key, Key::Character(c) if c.eq_ignore_ascii_case("k"))
                && matches!(
                    self.inspector.edit,
                    Some((crate::inspector::EditKey::Prop(_), _))
                );
            if k_on_number {
                self.inspector_commit();
                self.stamp();
                self.request_redraw();
                return;
            }
            if self.inspector_key(key) {
                self.request_redraw();
            }
            return;
        }
        let dirty = match key {
            Key::Named(NamedKey::Escape) => {
                if self.context_close() || self.popup_close() || self.menu_open.take().is_some() {
                    self.selected_clip = None;
                    true
                } else if self.close_clip_view() {
                    // Back to the arrangement; the clip stays selected.
                    true
                } else if self.selected_clip.take().is_some() {
                    true
                } else {
                    self.editor.deselect()
                }
            }
            Key::Named(NamedKey::Delete) | Key::Named(NamedKey::Backspace) => {
                // Inside the clip view Delete is the pick's — a key, or
                // a moment — never the clip or the object behind it.
                // On the arrangement a selected clip takes the hit first;
                // objects only when no clip is selected.
                if self.clip_view.is_some() {
                    self.clip_view_delete()
                } else if let Some(r) = self.selected_clip.take() {
                    match r {
                        crate::arrange::ClipRef::Obj { obj, c } => self
                            .editor
                            .index_of(obj)
                            .is_some_and(|i| self.editor.delete_obj_clip(i, c)),
                        crate::arrange::ClipRef::Comp(i) => self.editor.delete_clip(i),
                    }
                } else {
                    self.editor.delete_selected()
                }
            }
            Key::Named(NamedKey::Space) => self.toggle_play(),
            Key::Named(NamedKey::Tab) => self.toggle_fly(),
            Key::Character(c) if c == " " => self.toggle_play(),
            Key::Character(c) => {
                let ctrl = self.modifiers.control_key();
                let key = c.to_lowercase();
                if ctrl && key == "q" {
                    if self.confirm_discard(crate::project::Discard::Quit) {
                        event_loop.exit();
                    }
                    true
                } else if ctrl && key == "s" {
                    let file = self.current_file.clone();
                    self.save_project(&file);
                    true
                } else if ctrl && key == "o" {
                    if self.confirm_discard(crate::project::Discard::Open) {
                        self.spawn_picker(picker::Purpose::OpenComp);
                    }
                    true
                } else if ctrl && key == "0" {
                    // Ctrl+0: back to the resting canvas fit.
                    self.canvas_view.reset(self.editor.canvas());
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
                    // With an object clip selected, L is its loop toggle —
                    // the transport loop keeps the key otherwise.
                    if let Some(crate::arrange::ClipRef::Obj { obj, c }) = self.selected_clip {
                        match self.editor.index_of(obj) {
                            Some(i) => self.editor.toggle_obj_clip_loop(i, c),
                            None => self.toggle_loop(),
                        }
                    } else {
                        self.toggle_loop()
                    }
                } else if ctrl && key == "d"
                    && let Some(crate::arrange::ClipRef::Obj { obj, c }) = self.selected_clip
                {
                    // Ctrl+D on a selected clip: duplicate the clip flush
                    // after itself; the canvas keeps Ctrl+D for objects.
                    match self.editor.index_of(obj) {
                        Some(i) => match self.editor.duplicate_obj_clip(i, c) {
                            Some(nc) => {
                                self.selected_clip =
                                    Some(crate::arrange::ClipRef::Obj { obj, c: nc });
                                true
                            }
                            None => false,
                        },
                        None => false,
                    }
                } else if !ctrl && key == "k" {
                    // K: the stamp, shaped by the clip view when it's open.
                    self.stamp()
                } else if !ctrl && key == "r" {
                    // R: the gizmo's other half — arrows or rings.
                    self.gizmo_mode = self.gizmo_mode.toggled();
                    self.gizmo_hover = None;
                    true
                } else {
                    self.editor.char_key(&key, ctrl, self.modifiers.shift_key())
                }
            }
            _ => false,
        };
        if dirty {
            self.request_redraw();
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
        self.editor.bar_s = 4.0 * 60.0 / self.grid().bpm.max(1.0);
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

}
