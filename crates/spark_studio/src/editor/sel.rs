//! Selection management and whole-selection transforms.

use crate::history::Tag;
use crate::props::{Prop, remap};

use super::Editor;

impl Editor {
    pub fn select(&mut self, i: Option<usize>) -> bool {
        self.history.commit();
        self.range_anchor = i;
        let old = std::mem::take(&mut self.selection);
        self.selection = i.into_iter().collect();
        self.expand_groups();
        old != self.selection
    }

    /// Ctrl+click on a layer row: toggle membership.
    pub fn toggle_select(&mut self, i: usize) -> bool {
        self.history.commit();
        self.range_anchor = Some(i);
        self.toggle_index(i);
        true
    }

    /// Shift+click on a layer row: take the whole stack run between the
    /// anchor (the last plain/ctrl click) and `i`. The anchor stays put, so
    /// shift-clicking around re-spans from one origin rather than ratcheting.
    /// With no anchor yet it's just a plain click.
    pub fn select_range(&mut self, i: usize) -> bool {
        if i >= self.shapes.len() {
            return false;
        }
        let anchor = self.range_anchor.filter(|&a| a < self.shapes.len());
        let Some(anchor) = anchor else {
            return self.select(Some(i));
        };
        self.history.commit();
        let old = self.selection.clone();
        // The clicked row goes last so it reads as primary — that's the card
        // the color home and the handles target.
        let mut out: Vec<usize> = (anchor.min(i)..=anchor.max(i)).filter(|&j| j != i).collect();
        out.push(i);
        self.selection = out;
        self.expand_groups();
        old != self.selection
    }

    /// Grow the selection to whole merged groups — members travel
    /// together. The clicked shape stays primary (last).
    pub(super) fn expand_groups(&mut self) {
        let Some(&primary) = self.selection.last() else {
            return;
        };
        let mut out: Vec<usize> = Vec::new();
        for &i in &self.selection {
            let g = self.group.get(i).copied().unwrap_or(0);
            if g == 0 {
                if !out.contains(&i) {
                    out.push(i);
                }
            } else {
                for (j, &gj) in self.group.iter().enumerate() {
                    if gj == g && !out.contains(&j) {
                        out.push(j);
                    }
                }
            }
        }
        out.retain(|&j| j != primary);
        out.push(primary);
        self.selection = out;
    }

    /// Toggle `i` in the selection; a merged group joins or leaves whole.
    pub(super) fn toggle_index(&mut self, i: usize) {
        let g = self.group.get(i).copied().unwrap_or(0);
        match self.selection.iter().position(|&s| s == i) {
            Some(pos) => {
                if g == 0 {
                    self.selection.remove(pos);
                } else {
                    let group = std::mem::take(&mut self.group);
                    self.selection
                        .retain(|&s| group.get(s).copied().unwrap_or(0) != g);
                    self.group = group;
                }
            }
            None => {
                self.selection.push(i);
                self.expand_groups();
            }
        }
    }

    /// Ctrl+G: merge the selection into one group — one layer row, one
    /// object to grab. Every member keeps its own color, style, geometry,
    /// and keyframes; merging a mix re-merges everything into a fresh
    /// group.
    pub fn merge_selected(&mut self) -> bool {
        if self.selection.len() < 2 {
            println!("select 2+ shapes to merge");
            return false;
        }
        let s = self.snap();
        self.history.push(s);
        let id = self.group.iter().copied().max().unwrap_or(0) + 1;
        for &i in &self.selection {
            self.group[i] = id;
        }
        println!("merged {} shapes", self.selection.len());
        true
    }

    /// Ctrl+Shift+G: dissolve the selection's groups into loose shapes.
    pub fn unmerge_selected(&mut self) -> bool {
        if self
            .selection
            .iter()
            .all(|&i| self.group.get(i).copied().unwrap_or(0) == 0)
        {
            println!("nothing merged in the selection");
            return false;
        }
        let s = self.snap();
        self.history.push(s);
        for &i in &self.selection {
            self.group[i] = 0;
        }
        println!("unmerged");
        true
    }

    /// Merge-group ids per shape (0 = ungrouped), for panels.
    pub fn groups(&self) -> &[u32] {
        &self.group
    }

    /// Eye states per shape, for the cards.
    pub fn hidden(&self) -> &[bool] {
        &self.hidden
    }

    /// Folder-aware: a shape in a hidden folder reads as hidden.
    pub fn is_hidden(&self, i: usize) -> bool {
        self.shape_hidden(i)
    }

    /// The eye button: flip a shape's visibility — a whole merged group
    /// flips together to its anchor's new state.
    pub fn toggle_hidden(&mut self, i: usize) -> bool {
        if i >= self.hidden.len() {
            return false;
        }
        let s = self.snap();
        self.history.push(s);
        let to = !self.hidden[i];
        let g = self.group.get(i).copied().unwrap_or(0);
        if g == 0 {
            self.hidden[i] = to;
        } else {
            for (j, &gj) in self.group.iter().enumerate() {
                if gj == g {
                    self.hidden[j] = to;
                }
            }
        }
        true
    }

    /// Move the shape at `from` to stack position `to` (layer drag). The
    /// whole drag coalesces into one undo step.
    pub fn move_layer(&mut self, from: usize, to: usize) -> bool {
        if from == to || from >= self.shapes.len() || to >= self.shapes.len() {
            return false;
        }
        let s = self.snap();
        self.history.change(Tag::Reorder, s);
        let shape = self.shapes.remove(from);
        self.shapes.insert(to, shape);
        let name = self.names.remove(from);
        self.names.insert(to, name);
        let anim = self.anim.remove(from);
        self.anim.insert(to, anim);
        let react = self.react.remove(from);
        self.react.insert(to, react);
        let group = self.group.remove(from);
        self.group.insert(to, group);
        let hidden = self.hidden.remove(from);
        self.hidden.insert(to, hidden);
        let folder = self.folder.remove(from);
        self.folder.insert(to, folder);
        for s in &mut self.selection {
            *s = remap(*s, from, to);
        }
        self.clear_posed();
        // The clipboard's shape index no longer means what it did.
        self.key_clip = None;
        // A drag can land a layer in the middle of a folder's run; pull the
        // runs back together so the list stays honest about draw order.
        self.normalize_folders();
        true
    }

    pub fn delete_selected(&mut self) -> bool {
        if self.selection.is_empty() {
            return false;
        }
        let s = self.snap();
        self.history.push(s);
        let mut idx = std::mem::take(&mut self.selection);
        idx.sort_unstable();
        idx.dedup();
        for &i in idx.iter().rev() {
            self.shapes.remove(i);
            self.names.remove(i);
            self.anim.remove(i);
            self.react.remove(i);
            self.group.remove(i);
            self.hidden.remove(i);
            self.folder.remove(i);
        }
        // Emptied folders go with their contents.
        self.normalize_folders();
        self.clear_posed();
        self.key_clip = None;
        println!(
            "deleted {} shape(s) ({} left)",
            idx.len(),
            self.shapes.len()
        );
        true
    }

    /// Ctrl+D: clone the selection — geometry, name, path vertices, and
    /// animation — nudged down-right so the copies read as copies. Keyed
    /// X/Y curves shift by the same nudge, so animated copies fly the same
    /// path beside the original instead of on top of it.
    pub fn duplicate_selected(&mut self) -> bool {
        if self.selection.is_empty() {
            return false;
        }
        const NUDGE: f32 = 48.0;
        let s = self.snap();
        self.history.push(s);
        let mut idx = self.selection.clone();
        idx.sort_unstable();
        idx.dedup();
        let mut new_sel = Vec::new();
        // Copies of merged shapes merge with each other, not the originals.
        let mut next_group = self.group.iter().copied().max().unwrap_or(0);
        let mut gmap: Vec<(u32, u32)> = Vec::new();
        for &i in &idx {
            let mut shape = self.shapes[i];
            shape.translate([NUDGE, NUDGE]);
            if let Some((id, _, _)) = shape.path_meta() {
                let verts = self.paths.get(id).cloned().unwrap_or_default();
                shape.set_path_start(self.paths.len());
                self.paths.push(verts);
            }
            let mut anim = self.anim[i].clone();
            for track in &mut anim.tracks {
                if matches!(track.prop, Prop::X | Prop::Y) {
                    for k in &mut track.keys {
                        k.v += NUDGE;
                    }
                }
            }
            self.shapes.push(shape);
            self.names.push(self.names[i].clone());
            self.anim.push(anim);
            self.react.push(self.react[i]);
            let g = self.group[i];
            self.group.push(if g == 0 {
                0
            } else {
                match gmap.iter().find(|(from, _)| *from == g) {
                    Some(&(_, to)) => to,
                    None => {
                        next_group += 1;
                        gmap.push((g, next_group));
                        next_group
                    }
                }
            });
            self.hidden.push(self.hidden[i]);
            // Copies land loose: duplicating out of a folder shouldn't
            // silently stuff the copies back into it.
            self.folder.push(0);
            new_sel.push(self.shapes.len() - 1);
        }
        self.selection = new_sel;
        println!("duplicated {} shape(s)", idx.len());
        true
    }

    pub fn deselect(&mut self) -> bool {
        self.history.commit();
        let had = !self.selection.is_empty();
        self.selection.clear();
        had
    }

    /// Live picker color: becomes the current color and paints every selected
    /// shape, the whole drag coalescing into one undo step. With `to_b`,
    /// gradient-enabled shapes take it as the gradient's end color instead.
    /// Works with nothing selected — that's how you choose a color to draw
    /// with before there's anything to draw on.
    pub fn set_rgb_selection(&mut self, rgb: [f32; 3], to_b: bool) -> bool {
        self.set_current_color(rgb, to_b);
        true
    }

    /// Gradient fill toggle for the selection. Turning it on seeds an
    /// unset end color with a deep-dimmed copy of the first, so the wash
    /// reads immediately.
    pub fn set_gradient(&mut self, on: bool) -> bool {
        if self.selection.is_empty() {
            return false;
        }
        let s = self.snap();
        self.history.push(s);
        for &i in &self.selection {
            let sh = &mut self.shapes[i];
            if on && !sh.gradient() && sh.rgb2() == [0.0; 3] {
                let c = sh.rgb();
                sh.set_rgb2([c[0] * 0.15, c[1] * 0.15, c[2] * 0.15]);
            }
            sh.set_gradient(on);
        }
        true
    }

    /// Uniform-scale every selected shape; with `around`, positions orbit
    /// that point too (group scaling). Coalesces into one undo step per
    /// handle drag.
    pub fn scale_selection(&mut self, factor: f32, around: Option<[f32; 2]>) -> bool {
        if self.selection.is_empty() || !factor.is_finite() || factor <= 0.0 {
            return false;
        }
        self.record(Tag::Handle);
        for i in self.selection.clone() {
            if let Some(c0) = around {
                let c = self.shapes[i].center();
                self.shapes[i].set_center([
                    c0[0] + (c[0] - c0[0]) * factor,
                    c0[1] + (c[1] - c0[1]) * factor,
                ]);
            }
            self.scale_index(i, factor);
        }
        self.mark_posed_selection();
        true
    }

    /// Rotate every selected shape by `delta`; with `around`, positions
    /// orbit that point too (group rotation).
    pub fn rotate_selection(&mut self, delta: f32, around: Option<[f32; 2]>) -> bool {
        if self.selection.is_empty() || !delta.is_finite() {
            return false;
        }
        self.record(Tag::Handle);
        let (sn, cs) = delta.sin_cos();
        for &i in &self.selection {
            if let Some(c0) = around {
                let c = self.shapes[i].center();
                let d = [c[0] - c0[0], c[1] - c0[1]];
                self.shapes[i]
                    .set_center([c0[0] + d[0] * cs - d[1] * sn, c0[1] + d[0] * sn + d[1] * cs]);
            }
            self.shapes[i].rotate_by(delta);
        }
        self.mark_posed_selection();
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use spark_render::Shape;

    /// A stack of `n` loose shapes, bottom to top.
    fn stack(n: usize) -> Editor {
        let mut e = Editor::empty();
        for k in 0..n {
            e.shapes.push(Shape::circle([k as f32 * 10.0, 0.0], 10.0));
            e.names.push(String::new());
            e.anim.push(crate::anim::ShapeAnim::default());
            e.react.push([1.0; 3]);
            e.group.push(0);
            e.hidden.push(false);
            e.folder.push(0);
        }
        e
    }

    fn sorted(e: &Editor) -> Vec<usize> {
        let mut v = e.selection.clone();
        v.sort_unstable();
        v
    }

    #[test]
    fn shift_click_spans_from_the_anchor() {
        let mut e = stack(6);
        e.select(Some(1));
        assert!(e.select_range(4));
        assert_eq!(e.primary(), Some(4), "the clicked row stays primary");
        assert_eq!(sorted(&e), vec![1, 2, 3, 4]);
    }

    #[test]
    fn shift_click_spans_downward_too() {
        let mut e = stack(6);
        e.select(Some(4));
        assert!(e.select_range(2));
        assert_eq!(e.primary(), Some(2));
        assert_eq!(sorted(&e), vec![2, 3, 4]);
    }

    #[test]
    fn shift_click_respans_from_the_same_anchor() {
        // The anchor must not ratchet along behind each Shift+click: the
        // second one re-spans from the original plain click.
        let mut e = stack(6);
        e.select(Some(3));
        e.select_range(5);
        e.select_range(1);
        assert_eq!(e.primary(), Some(1));
        assert_eq!(sorted(&e), vec![1, 2, 3]);
    }

    #[test]
    fn shift_click_without_an_anchor_is_a_plain_click() {
        let mut e = stack(4);
        assert!(e.select_range(2));
        assert_eq!(e.selection, vec![2]);
    }

    #[test]
    fn reorder_voids_the_anchor() {
        // Reordering shuffles indices, so the stored anchor stops meaning
        // what it did — the next Shift+click has to start fresh.
        let mut e = stack(5);
        e.select(Some(0));
        e.move_layer(0, 3);
        assert!(e.range_anchor.is_none());
    }
}
