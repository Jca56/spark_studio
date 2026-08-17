//! Project lifecycle: worker-thread results (file picker, audio analysis)
//! and File > New. Split from input so the click dispatch stays readable.

use crate::{AppEvent, Studio, picker};

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
                        picker::Purpose::SaveShape => {
                            let file = if path_str.ends_with(".sparkshape") {
                                path_str
                            } else {
                                format!("{path_str}.sparkshape")
                            };
                            self.editor.save_shape(&file);
                        }
                        picker::Purpose::ImportShape => {
                            self.editor.import_shapes(&path_str);
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
                            // No output device is survivable: the timeline
                            // still scrubs and keys, it just can't play.
                            Err(e) => println!(
                                "playback unavailable ({e}) — scrubbing and keyframing still work"
                            ),
                        }
                        // Open on 16 bars: a phrase and a half, with the
                        // quarter-note lines still visible.
                        self.time_view =
                            crate::timeline::TimeView::bars(&track.beat, track.duration, 16.0);
                        // Keys stamped before the bar-1 origin existed would
                        // hide behind the sidebar — pull them up to it.
                        self.editor.clamp_keys_to(track.beat.first_bar);
                        // A new track means a new grid — drop the old loop.
                        self.loop_region = None;
                        self.loop_on = false;
                        self.loop_drag = None;
                        self.audio = Some(track);
                        // A tempo the user typed outranks the estimate, and
                        // survives reopening the comp.
                        self.apply_bpm_override();
                        self.apply_loop();
                        self.audio_file = Some(path);
                    }
                    Err(e) => println!("audio import failed: {e}"),
                }
                self.request_redraw();
            }
        }
    }

    /// File > New: a blank comp, no track — a fresh page.
    pub(crate) fn new_project(&mut self) {
        self.editor.new_project();
        self.current_file = "untitled.spark".to_string();
        self.audio = None;
        self.player = None;
        self.audio_file = None;
        self.selected_keys.clear();
        self.key_drag = None;
        self.loop_region = None;
        self.loop_on = false;
        self.loop_drag = None;
        self.time_view = crate::timeline::TimeView::new(0.0, 1.0);
        self.lanes_scroll = 0.0;
        // No player left to hold the clock — rewind the editor's own.
        self.editor.set_time(0.0);
        println!("new project");
    }
}
