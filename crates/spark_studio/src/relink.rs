//! Relinking: pointing a track at its file again after the file moved.
//! A comp names files by path — meshes, placed comps, the song, every
//! other sound — and Alva tidies folders (2026-09-02: "a mesh I used
//! is no longer in the same spot"). A source that can't be read keeps
//! its place and says so in red; **right-click its row, its clip, or
//! the object on the canvas → Relink source…** picks the file, the
//! document takes the new path (undoably, where the path is document
//! state), and the file loads in where the old one was.

use std::path::PathBuf;

use crate::arrange::{ArrHit, ClipRef, RowKind};
use crate::doc::SONG;
use crate::timeline::Panel;
use crate::{Studio, picker};

/// A file a comp names, by what names it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Source {
    /// A mesh asset, by id.
    Mesh(u32),
    /// A placed comp, by asset id.
    Comp(u32),
    /// The song.
    Song,
    /// A sound asset, by id.
    Sound(u32),
}

impl Studio {
    /// The source behind what a right-click on the arrangement landed
    /// on: an object's mesh, an audio row's file, a comp clip's comp.
    /// `None` for anything that has no file behind it.
    pub(crate) fn source_at(&self, panel: &Panel, scale: f32, cx: f32, cy: f32) -> Option<Source> {
        let sc = self.arrange_scene(panel, scale);
        let of_object = |i: usize| {
            self.editor
                .shapes()
                .get(i)
                .and_then(|s| s.mesh_asset())
                .map(Source::Mesh)
        };
        let of_audio = |asset: u32| {
            if asset == SONG {
                Source::Song
            } else {
                Source::Sound(asset)
            }
        };
        match crate::arrange::hit(&sc, cx, cy, scale)? {
            ArrHit::Head(RowKind::Object(i)) => of_object(i),
            ArrHit::Clip(ClipRef::Obj { obj, .. }, _) => self.editor.index_of(obj).and_then(of_object),
            ArrHit::Head(RowKind::Audio(a)) | ArrHit::Volume(a) => Some(of_audio(a)),
            ArrHit::Clip(ClipRef::Audio(k), _) => self
                .audio_editor()
                .audio_clips()
                .get(k)
                .map(|c| of_audio(c.asset)),
            ArrHit::Clip(ClipRef::Comp(i), _) => {
                self.editor.comp_clips().get(i).map(|c| Source::Comp(c.comp))
            }
            ArrHit::Head(RowKind::CompTrack(t)) => self
                .editor
                .comp_clips()
                .iter()
                .find(|c| c.track == t)
                .map(|c| Source::Comp(c.comp)),
            _ => None,
        }
    }

    /// The source behind the primary selection, if it is a mesh —
    /// what a right-click on the canvas can relink.
    pub(crate) fn primary_source(&self) -> Option<Source> {
        let i = self.editor.primary()?;
        self.editor.shapes().get(i)?.mesh_asset().map(Source::Mesh)
    }

    /// The file a source names, as the comp has it.
    pub(crate) fn source_path(&self, src: Source) -> Option<String> {
        match src {
            Source::Mesh(id) => self
                .editor
                .assets()
                .iter()
                .find(|a| a.id == id)
                .map(|a| a.path.clone()),
            Source::Comp(id) => self.editor.comp_asset(id).map(|a| a.path.clone()),
            Source::Song => self.audio_editor().audio_path().map(str::to_string),
            Source::Sound(id) => self.audio_editor().sound(id).map(|s| s.path.clone()),
        }
    }

    /// Whether the source's file failed to load.
    pub(crate) fn source_missing(&self, src: Source) -> bool {
        match src {
            Source::Mesh(id) => self.mesh_missing.contains(&id),
            Source::Comp(id) => self.subcomps.get(&id).is_some_and(|pc| pc.missing),
            Source::Song => matches!(self.sounds.get(&SONG), Some(crate::sound::Slot::Missing)),
            Source::Sound(id) => matches!(self.sounds.get(&id), Some(crate::sound::Slot::Missing)),
        }
    }

    /// What the menu is titled over a source: the file's name, flagged
    /// when it couldn't be read.
    pub(crate) fn source_title(&self, src: Source) -> String {
        let path = self.source_path(src).unwrap_or_default();
        let name = std::path::Path::new(&path)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or(path);
        if self.source_missing(src) {
            format!("! missing: {name}")
        } else {
            name
        }
    }

    /// Relink source…: the picker, for the kind of file the source is.
    pub(crate) fn relink_start(&mut self, src: Source) -> bool {
        if matches!(src, Source::Song | Source::Sound(_)) && self.in_comp() {
            let note = "Audio belongs to the project — go Back first".to_string();
            println!("{note}");
            self.export_note = Some(note);
            return true;
        }
        self.spawn_picker(picker::Purpose::Relink(src));
        false
    }

    /// The picker came back: the comp names the new file, and it loads
    /// in where the old one was. What the file is *for* — the object,
    /// the clips, the volume — stays exactly as it was.
    pub(crate) fn relink(&mut self, src: Source, path: PathBuf) {
        let p = path.to_string_lossy().into_owned();
        match src {
            Source::Mesh(id) => {
                if !self.editor.relink_mesh(id, p) {
                    return;
                }
                self.meshes.remove(&id);
                self.mesh_missing.retain(|m| *m != id);
                self.spawn_mesh_load(Some(id), path);
            }
            Source::Comp(id) => {
                if !self.editor.relink_comp(id, p) {
                    return;
                }
                if let Some(pc) = self.subcomps.remove(&id) {
                    for (_, g) in pc.mesh_map {
                        self.meshes.remove(&g);
                    }
                }
                self.sync_subcomps();
            }
            Source::Song => {
                // The song's path isn't in the undo history (it never
                // was); the clips that place it are untouched.
                self.editor.set_audio_path(Some(p));
                self.audio = None;
                self.audio_file = None;
                self.song_missing(false);
                self.sync_audio();
            }
            Source::Sound(id) => {
                if !self.editor.relink_sound(id, p) {
                    return;
                }
                self.sounds.remove(&id);
                self.sync_sounds();
            }
        }
        println!("relinked {}", self.source_title(src));
        self.request_redraw();
    }
}
