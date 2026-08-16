//! The comp editor: tools, selection, and direct manipulation on the canvas.
//! Status feedback prints to the terminal until SparkUI text rendering lands.
//!
//! Selection is a set: the last entry is the *primary* (what the inspector
//! shows); relative edits and moves apply to every selected shape.
//! Mutating methods return `true` when the visible state changed, so the app
//! only redraws when something actually happened.

use std::path::Path;

use spark_render::{CANVAS_H, CANVAS_W, Shape, Viewport};

mod io;
mod snap;

use crate::history::{History, Snap, Tag};
pub use crate::props::{PALETTE, Prop, Props, Tool};
use crate::props::{PALETTE_NAMES, StyleClip, dist, draw_shape, remap};

pub const COMP_PATH: &str = "comp.spark";

enum Drag {
    Draw,
    Move {
        last: [f32; 2],
        /// The primary's *unsnapped* center, tracking the cursor's intent.
        /// Snapping quantizes this — never the already-snapped position,
        /// which would gridlock the drag.
        free: [f32; 2],
    },
}

pub struct Editor {
    shapes: Vec<Shape>,
    /// User-given layer names, parallel to `shapes` (empty = auto-label).
    names: Vec<String>,
    /// Selected shape indices; the last entry is the primary.
    selection: Vec<usize>,
    tool: Tool,
    drag: Option<Drag>,
    palette: usize,
    sides: u32,
    press: [f32; 2],
    cursor: [f32; 2],
    history: History,
    /// The comp's audio track, saved with the document.
    audio_path: Option<String>,
    /// Snap the dragged shape's center to the 60-unit canvas grid.
    pub snap_grid: bool,
    /// Snap to canvas center and other shapes' centers while dragging.
    pub smart_guides: bool,
    /// Active alignment guides: (vertical?, canvas coordinate).
    guides: Vec<(bool, f32)>,
    /// Ctrl+C'd style, waiting for Ctrl+V.
    style_clip: Option<StyleClip>,
}

impl Editor {
    pub fn new() -> Self {
        let mut editor = Self {
            shapes: Vec::new(),
            names: Vec::new(),
            selection: Vec::new(),
            tool: Tool::Select,
            drag: None,
            palette: 0,
            sides: 5,
            press: [0.0; 2],
            cursor: [0.0; 2],
            history: History::new(),
            audio_path: None,
            snap_grid: false,
            smart_guides: true,
            guides: Vec::new(),
            style_clip: None,
        };
        if Path::new(COMP_PATH).exists() {
            editor.load(COMP_PATH);
            // The startup load is the baseline, not an undoable edit.
            editor.history = History::new();
        }
        editor
    }

    fn snap(&self) -> Snap {
        Snap {
            shapes: self.shapes.clone(),
            names: self.names.clone(),
            selection: self.selection.clone(),
        }
    }

    fn apply(&mut self, snap: Snap) {
        self.shapes = snap.shapes;
        self.names = snap.names;
        self.selection = snap.selection;
        self.drag = None;
    }

    /// Record a coalescible change on the selection (skipped when nothing is
    /// selected, so the document can't gain no-op undo steps).
    fn record(&mut self, tag: Tag) {
        if !self.selection.is_empty() {
            let s = self.snap();
            self.history.change(tag, s);
        }
    }

    pub fn undo(&mut self) -> bool {
        let cur = self.snap();
        match self.history.undo(cur) {
            Some(s) => {
                self.apply(s);
                println!("undo");
                true
            }
            None => {
                println!("nothing to undo");
                false
            }
        }
    }

    pub fn redo(&mut self) -> bool {
        let cur = self.snap();
        match self.history.redo(cur) {
            Some(s) => {
                self.apply(s);
                println!("redo");
                true
            }
            None => {
                println!("nothing to redo");
                false
            }
        }
    }

    /// A mouse release ended whatever gesture was running; the next change
    /// starts a fresh undo step. Gestures that ended where they started
    /// (a layer dragged back to its slot) leave no undo step behind.
    pub fn end_gesture(&mut self) {
        let s = self.snap();
        self.history.drop_noop(&s);
        self.history.commit();
    }

    /// Window-space cursor (physical px) -> canvas units within the viewport
    /// region, then drive any active drag.
    pub fn set_cursor(&mut self, px: f64, py: f64, vp: Viewport) -> bool {
        let scale = (vp.w / CANVAS_W).min(vp.h / CANVAS_H).max(0.0001);
        let ox = vp.x + (vp.w - CANVAS_W * scale) * 0.5;
        let oy = vp.y + (vp.h - CANVAS_H * scale) * 0.5;
        let now = [(px as f32 - ox) / scale, (py as f32 - oy) / scale];
        self.cursor = now;
        let free_target = if let Some(Drag::Move { last, free }) = &mut self.drag {
            let d = [now[0] - last[0], now[1] - last[1]];
            *last = now;
            free[0] += d[0];
            free[1] += d[1];
            Some(*free)
        } else {
            None
        };
        if let Some(free) = free_target {
            self.move_selection_to(free);
            return true;
        }
        match &mut self.drag {
            Some(Drag::Draw) => {
                if let Some(&i) = self.selection.last() {
                    self.shapes[i] = draw_shape(
                        self.tool,
                        self.press,
                        now,
                        self.sides,
                        PALETTE[self.palette],
                    );
                }
                true
            }
            _ => false,
        }
    }

    /// Ctrl+click toggles membership in the selection; a plain click on an
    /// already-selected shape keeps the set (so groups drag together).
    pub fn mouse_down(&mut self, ctrl: bool) -> bool {
        if self.tool == Tool::Select {
            let hit = self.pick(self.cursor);
            let old = self.selection.clone();
            match hit {
                Some(i) if ctrl => {
                    self.history.commit();
                    match self.selection.iter().position(|&s| s == i) {
                        Some(pos) => {
                            self.selection.remove(pos);
                        }
                        None => self.selection.push(i),
                    }
                }
                Some(i) => {
                    if !self.selection.contains(&i) {
                        self.selection = vec![i];
                    }
                    // Pre-move state; dropped again at mouse_up if nothing
                    // moved.
                    let s = self.snap();
                    self.history.push(s);
                    let free = self.shapes[self.primary().unwrap_or(i)].center();
                    self.drag = Some(Drag::Move {
                        last: self.cursor,
                        free,
                    });
                }
                None if !ctrl => self.selection.clear(),
                None => {}
            }
            old != self.selection
        } else {
            self.press = self.cursor;
            let s = self.snap();
            self.history.push(s);
            self.shapes.push(draw_shape(
                self.tool,
                self.press,
                self.cursor,
                self.sides,
                PALETTE[self.palette],
            ));
            self.names.push(String::new());
            self.selection = vec![self.shapes.len() - 1];
            self.drag = Some(Drag::Draw);
            true
        }
    }

    pub fn mouse_up(&mut self) -> bool {
        let mut dirty = false;
        if let Some(Drag::Draw) = self.drag {
            // A click with no drag leaves an accidental speck — discard it.
            if dist(self.press, self.cursor) < 3.0
                && let Some(&i) = self.selection.last()
            {
                self.shapes.remove(i);
                self.names.remove(i);
                self.selection.clear();
                dirty = true;
            }
        }
        if self.drag.take().is_some() {
            // Discarded specks and moves that never moved undo to nothing —
            // drop the snapshot the gesture pushed.
            let s = self.snap();
            self.history.drop_noop(&s);
        }
        self.guides.clear();
        self.history.commit();
        dirty
    }

    fn pick(&self, p: [f32; 2]) -> Option<usize> {
        self.shapes
            .iter()
            .enumerate()
            .rev()
            .find(|(_, s)| s.pick_distance(p) <= 14.0)
            .map(|(i, _)| i)
    }

    pub fn wheel(&mut self, dy: f32, rotate: bool) -> bool {
        if self.selection.is_empty() {
            return false;
        }
        self.record(Tag::Wheel);
        let factor = (1.0 + dy * 0.08).clamp(0.5, 2.0);
        let rot = dy * 0.06;
        self.with_selected(|s| {
            if rotate {
                s.rotate_by(rot);
            } else {
                s.scale_by(factor);
            }
        })
    }

    pub fn char_key(&mut self, key: &str, ctrl: bool, shift: bool) -> bool {
        match (ctrl, key) {
            (true, "z") if shift => self.redo(),
            (true, "z") => self.undo(),
            (true, "c") => self.copy_style(),
            (true, "v") => self.paste_style(),
            (false, "1") => self.set_tool(Tool::Select),
            (false, "2") => self.set_tool(Tool::Circle),
            (false, "3") => self.set_tool(Tool::Box),
            (false, "4") => self.set_tool(Tool::Polygon),
            (false, "5") => self.set_tool(Tool::Line),
            (false, "q") => self.nudge(Tag::KeyRotate, |s| s.rotate_by(-0.0873)),
            (false, "e") => self.nudge(Tag::KeyRotate, |s| s.rotate_by(0.0873)),
            (false, "[") => self.adjust_sides(-1),
            (false, "]") => self.adjust_sides(1),
            (false, "c") => self.cycle_color(),
            (false, "t") => {
                let flip = self
                    .primary()
                    .and_then(|i| self.shapes[i].outline())
                    .map(|o| !o);
                match flip {
                    Some(on) => self.set_outline(on),
                    None => false,
                }
            }
            (false, "a") => self.nudge(Tag::KeyGlow, |s| s.add_glow(4.0)),
            (false, "z") => self.nudge(Tag::KeyGlow, |s| s.add_glow(-4.0)),
            (false, "w") => self.nudge(Tag::KeyBright, |s| s.add_intensity(0.1)),
            (false, "s") => self.nudge(Tag::KeyBright, |s| s.add_intensity(-0.1)),
            (false, "x") => self.delete_selected(),
            _ => false,
        }
    }

    /// A keyboard adjustment: coalesces with the run of same-tag presses.
    fn nudge(&mut self, tag: Tag, f: impl Fn(&mut Shape)) -> bool {
        self.record(tag);
        self.with_selected(f)
    }

    pub fn tool(&self) -> Tool {
        self.tool
    }

    /// The primary selection — the shape the inspector describes.
    pub fn primary(&self) -> Option<usize> {
        self.selection.last().copied()
    }

    pub fn selected_props(&self) -> Option<Props> {
        let s = &self.shapes[self.primary()?];
        let c = s.center();
        let rgb = s.rgb();
        Some(Props {
            x: c[0],
            y: c[1],
            rotation: s.rotation(),
            size: s.size(),
            glow: s.glow_radius(),
            brightness: s.brightness(),
            sides: s.sides(),
            box_size: s.box_size(),
            thickness: s.thickness(),
            palette: PALETTE.iter().position(|p| *p == rgb),
            outline: s.outline(),
            additive: s.additive(),
        })
    }

    /// Absolute-value sliders write to the primary shape.
    pub fn set_prop(&mut self, prop: Prop, value: f32) -> bool {
        let Some(i) = self.primary() else {
            return false;
        };
        self.record(Tag::Prop(prop));
        let s = &mut self.shapes[i];
        match prop {
            Prop::X => {
                let c = s.center();
                s.set_center([value, c[1]]);
            }
            Prop::Y => {
                let c = s.center();
                s.set_center([c[0], value]);
            }
            Prop::Rotation => s.set_rotation(value),
            Prop::Scale => {
                let cur = s.size();
                if cur > 0.001 {
                    s.scale_by(value / cur);
                }
            }
            Prop::Width => s.set_box_width(value),
            Prop::Height => s.set_box_height(value),
            Prop::Glow => s.set_glow(value),
            Prop::Brightness => s.set_brightness(value),
            Prop::Sides => s.set_sides(value.round() as u32),
            Prop::Thickness => s.set_thickness(value),
        }
        true
    }

    pub fn choose_tool(&mut self, tool: Tool) {
        self.set_tool(tool);
    }

    /// Pick a palette color: becomes the draw color and recolors the selection.
    pub fn set_color_index(&mut self, i: usize) -> bool {
        self.palette = i % PALETTE.len();
        let rgb = PALETTE[self.palette];
        if let [sel] = self.selection[..]
            && self.shapes[sel].rgb() == rgb
        {
            return false;
        }
        self.record(Tag::Color);
        self.with_selected(|s| s.set_rgb(rgb))
    }

    pub fn set_outline(&mut self, on: bool) -> bool {
        let Some(i) = self.primary() else {
            return false;
        };
        // `None` (a line) and already-matching both mean nothing to do.
        if self.shapes[i].outline() != Some(!on) {
            return false;
        }
        let s = self.snap();
        self.history.push(s);
        for &j in &self.selection {
            self.shapes[j].set_outline(on);
        }
        true
    }

    pub fn set_additive(&mut self, on: bool) -> bool {
        let Some(i) = self.primary() else {
            return false;
        };
        if self.shapes[i].additive() == on {
            return false;
        }
        let s = self.snap();
        self.history.push(s);
        for &j in &self.selection {
            self.shapes[j].set_additive(on);
        }
        true
    }

    /// Latest cursor position in canvas units.
    pub fn cursor(&self) -> [f32; 2] {
        self.cursor
    }

    /// Uniform-scale every selected shape; with `around`, positions orbit
    /// that point too (group scaling). Coalesces into one undo step per
    /// handle drag.
    pub fn scale_selection(&mut self, factor: f32, around: Option<[f32; 2]>) -> bool {
        if self.selection.is_empty() || !factor.is_finite() || factor <= 0.0 {
            return false;
        }
        self.record(Tag::Handle);
        for &i in &self.selection {
            if let Some(c0) = around {
                let c = self.shapes[i].center();
                self.shapes[i].set_center([
                    c0[0] + (c[0] - c0[0]) * factor,
                    c0[1] + (c[1] - c0[1]) * factor,
                ]);
            }
            self.shapes[i].scale_by(factor);
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

    pub fn shapes(&self) -> &[Shape] {
        &self.shapes
    }

    pub fn selection(&self) -> &[usize] {
        &self.selection
    }

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

    /// Apply an edit to every selected shape.
    fn with_selected(&mut self, f: impl Fn(&mut Shape)) -> bool {
        if self.selection.is_empty() {
            return false;
        }
        for &i in &self.selection {
            f(&mut self.shapes[i]);
        }
        true
    }

    fn set_tool(&mut self, tool: Tool) -> bool {
        self.tool = tool;
        if tool == Tool::Polygon {
            println!("tool: Polygon ({} sides)", self.sides);
        } else {
            println!("tool: {tool:?}");
        }
        // The toolbar highlights the active tool, so switching is visual now.
        true
    }

    fn adjust_sides(&mut self, delta: i32) -> bool {
        self.sides = (self.sides as i32 + delta).clamp(3, 24) as u32;
        let sides = self.sides;
        println!("polygon sides: {}", self.sides);
        if self.selection.iter().any(|&i| self.shapes[i].is_ngon()) {
            self.record(Tag::Sides);
        }
        self.with_selected(|s| s.set_sides(sides))
    }

    fn cycle_color(&mut self) -> bool {
        self.palette = (self.palette + 1) % PALETTE.len();
        let rgb = PALETTE[self.palette];
        println!("color: {}", PALETTE_NAMES[self.palette]);
        self.record(Tag::Color);
        self.with_selected(|s| s.set_rgb(rgb))
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
}
