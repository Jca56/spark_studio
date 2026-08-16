//! Document-adjacent editor state: layer names, the comp's audio track,
//! and save/load through the `doc` format.

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
        });
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
            if let Some(o) = clip.outline {
                sh.set_outline(o);
            }
            if let Some(t) = clip.thickness {
                sh.set_thickness(t);
            }
        }
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

    pub fn save(&self, path: &str) {
        let text = doc::serialize(
            &self.shapes,
            &self.paths,
            &self.names,
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
        let (shapes, paths, names, audio) = doc::parse(&text);
        println!("loaded {} shapes from {path}", shapes.len());
        let s = self.snap();
        self.history.push(s);
        self.shapes = shapes;
        self.paths = paths;
        self.names = names;
        self.audio_path = audio;
        self.selection.clear();
        self.drag = None;
    }
}
