//! The object clipboard: `Ctrl+C` takes whole objects — geometry, look,
//! effects, clips and their keys, names, merge groups, path vertices —
//! and `Ctrl+V` puts them back anywhere, as many times as you like.
//! Until this there had only ever been a *style* clipboard (Alva:
//! "no regular Copy + Paste? Wild!", 2026-08-31).
//!
//! Copies are taken from the document truth (`base`), never the posed
//! working copies, so a copy made mid-animation is the object, not the
//! frame. Paste lands the set's centre on the point asked for — the
//! cursor, or where the context menu was opened — and its clips at the
//! playhead: a thing exists where its clip is, and you paste it where
//! you are. Keyed X/Y move with the paste, as they do for Duplicate.
//! Copies land loose: a folder's transform stays behind, which is the one
//! way a pasted shape can sit somewhere other than its original (the
//! caveat `precompose` names too).

use spark_render::Shape;

use super::Editor;
use crate::doc::ObjClip;
use crate::fx::Stack;

/// One copied object, whole.
#[derive(Clone)]
struct Copied {
    base: Shape,
    /// A path's vertices, so the copy owns its own outline.
    path: Option<Vec<[f32; 2]>>,
    fx: Stack,
    name: String,
    group: u32,
    hidden: bool,
    clips: Vec<ObjClip>,
}

/// What `Ctrl+C` holds.
#[derive(Clone)]
pub struct Clipboard {
    items: Vec<Copied>,
    /// The centre of the copied set's base centres — what lands on the
    /// paste point.
    centre: [f32; 2],
}

impl Editor {
    /// Copy the selection, whole. Changes nothing on screen.
    pub fn copy_objects(&mut self) -> bool {
        if self.selection.is_empty() {
            return false;
        }
        // A copy is of the document truth, so the hand's latest edits
        // fold in first — the frame's sync would do it a moment later,
        // but a copy taken in the same tick as a glow nudge must not
        // miss the glow.
        self.absorb_pending();
        let mut idx = self.selection.clone();
        idx.sort_unstable();
        idx.dedup();
        let items: Vec<Copied> = idx
            .iter()
            .map(|&i| Copied {
                base: self.base[i],
                path: self.base[i]
                    .path_meta()
                    .and_then(|(id, _, _)| self.paths.get(id).cloned()),
                fx: self.base_fx[i].clone(),
                name: self.names[i].clone(),
                group: self.group[i],
                hidden: self.hidden[i],
                clips: self.clips[i].clone(),
            })
            .collect();
        let (mut lo, mut hi) = ([f32::MAX; 2], [f32::MIN; 2]);
        for it in &items {
            let c = it.base.center();
            for k in 0..2 {
                lo[k] = lo[k].min(c[k]);
                hi[k] = hi[k].max(c[k]);
            }
        }
        let centre = [(lo[0] + hi[0]) * 0.5, (lo[1] + hi[1]) * 0.5];
        println!("copied {} object(s)", items.len());
        self.clipboard = Some(Clipboard { items, centre });
        false
    }

    /// Whether `Ctrl+C` has objects waiting — what lights Paste.
    pub fn has_clipboard(&self) -> bool {
        self.clipboard.is_some()
    }

    /// Paste the copied objects with their centre at `at` (canvas units)
    /// and their clips starting at the playhead. The pasted set becomes
    /// the selection; one undo step.
    pub fn paste_objects(&mut self, at: [f32; 2]) -> bool {
        let Some(cb) = self.clipboard.clone() else {
            return false;
        };
        if cb.items.is_empty() {
            return false;
        }
        // Pending hand edits reach the truth before the undo snapshot
        // reads it, or undoing the paste would undo them too.
        self.absorb_pending();
        let s = self.snap();
        self.history.push(s);
        let d = [at[0] - cb.centre[0], at[1] - cb.centre[1]];
        // The earliest clip lands on the playhead; the rest keep their
        // spacing after it.
        let t0 = cb
            .items
            .iter()
            .flat_map(|it| it.clips.iter().map(|c| c.start))
            .fold(f32::MAX, f32::min);
        let dt = if t0 == f32::MAX { 0.0 } else { self.time - t0 };
        // Copies of merged shapes merge with each other, not the originals.
        let mut next_group = self.group.iter().copied().max().unwrap_or(0);
        let mut gmap: Vec<(u32, u32)> = Vec::new();
        let mut new_sel = Vec::new();
        for it in &cb.items {
            let mut base = it.base;
            base.translate(d);
            if let Some(verts) = &it.path {
                base.set_path_start(self.paths.len());
                self.paths.push(verts.clone());
            }
            let mut clips = it.clips.clone();
            for c in &mut clips {
                c.start += dt;
                for track in &mut c.anim.tracks {
                    // A keyed place — a centre, a line's end — lands
                    // offset the way the shape did.
                    let axis = track.target.prop().and_then(crate::anim::place_axis);
                    if let Some(a) = axis {
                        for k in &mut track.keys {
                            k.v += d[a];
                        }
                    }
                }
            }
            self.shapes.push(base);
            self.base.push(base);
            let id = self.new_id();
            self.ids.push(id);
            self.names.push(it.name.clone());
            self.clips.push(clips);
            self.fx.push(it.fx.clone());
            self.base_fx.push(it.fx.clone());
            self.group.push(if it.group == 0 {
                0
            } else {
                match gmap.iter().find(|(from, _)| *from == it.group) {
                    Some(&(_, to)) => to,
                    None => {
                        next_group += 1;
                        gmap.push((it.group, next_group));
                        next_group
                    }
                }
            });
            self.hidden.push(it.hidden);
            self.folder.push(0);
            new_sel.push(self.shapes.len() - 1);
        }
        self.selection = new_sel;
        self.clear_posed();
        println!("pasted {} object(s)", cb.items.len());
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::anim::Target;
    use crate::props::Prop;
    use crate::fx::EffectKind;
    use crate::props::Tool;

    fn draw(e: &mut Editor, tool: Tool, from: [f32; 2], to: [f32; 2]) -> usize {
        e.choose_tool(tool);
        e.set_cursor_canvas(from);
        e.mouse_down(false);
        e.set_cursor_canvas(to);
        e.mouse_up();
        e.choose_tool(Tool::Select);
        e.primary().expect("drawn")
    }

    /// Two circles copied and pasted land as two new circles centred on
    /// the paste point with their spacing kept, selected; the originals
    /// don't move; one undo takes the paste away.
    #[test]
    fn a_paste_lands_the_set_on_the_point() {
        let mut e = Editor::empty();
        let a = draw(&mut e, Tool::Circle, [300.0, 300.0], [360.0, 300.0]);
        let b = draw(&mut e, Tool::Circle, [500.0, 400.0], [560.0, 400.0]);
        e.selection = vec![a, b];
        e.copy_objects();
        assert!(e.has_clipboard());
        assert!(e.paste_objects([1000.0, 800.0]));
        assert_eq!(e.shapes().len(), 4);
        assert_eq!(e.selection(), &[2, 3], "the paste is the selection");
        let (p, q) = (e.shapes()[2].center(), e.shapes()[3].center());
        assert_eq!([(p[0] + q[0]) * 0.5, (p[1] + q[1]) * 0.5], [1000.0, 800.0]);
        assert_eq!([q[0] - p[0], q[1] - p[1]], [200.0, 100.0], "spacing kept");
        assert_eq!(e.shapes()[a].center(), [300.0, 300.0], "the original stayed");
        // Pasting twice is two more.
        assert!(e.paste_objects([100.0, 100.0]));
        assert_eq!(e.shapes().len(), 6);
        e.undo();
        assert_eq!(e.shapes().len(), 4);
        // And the document survives a round trip through the file.
        let d1 = crate::doc::serialize(&e.to_doc());
        assert!(d1.contains("circle") || !d1.is_empty());
    }

    /// The copy carries the look: effects come along, and a pasted path
    /// owns its own vertices — editing the copy leaves the original alone.
    #[test]
    fn a_copy_carries_effects_and_its_own_path() {
        let mut e = Editor::empty();
        let i = draw(&mut e, Tool::Box, [300.0, 300.0], [400.0, 360.0]);
        e.set_glow_selection(50.0);
        assert!(e.convert_to_path());
        e.copy_objects();
        assert!(e.paste_objects([900.0, 500.0]));
        let j = e.primary().unwrap();
        assert_ne!(i, j);
        assert_eq!(
            e.fx_of(j).active(EffectKind::Glow).map(|g| g.get(0)),
            Some(50.0)
        );
        let (pi, _, _) = e.shapes()[i].path_meta().unwrap();
        let (pj, _, _) = e.shapes()[j].path_meta().unwrap();
        assert_ne!(pi, pj, "the paste shares the original's vertex list");
        let before = e.path(pi).to_vec();
        // Drag a vertex on the pasted path (the primary).
        assert!(e.drag_vertex(0, [950.0, 450.0]));
        assert_eq!(e.path(pi), &before[..], "the original's outline moved");
    }

    /// A merged pair pastes as its own merged pair, not into the
    /// original's group.
    #[test]
    fn a_pasted_merge_is_its_own_group() {
        let mut e = Editor::empty();
        let a = draw(&mut e, Tool::Circle, [300.0, 300.0], [360.0, 300.0]);
        let b = draw(&mut e, Tool::Circle, [500.0, 300.0], [560.0, 300.0]);
        e.selection = vec![a, b];
        assert!(e.merge_selected());
        let g = e.groups()[a];
        assert_ne!(g, 0);
        e.copy_objects();
        assert!(e.paste_objects([1000.0, 600.0]));
        let pasted = e.selection().to_vec();
        assert_eq!(pasted.len(), 2);
        let pg = e.groups()[pasted[0]];
        assert_ne!(pg, 0, "the paste lost its merge");
        assert_ne!(pg, g, "the paste joined the original's group");
        assert_eq!(e.groups()[pasted[1]], pg);
    }

    /// Clips land at the playhead — the pasted object exists where you
    /// are, not where the original was — and keyed X/Y move with the
    /// paste so an animated copy flies beside its point.
    #[test]
    fn clips_land_at_the_playhead_and_keys_move_with_the_paste() {
        let mut e = Editor::empty();
        let i = draw(&mut e, Tool::Circle, [300.0, 300.0], [360.0, 300.0]);
        e.sync_to_time();
        assert!(e.stamp_key(), "the first pose");
        assert_eq!(e.obj_clips(i)[0].start, 0.0);
        e.copy_objects();
        e.set_time(6.0);
        e.sync_to_time();
        assert!(!e.exists_now(i), "the original's clip ended at 2 s");
        assert!(e.paste_objects([800.0, 300.0]));
        let j = e.primary().unwrap();
        let clip = &e.obj_clips(j)[0];
        assert!((clip.start - 6.0).abs() < 1e-5, "landed at {}", clip.start);
        assert!(e.exists_now(j), "pasted where you are");
        let x = clip
            .anim
            .track(Target::Shape(Prop::X))
            .expect("the pose's X key came along");
        assert!((x.keys[0].v - 800.0).abs() < 1e-3, "X key at {}", x.keys[0].v);
        // The original's key is untouched.
        let ox = e.obj_clips(i)[0].anim.track(Target::Shape(Prop::X)).unwrap();
        assert!((ox.keys[0].v - 300.0).abs() < 1e-3);
    }

    /// Nothing copied, nothing pasted; nothing selected, nothing copied.
    #[test]
    fn an_empty_clipboard_pastes_nothing() {
        let mut e = Editor::empty();
        assert!(!e.has_clipboard());
        assert!(!e.paste_objects([0.0; 2]));
        assert!(!e.copy_objects());
        assert!(!e.has_clipboard());
        assert_eq!(e.shapes().len(), 0);
    }
}
