//! Ctrl+Shift+C — Make Comp from Selection: hive the selected objects off
//! into their own .spark file and play them from a comp clip exactly
//! where they were. Ableton's consolidate, AE's precompose: you draw and
//! animate *in the project*, then comp-ify what turned out to be a
//! self-contained piece — nobody plans comps ahead.
//!
//! Time is the easy part now: an object's motion is clip-local, so
//! nothing about the keys changes — only the clip *starts* shift so the
//! earliest selected clip is the comp's local zero, the comp's duration
//! is the selection's clip span, and the comp clip is placed over that
//! span in the host. The picture inside the span is exactly what it was.
//!
//! A folder travels only if every member is selected; a partial selection
//! leaves the folder (and its transform) behind — the one way the
//! picture can shift, named here.

use crate::doc::{self, Clip};

use super::Editor;

impl Editor {
    /// Write the selection to `path` as a comp and replace it with a comp
    /// clip. One undo step covers the removal and the placement (the file
    /// itself stays — files aren't undoable). Returns the clip's index,
    /// or `None` with nothing changed.
    pub fn precompose(
        &mut self,
        path: &str,
        fallback_start: f32,
        fallback_len: f32,
    ) -> Option<usize> {
        if self.selection.is_empty() {
            println!("select something to make a comp of");
            return None;
        }
        let mut idx = self.selection.clone();
        idx.sort_unstable();
        idx.dedup();
        // Where the selection exists on song time: its clips' extent.
        let (mut t0, mut t1) = (f32::MAX, f32::MIN);
        for &i in &idx {
            for c in self.obj_clips(i) {
                t0 = t0.min(c.start);
                t1 = t1.max(c.end());
            }
        }
        let (start, span) = if t0 < t1 {
            (t0, t1 - t0)
        } else {
            // Nothing scheduled at all: a static comp at the playhead.
            (fallback_start, fallback_len.max(0.1))
        };
        // Folders travel only whole.
        let carried: Vec<u32> = self
            .folders
            .iter()
            .map(|f| f.id)
            .filter(|&id| {
                let members = self.folder_members(id);
                !members.is_empty() && members.iter().all(|m| idx.contains(m))
            })
            .collect();
        let mut d = doc::Doc {
            canvas: self.canvas,
            duration: Some(span),
            ..Default::default()
        };
        for &i in &idx {
            // Base state travels as-is; the motion is in the clips, which
            // only shift their starts to the comp's local zero.
            let mut c = self.base[i];
            if let Some((id, _, _)) = c.path_meta() {
                c.set_path_start(d.paths.len());
                d.paths.push(self.paths.get(id).cloned().unwrap_or_default());
            }
            let mut clips = self.clips[i].clone();
            for clip in &mut clips {
                clip.start -= start;
            }
            d.shapes.push(c);
            d.ids.push(self.ids[i]);
            d.names.push(self.names[i].clone());
            d.oclips.push(clips);
            d.fx.push(self.base_fx[i].clone());
            d.groups.push(self.group[i]);
            d.hidden.push(self.hidden[i]);
            let f = self.folder[i];
            d.folder.push(if carried.contains(&f) { f } else { 0 });
        }
        for id in &carried {
            if let Some(f) = self.folder(*id) {
                d.folders.push(f.clone());
            }
        }
        d.assets = self
            .assets
            .iter()
            .filter(|a| d.shapes.iter().any(|s| s.mesh_asset() == Some(a.id)))
            .cloned()
            .collect();
        if let Some(base) = std::path::Path::new(path).parent() {
            super::io::relativize_paths(&mut d, base);
        }
        if let Err(e) = std::fs::write(path, doc::serialize(&d)) {
            // Nothing has been touched yet: a failed write changes nothing.
            println!("couldn't write {path}: {e}");
            return None;
        }
        // From here, one undo step: objects out, asset and clip in.
        let s = self.snap();
        self.history.push(s);
        for &i in idx.iter().rev() {
            self.remove_shape(i);
        }
        self.selection.clear();
        self.normalize_folders();
        self.clear_posed();
        let comp = self.add_comp_asset(path.to_string());
        let track = self.free_track(start, span);
        self.comp_clips.push(Clip {
            track,
            comp,
            start,
            len: span,
        });
        println!(
            "made comp {path}: {} object(s), {span:.2}s — drag the clip's edge to loop it",
            d.shapes.len()
        );
        Some(self.comp_clips.len() - 1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::anim::{Ease, Key, Target, Track};
    use crate::props::Prop;
    use spark_render::Shape;

    fn keyed_editor(dir: &std::path::Path) -> (Editor, String) {
        let mut e = Editor::empty();
        // Born at t=8 with a 2s clip carrying a move.
        e.set_time(8.0);
        let i = e.push_shape(Shape::rect([400.0, 300.0], [50.0, 30.0]));
        e.select(Some(i));
        e.clip_anim_mut(i, 0).tracks.push(Track {
            target: Target::Shape(Prop::X),
            keys: vec![
                Key {
                    t: 0.0,
                    v: 400.0,
                    ease: Ease::Linear,
                },
                Key {
                    t: 2.0,
                    v: 900.0,
                    ease: Ease::Linear,
                },
            ],
        });
        (e, dir.join("piece.spark").to_string_lossy().into_owned())
    }

    /// The whole promise: clip starts shift to local zero, keys untouched,
    /// the comp's length is the clip span, and the comp clip sits where
    /// the selection was — the picture inside the span is untouched, and
    /// undo puts it all back.
    #[test]
    fn precompose_keeps_the_picture_and_undoes_whole() {
        let dir = std::env::temp_dir().join(format!("spark-precompose-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let (mut e, path) = keyed_editor(&dir);
        let clip = e.precompose(&path, 0.0, 2.0).expect("precomposed");
        assert!(e.shapes().is_empty(), "the object moved out");
        let c = e.comp_clips()[clip];
        assert_eq!((c.start, c.len), (8.0, e.bar_s));
        let d = doc::parse(&std::fs::read_to_string(&path).unwrap());
        assert_eq!(d.duration, Some(e.bar_s));
        assert_eq!(d.oclips[0][0].start, 0.0, "the clip landed at local zero");
        let keys = &d.oclips[0][0].anim.tracks[0].keys;
        assert_eq!((keys[0].t, keys[1].t), (0.0, 2.0), "keys stayed clip-local");
        // The comp poses to the authored motion at its own local time.
        let pc = crate::comps::PlacedComp::new(path.clone(), d, Vec::new());
        assert_eq!(pc.period, e.bar_s);
        let posed = crate::comps::pose(&pc, 1.0, None, spark_render::CANVAS);
        assert!(
            (posed[0].center()[0] - 650.0).abs() < 1e-3,
            "mid-move at local 1s, got {}",
            posed[0].center()[0]
        );
        // One undo step brings the object back and takes the clip away.
        e.undo();
        assert_eq!(e.shapes().len(), 1);
        assert!(e.comp_clips().is_empty());
        std::fs::remove_dir_all(&dir).ok();
    }

    /// An object with no clips at all becomes a static comp at the
    /// playhead fallback.
    #[test]
    fn a_clipless_selection_lands_at_the_fallback() {
        let dir = std::env::temp_dir().join(format!("spark-precompose-s-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut e = Editor::empty();
        let i = e.push_shape(Shape::circle([100.0, 100.0], 20.0));
        e.delete_obj_clip(i, 0);
        e.select(Some(i));
        let path = dir.join("static.spark").to_string_lossy().into_owned();
        let clip = e.precompose(&path, 12.0, 2.0).expect("precomposed");
        let c = e.comp_clips()[clip];
        assert_eq!((c.start, c.len), (12.0, 2.0));
        std::fs::remove_dir_all(&dir).ok();
    }
}
