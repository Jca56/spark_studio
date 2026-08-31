//! Drag snapping (grid + smart guides) and the display overlay list.

use spark_render::Shape;

use super::Editor;

impl Editor {
    /// Place the primary's center at `free` (the cursor's unsnapped intent),
    /// quantized to the grid or a smart guide, and drag the whole selection
    /// with it. Snapping always quantizes `free` — never the shape's
    /// current position — so escaping a snap only takes moving the cursor
    /// past the threshold.
    pub(super) fn move_selection_to(&mut self, free: [f32; 2]) {
        self.guides.clear();
        let Some(p) = self.primary() else { return };
        let mut target = free;
        if self.snap_grid {
            const G: f32 = 60.0;
            target = [(free[0] / G).round() * G, (free[1] / G).round() * G];
        } else if self.smart_guides {
            const T: f32 = 9.0;
            let mut best_x: Option<f32> = None;
            let mut best_y: Option<f32> = None;
            let mut consider = |x: f32, y: f32| {
                if (x - free[0]).abs() < T
                    && best_x.is_none_or(|b| (x - free[0]).abs() < (b - free[0]).abs())
                {
                    best_x = Some(x);
                }
                if (y - free[1]).abs() < T
                    && best_y.is_none_or(|b| (y - free[1]).abs() < (b - free[1]).abs())
                {
                    best_y = Some(y);
                }
            };
            consider(self.canvas[0] * 0.5, self.canvas[1] * 0.5);
            for (i, s) in self.shapes.iter().enumerate() {
                if !self.selection.contains(&i) {
                    let sc = s.center();
                    consider(sc[0], sc[1]);
                }
            }
            if let Some(x) = best_x {
                target[0] = x;
                self.guides.push((true, x));
            }
            if let Some(y) = best_y {
                target[1] = y;
                self.guides.push((false, y));
            }
        }
        let c = self.shapes[p].center();
        let d = [target[0] - c[0], target[1] - c[1]];
        if d != [0.0, 0.0] {
            for &i in &self.selection {
                self.shapes[i].translate(d);
            }
            self.mark_posed_selection();
        }
    }

    /// The document plus editor overlays (selection halos, smart guides).
    /// Document shapes come first, so `shapes().len()` counts them for
    /// render-time effects.
    /// A shape with its folder's transform composed on — what actually gets
    /// drawn, picked and outlined. Loose shapes and identity folders pass
    /// straight through.
    pub fn posed_shape(&self, i: usize, shape: Shape) -> Shape {
        let mut out = shape;
        // Effects paint onto the display copy, never the document.
        if let Some(stack) = self.fx.get(i) {
            crate::fx::resolve(&mut out, stack);
        }
        let id = self.folder_of(i);
        if id == 0 {
            return out;
        }
        match self.folder(id) {
            Some(f) if !f.is_identity() => {
                f.compose(&mut out, self.folder_pivot(id));
                out
            }
            _ => out,
        }
    }

    pub fn display_shapes(&self) -> Vec<Shape> {
        let mut v = Vec::with_capacity(self.shapes.len() + self.selection.len() * 2 + 2);
        // Hidden shapes keep their slot (render-time audio react indexes by
        // position) but draw as nothing.
        for (i, s) in self.shapes.iter().enumerate() {
            if self.shape_hidden(i) {
                v.push(Shape::circle([-1e5, -1e5], 0.001).intensity(0.0));
            } else {
                v.push(self.posed_shape(i, *s));
            }
        }
        for &i in &self.selection {
            if self.shape_hidden(i) {
                continue;
            }
            // Two-coat ants: a solid black stroke with a thinner gold
            // dashed light riding its center — readable over any shape
            // color, which white-plus-shape-color never was.
            // Ants ride the composed pose, so a shape inside a moved folder
            // is outlined where it's actually drawn.
            let halo = self.posed_shape(i, self.shapes[i]).selection_halo();
            let mut back = halo.stroke(2.0).color(0.0, 0.0, 0.0).intensity(1.0);
            back.set_additive(false);
            v.push(back);
            v.push(halo.stroke(1.3).color(1.0, 0.78, 0.09));
        }
        // Smart-guide lines, drawn as pure light across the whole stage.
        for &(vertical, at) in &self.guides {
            let mut g = if vertical {
                Shape::line([at, 0.0], [at, self.canvas[1]], 1.2)
            } else {
                Shape::line([0.0, at], [self.canvas[0], at], 1.2)
            };
            g = g.color(1.0, 0.78, 0.09).intensity(0.8).glow(4.0);
            g.set_additive(true);
            v.push(g);
        }
        v
    }
}
