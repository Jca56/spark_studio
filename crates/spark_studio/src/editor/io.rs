//! Document-adjacent editor state: layer names, the comp's audio track,
//! and save/load through the `doc` format.

use crate::anim::ShapeAnim;
use crate::doc;
use crate::props::StyleClip;

use super::Editor;

impl Editor {
    /// Ctrl+C: remember the primary's look (never its geometry).
    pub fn copy_style(&mut self) -> bool {
        let Some(i) = self.primary() else {
            return false;
        };
        let s = &self.shapes[i];
        self.style_clip = Some(StyleClip {
            rgb: s.rgb(),
            intensity: s.brightness(),
            glow: s.glow_radius(),
            thickness: s.thickness(),
            outline: s.outline(),
            additive: s.additive(),
            gradient: s.gradient(),
            rgb2: s.rgb2(),
            density: s.density(),
            twinkle: s.twinkle(),
            twinkle_rate: s.twinkle_rate(),
            star_form: s.star_form(),
        });
        // Most recent copy wins Ctrl+V.
        self.key_clip = None;
        println!("copied style");
        false
    }

    /// Ctrl+V: apply the remembered look to every selected shape.
    pub fn paste_style(&mut self) -> bool {
        let Some(clip) = self.style_clip.clone() else {
            return false;
        };
        if self.selection.is_empty() {
            return false;
        }
        let snap = self.snap();
        self.history.push(snap);
        for &i in &self.selection {
            let sh = &mut self.shapes[i];
            sh.set_rgb(clip.rgb);
            sh.set_brightness(clip.intensity);
            sh.set_glow(clip.glow);
            sh.set_additive(clip.additive);
            sh.set_gradient(clip.gradient);
            sh.set_rgb2(clip.rgb2);
            if let Some(o) = clip.outline {
                sh.set_outline(o);
            }
            if let Some(t) = clip.thickness {
                sh.set_thickness(t);
            }
            // The scatter itself is geometry, not look: the seed stays put,
            // so pasting a style onto a field restyles the sky you have
            // rather than swapping it for the one you copied from.
            if let Some(n) = clip.density {
                sh.set_density(n);
            }
            if let Some(v) = clip.twinkle {
                sh.set_twinkle(v);
            }
            if let Some(v) = clip.twinkle_rate {
                sh.set_twinkle_rate(v);
            }
            if let Some(f) = clip.star_form {
                sh.set_star_form(f);
            }
        }
        self.mark_posed_selection();
        println!("pasted style to {} shape(s)", self.selection.len());
        true
    }

    /// The layer's user-given name ("" = auto-label).
    pub fn name(&self, i: usize) -> &str {
        self.names.get(i).map(String::as_str).unwrap_or("")
    }

    pub fn rename_primary(&mut self, name: String) -> bool {
        let Some(i) = self.primary() else {
            return false;
        };
        if self.names[i] == name {
            return false;
        }
        let s = self.snap();
        self.history.push(s);
        self.names[i] = name;
        true
    }

    pub fn audio_path(&self) -> Option<&str> {
        self.audio_path.as_deref()
    }

    pub fn set_audio_path(&mut self, path: Option<String>) {
        self.audio_path = path;
    }

    /// The tempo the user typed, if they have. Detection is a guess; this is
    /// the number the person who made the track knows.
    pub fn bpm_override(&self) -> Option<f32> {
        self.bpm_override
    }

    pub fn set_bpm_override(&mut self, bpm: Option<f32>) {
        self.bpm_override = bpm;
    }

    /// File > New: a blank comp. Undoable, like open.
    pub fn new_project(&mut self) {
        let s = self.snap();
        self.history.push(s);
        self.shapes.clear();
        self.paths.clear();
        self.names.clear();
        self.anim.clear();
        self.react.clear();
        self.group.clear();
        self.hidden.clear();
        self.folder.clear();
        self.folders.clear();
        self.selection.clear();
        self.audio_path = None;
        self.bpm_override = None;
        self.drag = None;
        self.clear_posed();
        self.key_clip = None;
    }

    pub fn save(&self, path: &str) {
        // Keyed properties bake to their t=0 pose so a save at any playhead
        // position writes identical bytes — comps keep diffing clean in git.
        let posed: Vec<_> = self
            .shapes
            .iter()
            .zip(&self.anim)
            .map(|(s, a)| {
                let mut c = *s;
                a.apply(&mut c, 0.0);
                c
            })
            .collect();
        let text = doc::serialize(&doc::Doc {
            shapes: posed,
            paths: self.paths.clone(),
            names: self.names.clone(),
            anims: self.anim.clone(),
            reacts: self.react.clone(),
            groups: self.group.clone(),
            hidden: self.hidden.clone(),
            folder: self.folder.clone(),
            folders: self.folders.clone(),
            audio: self.audio_path.clone(),
            bpm: self.bpm_override,
        });
        match std::fs::write(path, text) {
            Ok(()) => println!("saved {} shapes -> {path}", self.shapes.len()),
            Err(e) => println!("save failed: {e}"),
        }
    }

    pub fn load(&mut self, path: &str) {
        let text = match std::fs::read_to_string(path) {
            Ok(t) => t,
            Err(e) => {
                println!("load failed: {e}");
                return;
            }
        };
        let d = doc::parse(&text);
        println!("loaded {} shapes from {path}", d.shapes.len());
        let s = self.snap();
        self.history.push(s);
        self.shapes = d.shapes;
        self.paths = d.paths;
        self.names = d.names;
        self.anim = d.anims;
        self.react = d.reacts;
        self.group = d.groups;
        self.hidden = d.hidden;
        self.folder = d.folder;
        self.folders = d.folders;
        self.audio_path = d.audio;
        self.bpm_override = d.bpm;
        self.selection.clear();
        self.drag = None;
        self.clear_posed();
        self.key_clip = None;
        // Trust the file's shape order, but re-establish the invariant in
        // case it was hand-edited.
        self.normalize_folders();
    }

    /// File > Save Shape...: the selection, baked at t=0, written as a
    /// mini comp (no audio, no keys) for re-import into any project.
    pub fn save_shape(&self, path: &str) {
        if self.selection.is_empty() {
            println!("nothing selected to save");
            return;
        }
        let mut idx = self.selection.clone();
        idx.sort_unstable();
        idx.dedup();
        let mut shapes = Vec::new();
        let mut paths = Vec::new();
        let mut names = Vec::new();
        let mut reacts = Vec::new();
        let mut groups = Vec::new();
        let mut hiddens = Vec::new();
        let mut folder = Vec::new();
        for &i in &idx {
            let mut c = self.shapes[i];
            self.anim[i].apply(&mut c, 0.0);
            if let Some((id, _, _)) = c.path_meta() {
                c.set_path_start(paths.len());
                paths.push(self.paths.get(id).cloned().unwrap_or_default());
            }
            shapes.push(c);
            names.push(self.names[i].clone());
            reacts.push(self.react[i]);
            groups.push(self.group[i]);
            hiddens.push(self.hidden[i]);
            folder.push(self.folder[i]);
        }
        let folders: Vec<_> = self
            .folders
            .iter()
            .filter(|f| folder.contains(&f.id))
            .cloned()
            .collect();
        let anims = vec![ShapeAnim::default(); shapes.len()];
        let text = doc::serialize(&doc::Doc {
            shapes: shapes.clone(),
            paths,
            names,
            anims,
            reacts,
            groups,
            hidden: hiddens,
            folder,
            folders,
            // A saved shape carries no song, so it carries no tempo.
            audio: None,
            bpm: None,
        });
        match std::fs::write(path, text) {
            Ok(()) => println!("saved {} shape(s) -> {path}", shapes.len()),
            Err(e) => println!("shape save failed: {e}"),
        }
    }

    /// File > Import Shape...: append a saved shape file to the comp,
    /// grouping intact, and select what arrived.
    pub fn import_shapes(&mut self, path: &str) -> bool {
        let text = match std::fs::read_to_string(path) {
            Ok(t) => t,
            Err(e) => {
                println!("shape import failed: {e}");
                return false;
            }
        };
        let d = doc::parse(&text);
        let shapes = d.shapes;
        let (names, anims, reacts, groups, hiddens) =
            (d.names, d.anims, d.reacts, d.groups, d.hidden);
        if shapes.is_empty() {
            println!("no shapes in {path}");
            return false;
        }
        let s = self.snap();
        self.history.push(s);
        let path_base = self.paths.len();
        // Imported group ids land above every id already in the comp.
        let group_base = self.group.iter().copied().max().unwrap_or(0);
        let folder_base = self.folders.iter().map(|f| f.id).max().unwrap_or(0);
        let start = self.shapes.len();
        for (k, mut shape) in shapes.into_iter().enumerate() {
            if let Some((id, _, _)) = shape.path_meta() {
                shape.set_path_start(path_base + id);
            }
            self.shapes.push(shape);
            self.names.push(names.get(k).cloned().unwrap_or_default());
            self.anim.push(anims.get(k).cloned().unwrap_or_default());
            self.react.push(reacts.get(k).copied().unwrap_or([1.0; 3]));
            let g = groups.get(k).copied().unwrap_or(0);
            self.group.push(if g == 0 { 0 } else { group_base + g });
            self.hidden.push(hiddens.get(k).copied().unwrap_or(false));
            // Imported folder ids land above every id already in the comp.
            let f = d.folder.get(k).copied().unwrap_or(0);
            self.folder.push(if f == 0 { 0 } else { folder_base + f });
        }
        for f in d.folders {
            self.folders.push(crate::editor::Folder {
                id: folder_base + f.id,
                ..f
            });
        }
        self.paths.extend(d.paths);
        self.selection = (start..self.shapes.len()).collect();
        let n = self.shapes.len() - start;
        self.normalize_folders();
        println!("imported {n} shape(s) from {path}");
        true
    }
}
