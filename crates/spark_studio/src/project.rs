//! Project lifecycle: worker-thread results (file picker, audio analysis,
//! the export's FFmpeg), File > New, the canvas's size, and the export
//! as the studio runs it. Split from input so the click dispatch stays
//! readable.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use spark_render::Scene;

use crate::{AppEvent, Studio, export, picker};

/// How long one redraw may spend rendering export frames before the
/// editor gets a turn — the status strip has to move too.
const EXPORT_SLICE: Duration = Duration::from_millis(40);

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
                            self.after_open();
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
                            self.sync_meshes();
                        }
                        picker::Purpose::ImportMesh => self.import_mesh(path),
                        picker::Purpose::PlaceComp => self.place_comp(path),
                        picker::Purpose::ExportVideo => self.start_export(path),
                    }
                }
                self.request_redraw();
            }
            AppEvent::Exported(result) => {
                self.export = None;
                let note = match result {
                    Ok(path) => format!("Exported: {path}"),
                    Err(e) => format!("Export failed: {e}"),
                };
                println!("{note}");
                self.export_note = Some(note);
                self.request_redraw();
            }
            AppEvent::MeshLoaded(id, path, result) => {
                self.mesh_loaded(id, path, result);
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
                        // The track's cursor is the clock from here on.
                        self.silent_play = None;
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
        self.canvas_view.reset(self.editor.canvas());
        self.subcomps.clear();
        self.selected_clip = None;
        self.clip_drag = None;
        self.last_clip_click = None;
        self.current_file = crate::editor::UNTITLED.to_string();
        self.audio = None;
        self.player = None;
        self.silent_play = None;
        self.audio_file = None;
        self.meshes.clear();
        self.selected_keys.clear();
        self.key_drag = None;
        self.loop_region = None;
        self.loop_drag = None;
        self.loop_on = false;
        // Back to the silent comp's own clock — the timeline stays up, so a
        // blank project can be choreographed before a track exists.
        self.time_view = crate::timeline::TimeView::bars(&self.grid(), self.duration(), 16.0);
        self.lanes_scroll = 0.0;
        // No player left to hold the clock — rewind the editor's own.
        self.editor.set_time(0.0);
        println!("new project");
    }
}

impl Studio {
    /// Canvas > a preset: resize the comp and look at it fresh.
    pub(crate) fn set_canvas(&mut self, canvas: [f32; 2]) {
        if self.editor.set_canvas(canvas) {
            self.canvas_view.reset(self.editor.canvas());
        }
    }

    /// Whether an export still has frames to render — what keeps the
    /// redraw loop turning. Once every frame is with FFmpeg the loop
    /// rests until it reports back.
    pub(crate) fn exporting(&self) -> bool {
        self.export.as_ref().is_some_and(|j| !j.rendered_all())
    }

    /// File > Export Video...: the loop region if one is set, otherwise
    /// the whole comp, at the canvas's size, with the song if there is
    /// one. The transport stops first — the export owns the clock now.
    pub(crate) fn start_export(&mut self, path: PathBuf) {
        if self.export.is_some() {
            println!("an export is already running");
            return;
        }
        let mut file = path.to_string_lossy().into_owned();
        if !file.ends_with(".mp4") {
            file.push_str(".mp4");
        }
        if self.playing() {
            self.toggle_play();
        }
        let range = match self.loop_region {
            Some((a, b)) if b > a => (a, b),
            _ => (0.0, self.duration()),
        };
        let Some(gpu) = &self.gpu else { return };
        let audio = self.audio.as_ref().and(self.audio_file.as_deref());
        let proxy = self.proxy.clone();
        let note = match export::Job::start(
            &gpu.device,
            &gpu.queue,
            gpu.surface_format(),
            self.editor.canvas(),
            range,
            audio,
            file,
            move |result| {
                let _ = proxy.send_event(AppEvent::Exported(result));
            },
        ) {
            Ok(job) => {
                self.export = Some(job);
                None
            }
            Err(e) => {
                println!("export failed: {e}");
                Some(format!("Export failed: {e}"))
            }
        };
        self.export_note = note;
        self.request_redraw();
    }

    /// Esc: stop the export, kill FFmpeg, remove the half-file.
    pub(crate) fn cancel_export(&mut self) -> bool {
        match &mut self.export {
            Some(job) => {
                job.cancel();
                println!("export cancelled");
                true
            }
            None => false,
        }
    }

    /// Render export frames for a slice of this redraw. Each frame poses
    /// the document at its own time, draws it through the export's stage
    /// with none of the editor's marks, and hands it on; the playhead
    /// goes back where it was so the editor's own frame is unchanged.
    pub(crate) fn export_tick(&mut self) {
        let Some(job) = &mut self.export else { return };
        let Some(gpu) = &self.gpu else { return };
        if job.rendered_all() {
            return;
        }
        let keep = self.editor.time();
        let camera = job.camera();
        let slice = Instant::now();
        while !job.rendered_all() && slice.elapsed() < EXPORT_SLICE {
            self.editor.set_time(job.next_time());
            self.editor.sync_to_time();
            let assembled = crate::scene::assemble(
                &self.editor,
                self.audio.as_ref(),
                &self.meshes,
                &self.subcomps,
                &camera,
                Vec::new(),
                Vec::new(),
                false,
            );
            let scene = Scene {
                shapes: &assembled.shapes,
                models: &assembled.models,
                paths: &assembled.paths,
                meshes: &assembled.meshes,
                lights: &assembled.lights,
                camera: &camera,
                time: self.editor.time(),
                over: assembled.over,
            };
            job.render(&gpu.device, &gpu.queue, &scene);
        }
        if job.rendered_all() {
            println!("every frame rendered in {:.1}s; encoding...", job.elapsed());
        }
        self.editor.set_time(keep);
        self.editor.sync_to_time();
    }
}

impl Studio {
    /// Everything a freshly loaded document needs around it: the view
    /// re-centred on its canvas, its track, its meshes (asset ids are per
    /// comp — another comp's asset 1 is not this one's), and its placed
    /// comps read from disk. Shared by File > Open and by double-clicking
    /// a clip open.
    fn after_open(&mut self) {
        self.canvas_view.reset(self.editor.canvas());
        self.sync_audio();
        self.meshes.clear();
        self.sync_meshes();
        self.subcomps.clear();
        self.selected_clip = None;
        self.clip_drag = None;
        self.last_clip_click = None;
        self.sync_subcomps();
        self.request_redraw();
    }

    /// Read every placed comp the document names that isn't loaded yet.
    pub(crate) fn sync_subcomps(&mut self) {
        let want: Vec<(u32, String)> = self
            .editor
            .comp_assets()
            .iter()
            .filter(|a| !self.subcomps.contains_key(&a.id))
            .map(|a| (a.id, a.path.clone()))
            .collect();
        for (id, path) in want {
            self.load_subcomp(id, path);
        }
    }

    /// Parse one placed comp and send its meshes to the GPU under fresh
    /// global ids (see `comps::SUB_MESH_BASE`). A file that can't be read
    /// keeps its place on the arrangement and says so.
    fn load_subcomp(&mut self, id: u32, path: String) {
        let pc = match std::fs::read_to_string(&path) {
            Ok(text) => {
                let d = crate::doc::parse(&text);
                if !d.clips.is_empty() {
                    println!("note: {path} has clips of its own — nested clips don't play yet");
                }
                let mut map = Vec::new();
                for a in &d.assets {
                    let g = self.sub_mesh_next;
                    self.sub_mesh_next += 1;
                    map.push((a.id, g));
                    self.spawn_mesh_load(Some(g), PathBuf::from(&a.path));
                }
                println!(
                    "placed comp ready: {path} ({:.2}s loop)",
                    crate::comps::period_of(&d)
                );
                crate::comps::PlacedComp::new(path, d, map)
            }
            Err(e) => {
                println!("couldn't read placed comp {path}: {e}");
                crate::comps::PlacedComp::broken(path)
            }
        };
        self.subcomps.insert(id, pc);
    }

    /// File > Place Comp…: register the file and drop a one-period clip
    /// at the playhead on the first free track. Drag its right edge to
    /// loop it out.
    pub(crate) fn place_comp(&mut self, path: PathBuf) {
        let p = path.to_string_lossy().into_owned();
        let same = std::fs::canonicalize(&p)
            .ok()
            .zip(std::fs::canonicalize(&self.current_file).ok())
            .is_some_and(|(a, b)| a == b)
            || p == self.current_file;
        if same {
            // The recursion guard, at the door: a comp playing itself
            // would be turtles all the way down.
            println!("a comp can't place itself");
            return;
        }
        let id = self.editor.add_comp_asset(p);
        self.sync_subcomps();
        let period = self.subcomps.get(&id).map(|pc| pc.period).unwrap_or(1.0);
        let start = self
            .editor
            .time()
            .clamp(0.0, (self.duration() - period).max(0.0));
        let track = self.editor.free_track(start, period);
        let i = self.editor.place_clip(id, track, start, period);
        self.selected_clip = Some(i);
        self.timeline_tab = crate::timeline::Tab::Arrange;
        println!("clip placed — drag its right edge to loop it out");
    }

    /// Double-click a clip: open its comp for editing. The project stays
    /// on disk exactly as saved — when the edit is done, save here and
    /// File > Open the project again; its placed comps re-read then.
    pub(crate) fn open_clip_comp(&mut self, i: usize) {
        let Some(path) = self
            .editor
            .clips()
            .get(i)
            .and_then(|c| self.editor.comp_asset(c.comp))
            .map(|a| a.path.clone())
        else {
            return;
        };
        println!("opening placed comp — save it, then reopen the project from File > Open");
        self.editor.load(&path);
        self.current_file = path;
        self.after_open();
    }

    /// The Arrange tab's layout, for hit-testing and drawing alike.
    pub(crate) fn arrange_scene(
        &self,
        panel: &crate::timeline::Panel,
        scale: f32,
    ) -> crate::arrange::ArrangeScene {
        crate::arrange::build(
            panel,
            &self.time_view,
            scale,
            &self.editor,
            &self.subcomps,
            self.selected_clip,
            self.lanes_scroll,
        )
    }
}
