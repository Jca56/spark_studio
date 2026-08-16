//! Selection management and whole-selection transforms.

use super::Editor;
use crate::history::Tag;
use crate::props::remap;

impl Editor {
    pub fn select(&mut self, i: Option<usize>) -> bool {
        self.history.commit();
        let new: Vec<usize> = i.into_iter().collect();
        let changed = self.selection != new;
        self.selection = new;
        changed
    }

    /// Ctrl+click on a layer row: toggle membership.
    pub fn toggle_select(&mut self, i: usize) -> bool {
        self.history.commit();
        match self.selection.iter().position(|&s| s == i) {
            Some(pos) => {
                self.selection.remove(pos);
            }
            None => self.selection.push(i),
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
        for s in &mut self.selection {
            *s = remap(*s, from, to);
        }
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
        }
        println!(
            "deleted {} shape(s) ({} left)",
            idx.len(),
            self.shapes.len()
        );
        true
    }

    pub fn deselect(&mut self) -> bool {
        self.history.commit();
        let had = !self.selection.is_empty();
        self.selection.clear();
        had
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
        true
    }
}
