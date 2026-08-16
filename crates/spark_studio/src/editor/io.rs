//! Document-adjacent editor state: layer names, the comp's audio track,
//! and save/load through the `doc` format.

use crate::doc;

use super::Editor;

impl Editor {
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
        let text = doc::serialize(&self.shapes, &self.names, self.audio_path.as_deref());
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
        let (shapes, names, audio) = doc::parse(&text);
        println!("loaded {} shapes from {path}", shapes.len());
        let s = self.snap();
        self.history.push(s);
        self.shapes = shapes;
        self.names = names;
        self.audio_path = audio;
        self.selection.clear();
        self.drag = None;
    }
}
