//! Project lifecycle: worker-thread results (file picker, audio analysis,
//! the export's FFmpeg), File > New, the canvas's size, and the export
//! as the studio runs it. Split from input so the click dispatch stays
//! readable.

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use crate::editor::Editor;
use crate::{AppEvent, Studio, doc, picker};

/// The project, parked whole while a placed comp is edited — popping
/// this is what makes Back instant and lossless.
pub(crate) struct Crumb {
    pub editor: Editor,
    pub file: String,
    pub baseline: String,
    pub meshes: HashMap<u32, crate::meshes::MeshAssetGpu>,
    pub subcomps: HashMap<u32, crate::comps::PlacedComp>,
    pub canvas_view: crate::view::CanvasView,
    pub selected_clips: Vec<crate::arrange::ClipRef>,
}

/// Which gesture is asking to throw unsaved work away.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Discard {
    Quit,
    New,
    Open,
}


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
                            // Wherever the breadcrumb was, an open lands
                            // at the top of a fresh project.
                            self.comp_stack.clear();
                            let session = self.editor.load(&path_str);
                            self.current_file = path_str;
                            self.after_open(session);
                        }
                        picker::Purpose::SaveComp => {
                            let file = if path_str.ends_with(".spark") {
                                path_str
                            } else {
                                format!("{path_str}.spark")
                            };
                            self.save_project(&file);
                            self.current_file = file;
                        }
                        picker::Purpose::ImportAudio => {
                            // The song belongs to the comp: remembered on
                            // save, reloaded on open. Its clip stays where
                            // it was placed; a first song plays whole from
                            // the top.
                            if self.in_comp() {
                                let note = "Audio belongs to the project — go Back first".to_string();
                                println!("{note}");
                                self.export_note = Some(note);
                            } else {
                                self.editor.set_audio_path(Some(path_str));
                                if !self.editor.asset_placed(crate::doc::SONG) {
                                    self.editor.place_audio(crate::doc::SONG, 0.0, 0.0);
                                }
                                self.import_audio(path);
                            }
                        }
                        picker::Purpose::ImportSound => self.import_sound(path),
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
                        picker::Purpose::Relink(src) => self.relink(src, path),
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
            AppEvent::SoundLoaded(id, path, result) => self.sound_loaded(id, path, result),
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
                        // A new track means a new grid — drop the old loop.
                        self.loop_region = None;
                        self.loop_on = false;
                        self.loop_drag = None;
                        self.audio = Some(track);
                        self.song_missing(false);
                        // Open on 16 bars: a phrase and a half, with the
                        // quarter-note lines still visible. The device
                        // opens with the first voice (`sync_voices`).
                        self.time_view = crate::timeline::TimeView::bars(
                            &self.grid(),
                            crate::transport::OPEN_END,
                            16.0,
                        );
                        // A tempo the user typed outranks the estimate, and
                        // survives reopening the comp.
                        self.apply_bpm_override();
                        self.apply_loop();
                        self.editor.bar_s = 4.0 * 60.0 / self.grid().bpm.max(1.0);
                        self.audio_file = Some(path);
                        // A reopened project gets its parked loop and
                        // playhead back, now that the grid they mean
                        // exists again.
                        if let Some(s) = self.restore_session.take() {
                            self.apply_session(s);
                        }
                    }
                    Err(e) => {
                        println!("audio import failed: {e}");
                        self.song_missing(true);
                    }
                }
                self.request_redraw();
            }
        }
    }

    /// File > New: a blank comp, no track — a fresh page.
    pub(crate) fn new_project(&mut self) {
        self.editor.new_project();
        self.comp_stack.clear();
        self.saved_baseline = doc::serialize(&self.editor.to_doc());
        self.canvas_view.reset(self.editor.canvas());
        self.subcomps.clear();
        self.selected_clips.clear();
        self.clip_drag = None;
        self.last_clip_click = None;
        self.current_file = crate::editor::UNTITLED.to_string();
        self.audio = None;
        self.player = None;
        self.player_failed = false;
        self.voices_key = None;
        self.sounds.clear();
        self.vol_drag = None;
        self.silent_play = None;
        self.audio_file = None;
        self.meshes.clear();
        self.mesh_missing.clear();
        self.loop_region = None;
        self.loop_drag = None;
        self.loop_on = false;
        // Back to the silent comp's own clock — the timeline stays up, so a
        // blank project can be choreographed before a track exists.
        self.time_view = crate::timeline::TimeView::bars(
            &self.grid(),
            crate::transport::OPEN_END,
            16.0,
        );
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
}

impl Studio {
    /// Everything a freshly loaded document needs around it: the view
    /// re-centred on its canvas, its track, its meshes (asset ids are per
    /// comp — another comp's asset 1 is not this one's), its placed comps
    /// read from disk — and where work left off, applied. The tab comes
    /// back at once; the playhead and loop wait for the track, whose
    /// arrival resets them (or apply now, when no track is coming).
    fn after_open(&mut self, session: doc::Session) {
        self.saved_baseline = doc::serialize(&self.editor.to_doc());
        self.canvas_view.reset(self.editor.canvas());
        self.sync_audio();
        self.sounds.clear();
        self.sync_sounds();
        self.meshes.clear();
        self.mesh_missing.clear();
        self.sync_meshes();
        self.subcomps.clear();
        self.selected_clips.clear();
        self.clip_drag = None;
        self.last_clip_click = None;
        self.clear_doc_ui_state();
        self.sync_subcomps();
        if self.audio_loading.is_some() {
            self.restore_session = Some(session);
        } else {
            self.apply_session(session);
        }
        self.request_redraw();
    }

    /// Land where the file says work left off.
    fn apply_session(&mut self, s: doc::Session) {
        if let Some((a, b, on)) = s.loop_region {
            self.loop_region = Some((a, b));
            self.loop_on = on;
            self.apply_loop();
        }
        if let Some(t) = s.playhead {
            self.seek(t.max(0.0));
        }
        // The timeline's modes come back the way they were left; a file
        // without them keeps whatever the session has.
        if let Some(on) = s.snap {
            self.snap_playhead = on;
        }
        if let Some(on) = s.wave {
            self.wave_overlay = on;
        }
        if let Some(g) = s.grid.and_then(crate::timeline::Grid::from_per_bar) {
            self.grid_div = g;
        }
    }

    /// Everything that points into the document by index or id, cleared —
    /// what swapping the document out from under the studio requires.
    fn clear_doc_ui_state(&mut self) {
        self.clip_view = None;
        self.selected_clips.clear();
        self.vol_drag = None;
        self.row_drag = None;
        self.rows_seen = 0;
        self.clip_drag = None;
        self.last_clip_click = None;
    }

    /// Whether the document differs from its last save. Session state
    /// (playhead, loop, tab) is outside both sides of the comparison, so
    /// scrubbing never stars the title.
    pub(crate) fn is_dirty(&self) -> bool {
        doc::serialize(&self.editor.to_doc()) != self.saved_baseline
    }

    /// The same question across the whole breadcrumb: the doc in hand,
    /// and every project parked on the stack under it.
    fn any_dirty(&self) -> bool {
        self.is_dirty()
            || self
                .comp_stack
                .iter()
                .any(|c| doc::serialize(&c.editor.to_doc()) != c.baseline)
    }

    /// Quit / New / Open with unsaved work: the first gesture says so in
    /// the status strip, the same gesture again within six seconds means
    /// it. There is no dialog machinery in this editor, and a two-beat
    /// confirm in the strip is honest without one.
    pub(crate) fn confirm_discard(&mut self, what: Discard) -> bool {
        if !self.any_dirty() {
            self.pending_discard = None;
            return true;
        }
        if self
            .pending_discard
            .take()
            .is_some_and(|(w, t)| w == what && t.elapsed() < Duration::from_secs(6))
        {
            return true;
        }
        self.pending_discard = Some((what, Instant::now()));
        let verb = match what {
            Discard::Quit => "quit",
            Discard::New => "New",
            Discard::Open => "Open",
        };
        let note = format!("Unsaved changes — Ctrl+S saves; {verb} again within 6s discards");
        println!("{note}");
        self.export_note = Some(note);
        self.request_redraw();
        false
    }

    /// Save `path` with where work left off riding along, and reset the
    /// dirty baseline. Every save in the app comes through here.
    pub(crate) fn save_project(&mut self, path: &str) {
        let session = doc::Session {
            loop_region: self.loop_region.map(|(a, b)| (a, b, self.loop_on)),
            playhead: Some(self.editor.time()),
            snap: Some(self.snap_playhead),
            wave: Some(self.wave_overlay),
            grid: Some(self.grid_div.per_bar() as u32),
        };
        self.editor.save(path, &session);
        self.saved_baseline = doc::serialize(&self.editor.to_doc());
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
        let start = self.editor.time().max(0.0);
        let track = self.editor.free_track(start, period);
        let i = self.editor.place_clip(id, track, start, period);
        self.selected_clips = vec![crate::arrange::ClipRef::Comp(i)];
        println!("clip placed — drag its right edge to loop it out");
    }

    /// Double-click a clip: step *into* its comp. The project doesn't go
    /// anywhere — it parks whole on the breadcrumb stack, unsaved changes
    /// and all — and the title turns into `project > comp`; clicking that
    /// is the way back. The song keeps playing where it is: the comp is
    /// edited against the project's track and grid, the way a clip is
    /// edited inside a Live set.
    pub(crate) fn open_clip_comp(&mut self, i: usize) {
        let Some(path) = self
            .editor
            .comp_clips()
            .get(i)
            .and_then(|c| self.editor.comp_asset(c.comp))
            .map(|a| a.path.clone())
        else {
            return;
        };
        if !std::path::Path::new(&path).exists() {
            println!("can't open {path}: the file is missing");
            return;
        }
        let crumb = Crumb {
            editor: std::mem::replace(&mut self.editor, Editor::empty()),
            file: std::mem::take(&mut self.current_file),
            baseline: std::mem::take(&mut self.saved_baseline),
            meshes: std::mem::take(&mut self.meshes),
            subcomps: std::mem::take(&mut self.subcomps),
            canvas_view: std::mem::take(&mut self.canvas_view),
            selected_clips: std::mem::take(&mut self.selected_clips),
        };
        self.comp_stack.push(crumb);
        // The comp's own parked session is ignored: the song, the loop
        // and the playhead are the project's right now.
        let _ = self.editor.load(&path);
        self.current_file = path;
        self.saved_baseline = doc::serialize(&self.editor.to_doc());
        self.canvas_view.reset(self.editor.canvas());
        self.sync_meshes();
        self.sync_subcomps();
        self.clear_doc_ui_state();
        println!("editing the comp — click the status bar's breadcrumb to go back");
        self.request_redraw();
    }

    /// The breadcrumb's Back: the comp auto-saves to its file — that is
    /// what the project re-reads — and the parked project comes back
    /// exactly as it was left, then re-reads the edited comp so every
    /// clip playing it shows the new version.
    pub(crate) fn leave_comp(&mut self) {
        let Some(crumb) = self.comp_stack.pop() else {
            return;
        };
        let edited = self.current_file.clone();
        self.save_project(&edited);
        self.editor = crumb.editor;
        self.current_file = crumb.file;
        self.saved_baseline = crumb.baseline;
        self.meshes = crumb.meshes;
        self.subcomps = crumb.subcomps;
        self.canvas_view = crumb.canvas_view;
        self.clear_doc_ui_state();
        self.selected_clips = crumb.selected_clips;
        self.reload_subcomp_at(&edited);
        self.request_redraw();
    }

    /// Drop and re-read every placed comp backed by `path`, GPU meshes
    /// included — the edited version is the one the arrangement plays.
    fn reload_subcomp_at(&mut self, path: &str) {
        let ids: Vec<u32> = self
            .editor
            .comp_assets()
            .iter()
            .filter(|a| a.path == path)
            .map(|a| a.id)
            .collect();
        for id in ids {
            if let Some(pc) = self.subcomps.remove(&id) {
                for (_, g) in pc.mesh_map {
                    self.meshes.remove(&g);
                }
            }
        }
        self.sync_subcomps();
    }

    /// Where this project's comps live: a `comps/` folder beside the
    /// project file. An unsaved project has no beside yet.
    fn comps_dir(&self) -> Option<PathBuf> {
        if self.current_file == crate::editor::UNTITLED {
            return None;
        }
        Some(std::path::Path::new(&self.current_file).parent()?.join("comps"))
    }

    /// File > New Comp: a fresh empty comp file beside the project, a
    /// one-bar clip of it at the playhead, and straight in to draw.
    pub(crate) fn new_comp(&mut self) {
        let Some(dir) = self.comps_dir() else {
            let note = "Save the project first — comps live beside it".to_string();
            println!("{note}");
            self.export_note = Some(note);
            self.request_redraw();
            return;
        };
        if let Err(e) = std::fs::create_dir_all(&dir) {
            println!("couldn't make {}: {e}", dir.display());
            return;
        }
        let mut n = 1;
        let path = loop {
            let p = dir.join(format!("comp-{n}.spark"));
            if !p.exists() {
                break p;
            }
            n += 1;
        };
        let bar = 4.0 * 60.0 / self.grid().bpm.max(1.0);
        let d = doc::Doc {
            canvas: self.editor.canvas(),
            duration: Some(bar),
            ..Default::default()
        };
        if let Err(e) = std::fs::write(&path, doc::serialize(&d)) {
            println!("couldn't write {}: {e}", path.display());
            return;
        }
        let p = path.to_string_lossy().into_owned();
        let id = self.editor.add_comp_asset(p);
        self.sync_subcomps();
        let start = self.editor.time();
        let track = self.editor.free_track(start, bar);
        let i = self.editor.place_clip(id, track, start, bar);
        self.selected_clips = vec![crate::arrange::ClipRef::Comp(i)];
        self.open_clip_comp(i);
    }

    /// Ctrl+Shift+C: Make Comp from Selection. The file lands beside the
    /// project, named after the primary layer; the clip lands exactly
    /// where the selection's motion was (see `editor::precompose`).
    // No longer bound: Make Comp left the context menu and `Ctrl+Shift+C`
    // at Alva's ask (2026-08-31); the File-menu comp flow's fate is theirs.
    #[allow(dead_code)]
    pub(crate) fn make_comp_from_selection(&mut self) -> bool {
        if self.editor.selection().is_empty() {
            println!("select something to make a comp of");
            return false;
        }
        let Some(dir) = self.comps_dir() else {
            let note = "Save the project first — comps live beside it".to_string();
            println!("{note}");
            self.export_note = Some(note);
            self.request_redraw();
            return true;
        };
        if let Err(e) = std::fs::create_dir_all(&dir) {
            println!("couldn't make {}: {e}", dir.display());
            return false;
        }
        let base: String = self
            .editor
            .primary()
            .map(|i| self.editor.display_name(i))
            .unwrap_or_else(|| "comp".into())
            .to_lowercase()
            .chars()
            .map(|c| if c.is_alphanumeric() { c } else { '-' })
            .collect();
        let mut n = 0;
        let path = loop {
            let name = if n == 0 {
                format!("{base}.spark")
            } else {
                format!("{base}-{n}.spark")
            };
            let p = dir.join(name);
            if !p.exists() {
                break p;
            }
            n += 1;
        };
        let bar = 4.0 * 60.0 / self.grid().bpm.max(1.0);
        let p = path.to_string_lossy().into_owned();
        match self.editor.precompose(&p, self.editor.time(), bar) {
            Some(clip) => {
                self.sync_subcomps();
                self.selected_clips = vec![crate::arrange::ClipRef::Comp(clip)];
                true
            }
            None => false,
        }
    }

}
