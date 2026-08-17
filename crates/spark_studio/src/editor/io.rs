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
        }
        self.mark_posed_selection();
        println!("pasted style to {} shape(s)", self.selection.len());
        true
    }

    /// The layer's user-given name ("" = auto-label).
    pub fn name(&self, i: usize) -> &str {
        self.names.get(i).map(String::as_str).unwrap_or("")
    }

    pub fn names(&self) -> &[String] {
        &self.names
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
        self.selection.clear();
        self.audio_path = None;
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
        let text = doc::serialize(
            &posed,
            &self.paths,
            &self.names,
            &self.anim,
            &self.react,
            &self.group,
            &self.hidden,
            self.audio_path.as_deref(),
        );
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
        let (shapes, paths, names, anim, react, group, hidden, audio) = doc::parse(&text);
        println!("loaded {} shapes from {path}", shapes.len());
        let s = self.snap();
        self.history.push(s);
        self.shapes = shapes;
        self.paths = paths;
        self.names = names;
        self.anim = anim;
        self.react = react;
        self.group = group;
        self.hidden = hidden;
        self.audio_path = audio;
        self.selection.clear();
        self.drag = None;
        self.clear_posed();
        self.key_clip = None;
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
        }
        let anims = vec![ShapeAnim::default(); shapes.len()];
        let text = doc::serialize(
            &shapes, &paths, &names, &anims, &reacts, &groups, &hiddens, None,
        );
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
        let (shapes, paths, names, anims, reacts, groups, hiddens, _) = doc::parse(&text);
        if shapes.is_empty() {
            println!("no shapes in {path}");
            return false;
        }
        let s = self.snap();
        self.history.push(s);
        let path_base = self.paths.len();
        // Imported group ids land above every id already in the comp.
        let group_base = self.group.iter().copied().max().unwrap_or(0);
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
        }
        self.paths.extend(paths);
        self.selection = (start..self.shapes.len()).collect();
        println!(
            "imported {} shape(s) from {path}",
            self.shapes.len() - start
        );
        true
    }
}
