//! Drag snapping (grid + smart guides) and the display overlay list.

use spark_render::{CANVAS_H, CANVAS_W, Shape};

use super::Editor;

impl Editor {
    /// After a raw move, pull the primary's center onto the grid or a smart
    /// guide (canvas center, other shapes' centers) and drag the whole
    /// selection with it. Corrections are recomputed fresh per move, so the
    /// snap is sticky within the threshold and escapes past it.
    pub(super) fn update_snap(&mut self) {
        self.guides.clear();
        let Some(p) = self.primary() else { return };
        let c = self.shapes[p].center();
        let mut dx = 0.0;
        let mut dy = 0.0;
        if self.snap_grid {
            const G: f32 = 60.0;
            dx = (c[0] / G).round() * G - c[0];
            dy = (c[1] / G).round() * G - c[1];
        } else if self.smart_guides {
            const T: f32 = 9.0;
            let mut best_x: Option<f32> = None;
            let mut best_y: Option<f32> = None;
            let mut consider = |x: f32, y: f32| {
                if (x - c[0]).abs() < T
                    && best_x.is_none_or(|b| (x - c[0]).abs() < (b - c[0]).abs())
                {
                    best_x = Some(x);
                }
                if (y - c[1]).abs() < T
                    && best_y.is_none_or(|b| (y - c[1]).abs() < (b - c[1]).abs())
                {
                    best_y = Some(y);
                }
            };
            consider(CANVAS_W * 0.5, CANVAS_H * 0.5);
            for (i, s) in self.shapes.iter().enumerate() {
                if !self.selection.contains(&i) {
                    let sc = s.center();
                    consider(sc[0], sc[1]);
                }
            }
            if let Some(x) = best_x {
                dx = x - c[0];
                self.guides.push((true, x));
            }
            if let Some(y) = best_y {
                dy = y - c[1];
                self.guides.push((false, y));
            }
        }
        if dx != 0.0 || dy != 0.0 {
            for &i in &self.selection {
                self.shapes[i].translate([dx, dy]);
            }
        }
    }

    /// The document plus editor overlays (selection halos, smart guides).
    /// Document shapes come first, so `shapes().len()` counts them for
    /// render-time effects.
    pub fn display_shapes(&self) -> Vec<Shape> {
        let mut v = Vec::with_capacity(self.shapes.len() + self.selection.len() + 2);
        v.extend_from_slice(&self.shapes);
        for &i in &self.selection {
            v.push(self.shapes[i].selection_halo());
        }
        // Smart-guide lines, drawn as pure light across the whole stage.
        for &(vertical, at) in &self.guides {
            let mut g = if vertical {
                Shape::line([at, 0.0], [at, CANVAS_H], 1.2)
            } else {
                Shape::line([0.0, at], [CANVAS_W, at], 1.2)
            };
            g = g.color(1.0, 0.78, 0.09).intensity(0.8).glow(4.0);
            g.set_additive(true);
            v.push(g);
        }
        v
    }
}
