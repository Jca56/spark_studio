//! Pointing an asset at a new file. The asset keeps its id, so every
//! object, clip and volume that names it is untouched — only the path
//! changes, undoably. Same path twice is nothing.

use super::Editor;

impl Editor {
    /// A mesh asset's file.
    pub fn relink_mesh(&mut self, id: u32, path: String) -> bool {
        let Some(k) = self.assets.iter().position(|a| a.id == id) else {
            return false;
        };
        if self.assets[k].path == path {
            return false;
        }
        let s = self.snap();
        self.history.push(s);
        self.assets[k].path = path;
        true
    }

    /// A placed comp's file.
    pub fn relink_comp(&mut self, id: u32, path: String) -> bool {
        let Some(k) = self.comp_assets.iter().position(|a| a.id == id) else {
            return false;
        };
        if self.comp_assets[k].path == path {
            return false;
        }
        let s = self.snap();
        self.history.push(s);
        self.comp_assets[k].path = path;
        true
    }

    /// A sound's file.
    pub fn relink_sound(&mut self, id: u32, path: String) -> bool {
        let Some(k) = self.sounds.iter().position(|s| s.id == id) else {
            return false;
        };
        if self.sounds[k].path == path {
            return false;
        }
        let s = self.snap();
        self.history.push(s);
        self.sounds[k].path = path;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A relink changes the path and nothing else, undoes, and refuses
    /// an id it doesn't know or a path it already has.
    #[test]
    fn a_relink_keeps_the_id_and_undoes() {
        let mut e = Editor::empty();
        let m = e.add_asset("/old/logo.glb".into());
        let c = e.add_comp_asset("/old/spin.spark".into());
        let s = e.add_sound("/old/vo.wav".into());
        assert!(e.relink_mesh(m, "/new/logo.glb".into()));
        assert!(e.relink_comp(c, "/new/spin.spark".into()));
        assert!(e.relink_sound(s, "/new/vo.wav".into()));
        assert_eq!(e.assets()[0].path, "/new/logo.glb");
        assert_eq!(e.assets()[0].id, m, "the id the objects name is kept");
        assert_eq!(e.comp_asset(c).unwrap().path, "/new/spin.spark");
        assert_eq!(e.sound(s).unwrap().path, "/new/vo.wav");
        assert!(!e.relink_mesh(m, "/new/logo.glb".into()), "same path: nothing");
        assert!(!e.relink_mesh(99, "/x".into()), "unknown asset: nothing");
        e.undo();
        assert_eq!(e.sound(s).unwrap().path, "/old/vo.wav");
        e.undo();
        assert_eq!(e.comp_asset(c).unwrap().path, "/old/spin.spark");
        e.undo();
        assert_eq!(e.assets()[0].path, "/old/logo.glb");
    }
}
