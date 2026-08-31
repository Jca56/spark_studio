//! Ctrl+Shift+C — Make Comp from Selection: hive the selected shapes off
//! into their own .spark file and play them from a clip exactly where
//! their motion was. Ableton's consolidate, AE's precompose: you draw
//! and animate *in the project*, then comp-ify what turned out to be a
//! self-contained piece — nobody plans comps ahead.
//!
//! Time is the careful part. The selection's keys live on song time; in
//! the new comp they shift so the first key is local zero, the comp's
//! declared duration is the key span, and the clip is placed at the old
//! first-key time for one span — so the picture at every moment of that
//! span is exactly what it was. Outside it the piece now *ends*, which
//! is the point: a clip exists only where it is placed, and dragging its
//! right edge loops the motion out. A selection with no keys becomes a
//! static comp on a clip at the playhead, `fallback_len` long.
//!
//! A folder travels only if every member is selected; a partial
//! selection leaves the folder (and its transform) behind, so a member
//! of a *moved* folder lands where its own numbers say — named here
//! because it is the one way the picture can shift.

use crate::doc::{self, Clip};

use super::Editor;

impl Editor {
    /// Write the selection to `path` as a comp and replace it with a
    /// clip. One undo step covers the removal and the placement (the
    /// file itself stays — files aren't undoable). Returns the clip's
    /// index, or `None` with nothing changed.
    pub fn precompose(&mut self, path: &str, fallback_start: f32, fallback_len: f32) -> Option<usize> {
        if self.selection.is_empty() {
            println!("select something to make a comp of");
            return None;
        }
        let mut idx = self.selection.clone();
        idx.sort_unstable();
        idx.dedup();
        // Where the selection's motion lives on song time.
        let (mut t0, mut t1) = (f32::MAX, f32::MIN);
        for &i in &idx {
            for tr in &self.anim[i].tracks {
                for k in &tr.keys {
                    t0 = t0.min(k.t);
                    t1 = t1.max(k.t);
                }
            }
        }
        let (start, span) = if t0 <= t1 && t1 - t0 > 0.01 {
            (t0, t1 - t0)
        } else {
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
            let mut anim = self.anim[i].clone();
            for tr in &mut anim.tracks {
                for k in &mut tr.keys {
                    k.t -= start;
                }
            }
            // Baked at local zero — the pose the comp opens on is the one
            // the project showed at `start`.
            let mut c = self.shapes[i];
            anim.apply_shape(&mut c, 0.0);
            if let Some((id, _, _)) = c.path_meta() {
                c.set_path_start(d.paths.len());
                d.paths.push(self.paths.get(id).cloned().unwrap_or_default());
            }
            d.shapes.push(c);
            d.names.push(self.names[i].clone());
            d.anims.push(anim);
            d.fx.push(self.fx[i].clone());
            d.reacts.push(self.react[i]);
            d.groups.push(self.group[i]);
            d.hidden.push(self.hidden[i]);
            let f = self.folder[i];
            d.folder.push(if carried.contains(&f) { f } else { 0 });
        }
        for id in &carried {
            if let Some(f) = self.folder(*id) {
                let mut f = f.clone();
                for tr in &mut f.anim.tracks {
                    for k in &mut tr.keys {
                        k.t -= start;
                    }
                }
                d.folders.push(f);
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
        // From here, one undo step: shapes out, asset and clip in.
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
        self.clips.push(Clip {
            track,
            comp,
            start,
            len: span,
        });
        println!(
            "made comp {path}: {} shape(s), {span:.2}s — drag the clip's edge to loop it",
            d.shapes.len()
        );
        Some(self.clips.len() - 1)
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
        let i = e.push_shape(Shape::rect([400.0, 300.0], [50.0, 30.0]));
        e.select(Some(i));
        // A move from 8s to 10s on song time.
        e.anim_of_mut(i).tracks.push(Track {
            target: Target::Shape(Prop::X),
            keys: vec![
                Key { t: 8.0, v: 400.0, ease: Ease::Linear },
                Key { t: 10.0, v: 900.0, ease: Ease::Linear },
            ],
        });
        (e, dir.join("piece.spark").to_string_lossy().into_owned())
    }

    /// The whole promise: keys shift to local zero, the comp's length is
    /// the key span, and the clip sits at the old first-key time — the
    /// picture inside the span is untouched, and undo puts it all back.
    #[test]
    fn precompose_keeps_the_picture_and_undoes_whole() {
        let dir = std::env::temp_dir().join(format!("spark-precompose-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let (mut e, path) = keyed_editor(&dir);
        let clip = e.precompose(&path, 0.0, 2.0).expect("precomposed");
        assert!(e.shapes().is_empty(), "the shape moved out");
        let c = e.clips()[clip];
        assert_eq!((c.start, c.len), (8.0, 2.0));
        let d = doc::parse(&std::fs::read_to_string(&path).unwrap());
        assert_eq!(d.duration, Some(2.0));
        let keys = &d.anims[0].tracks[0].keys;
        assert_eq!((keys[0].t, keys[1].t), (0.0, 2.0), "keys start at local zero");
        // Baked at local zero = the pose the project showed at 8s.
        assert_eq!(d.shapes[0].center()[0], 400.0);
        // The comp loops to exactly the authored motion.
        let pc = crate::comps::PlacedComp::new(path.clone(), d, Vec::new());
        assert_eq!(pc.period, 2.0);
        let posed = crate::comps::pose(&pc, 1.0);
        assert!((posed[0].0.center()[0] - 650.0).abs() < 1e-3, "mid-move at local 1s");
        // One undo step brings the shape back and takes the clip away.
        e.undo();
        assert_eq!(e.shapes().len(), 1);
        assert!(e.clips().is_empty());
        std::fs::remove_dir_all(&dir).ok();
    }

    /// A static selection becomes a clip at the playhead, a bar long.
    #[test]
    fn a_static_selection_lands_at_the_playhead() {
        let dir = std::env::temp_dir().join(format!("spark-precompose-s-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut e = Editor::empty();
        let i = e.push_shape(Shape::circle([100.0, 100.0], 20.0));
        e.select(Some(i));
        let path = dir.join("static.spark").to_string_lossy().into_owned();
        let clip = e.precompose(&path, 12.0, 2.0).expect("precomposed");
        let c = e.clips()[clip];
        assert_eq!((c.start, c.len), (12.0, 2.0));
        std::fs::remove_dir_all(&dir).ok();
    }
}
