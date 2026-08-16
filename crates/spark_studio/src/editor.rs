//! The comp editor: tools, selection, and direct manipulation on the canvas.
//! Status feedback prints to the terminal until SparkUI text rendering lands.
//!
//! Mutating methods return `true` when the visible state changed, so the app
//! only redraws when something actually happened.

use std::path::Path;

use spark_render::{CANVAS_H, CANVAS_W, Shape, Viewport};

const PALETTE: [[f32; 3]; 6] = [
    [1.00, 0.16, 0.85], // magenta
    [0.16, 0.75, 1.00], // cyan
    [0.55, 0.25, 1.00], // violet
    [1.00, 0.45, 0.10], // ember
    [0.10, 1.00, 0.55], // acid
    [1.00, 0.95, 0.30], // laser
];
const PALETTE_NAMES: [&str; 6] = ["magenta", "cyan", "violet", "ember", "acid", "laser"];

pub const COMP_PATH: &str = "comp.spark";

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Tool {
    Select,
    Circle,
    Box,
    Polygon,
    Line,
}

/// An animatable/editable property of the selected shape.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Prop {
    X,
    Y,
    Rotation,
    Glow,
    Brightness,
    Sides,
}

/// Snapshot of the selected shape's properties for the inspector.
pub struct Props {
    pub x: f32,
    pub y: f32,
    pub rotation: f32,
    pub glow: f32,
    pub brightness: f32,
    pub sides: Option<u32>,
}

enum Drag {
    Draw,
    Move { last: [f32; 2] },
}

pub struct Editor {
    shapes: Vec<Shape>,
    selection: Option<usize>,
    tool: Tool,
    drag: Option<Drag>,
    palette: usize,
    sides: u32,
    press: [f32; 2],
    cursor: [f32; 2],
}

impl Editor {
    pub fn new() -> Self {
        let mut editor = Self {
            shapes: Vec::new(),
            selection: None,
            tool: Tool::Select,
            drag: None,
            palette: 0,
            sides: 5,
            press: [0.0; 2],
            cursor: [0.0; 2],
        };
        if Path::new(COMP_PATH).exists() {
            editor.load();
        }
        editor
    }

    /// Window-space cursor (physical px) -> canvas units within the viewport
    /// region, then drive any active drag.
    pub fn set_cursor(&mut self, px: f64, py: f64, vp: Viewport) -> bool {
        let scale = (vp.w / CANVAS_W).min(vp.h / CANVAS_H).max(0.0001);
        let ox = vp.x + (vp.w - CANVAS_W * scale) * 0.5;
        let oy = vp.y + (vp.h - CANVAS_H * scale) * 0.5;
        let now = [(px as f32 - ox) / scale, (py as f32 - oy) / scale];
        self.cursor = now;
        match &mut self.drag {
            Some(Drag::Draw) => {
                if let Some(i) = self.selection {
                    self.shapes[i] =
                        draw_shape(self.tool, self.press, now, self.sides, PALETTE[self.palette]);
                }
                true
            }
            Some(Drag::Move { last }) => {
                let d = [now[0] - last[0], now[1] - last[1]];
                *last = now;
                if let Some(i) = self.selection {
                    self.shapes[i].translate(d);
                }
                true
            }
            None => false,
        }
    }

    pub fn mouse_down(&mut self) -> bool {
        if self.tool == Tool::Select {
            let old = self.selection;
            self.selection = self.pick(self.cursor);
            if self.selection.is_some() {
                self.drag = Some(Drag::Move { last: self.cursor });
            }
            old != self.selection
        } else {
            self.press = self.cursor;
            self.shapes.push(draw_shape(
                self.tool,
                self.press,
                self.cursor,
                self.sides,
                PALETTE[self.palette],
            ));
            self.selection = Some(self.shapes.len() - 1);
            self.drag = Some(Drag::Draw);
            true
        }
    }

    pub fn mouse_up(&mut self) -> bool {
        let mut dirty = false;
        if let Some(Drag::Draw) = self.drag {
            // A click with no drag leaves an accidental speck — discard it.
            if dist(self.press, self.cursor) < 3.0 {
                if let Some(i) = self.selection.take() {
                    self.shapes.remove(i);
                    dirty = true;
                }
            }
        }
        self.drag = None;
        dirty
    }

    fn pick(&self, p: [f32; 2]) -> Option<usize> {
        self.shapes
            .iter()
            .enumerate()
            .rev()
            .find(|(_, s)| s.distance(p) <= 14.0)
            .map(|(i, _)| i)
    }

    pub fn wheel(&mut self, dy: f32, rotate: bool) -> bool {
        let Some(i) = self.selection else {
            return false;
        };
        if rotate {
            self.shapes[i].rotate_by(dy * 0.06);
        } else {
            self.shapes[i].scale_by((1.0 + dy * 0.08).clamp(0.5, 2.0));
        }
        true
    }

    pub fn char_key(&mut self, key: &str, ctrl: bool) -> bool {
        match (ctrl, key) {
            (true, "s") => {
                self.save();
                false
            }
            (true, "o") => {
                self.load();
                true
            }
            (false, "1") => self.set_tool(Tool::Select),
            (false, "2") => self.set_tool(Tool::Circle),
            (false, "3") => self.set_tool(Tool::Box),
            (false, "4") => self.set_tool(Tool::Polygon),
            (false, "5") => self.set_tool(Tool::Line),
            (false, "q") => self.with_selected(|s| s.rotate_by(-0.0873)),
            (false, "e") => self.with_selected(|s| s.rotate_by(0.0873)),
            (false, "[") => self.adjust_sides(-1),
            (false, "]") => self.adjust_sides(1),
            (false, "c") => self.cycle_color(),
            (false, "t") => self.with_selected(Shape::toggle_outline),
            (false, "a") => self.with_selected(|s| s.add_glow(4.0)),
            (false, "z") => self.with_selected(|s| s.add_glow(-4.0)),
            (false, "w") => self.with_selected(|s| s.add_intensity(0.1)),
            (false, "s") => self.with_selected(|s| s.add_intensity(-0.1)),
            (false, "x") => self.delete_selected(),
            _ => false,
        }
    }

    pub fn tool(&self) -> Tool {
        self.tool
    }

    pub fn selected_props(&self) -> Option<Props> {
        let s = &self.shapes[self.selection?];
        let c = s.center();
        Some(Props {
            x: c[0],
            y: c[1],
            rotation: s.rotation(),
            glow: s.glow_radius(),
            brightness: s.brightness(),
            sides: s.sides(),
        })
    }

    pub fn set_prop(&mut self, prop: Prop, value: f32) -> bool {
        let Some(i) = self.selection else {
            return false;
        };
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
            Prop::Glow => s.set_glow(value),
            Prop::Brightness => s.set_brightness(value),
            Prop::Sides => s.set_sides(value.round() as u32),
        }
        true
    }

    pub fn choose_tool(&mut self, tool: Tool) {
        self.set_tool(tool);
    }

    fn with_selected(&mut self, f: impl FnOnce(&mut Shape)) -> bool {
        if let Some(i) = self.selection {
            f(&mut self.shapes[i]);
            true
        } else {
            false
        }
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
        self.with_selected(|s| s.set_sides(sides))
    }

    fn cycle_color(&mut self) -> bool {
        self.palette = (self.palette + 1) % PALETTE.len();
        let rgb = PALETTE[self.palette];
        println!("color: {}", PALETTE_NAMES[self.palette]);
        self.with_selected(|s| s.set_rgb(rgb))
    }

    pub fn delete_selected(&mut self) -> bool {
        if let Some(i) = self.selection.take() {
            self.shapes.remove(i);
            println!("deleted shape ({} left)", self.shapes.len());
            true
        } else {
            false
        }
    }

    pub fn deselect(&mut self) -> bool {
        self.selection.take().is_some()
    }

    /// The document plus editor overlays (canvas frame, selection halo).
    pub fn display_shapes(&self) -> Vec<Shape> {
        let mut v = Vec::with_capacity(self.shapes.len() + 2);
        v.push(
            Shape::rect(
                [CANVAS_W * 0.5, CANVAS_H * 0.5],
                [CANVAS_W * 0.5 - 2.0, CANVAS_H * 0.5 - 2.0],
            )
            .stroke(1.5)
            .glow(5.0)
            .color(0.45, 0.45, 0.65)
            .intensity(0.22),
        );
        v.extend_from_slice(&self.shapes);
        if let Some(i) = self.selection {
            v.push(self.shapes[i].selection_halo());
        }
        v
    }

    pub fn save(&self) {
        let mut out = String::from("spark-comp v0\n");
        for shape in &self.shapes {
            let vals: Vec<String> = shape.to_array().iter().map(|f| format!("{f}")).collect();
            out.push_str(&vals.join(" "));
            out.push('\n');
        }
        match std::fs::write(COMP_PATH, out) {
            Ok(()) => println!("saved {} shapes -> {COMP_PATH}", self.shapes.len()),
            Err(e) => println!("save failed: {e}"),
        }
    }

    pub fn load(&mut self) {
        let text = match std::fs::read_to_string(COMP_PATH) {
            Ok(t) => t,
            Err(e) => {
                println!("load failed: {e}");
                return;
            }
        };
        let mut shapes = Vec::new();
        for line in text.lines().skip(1) {
            let vals: Vec<f32> = line
                .split_whitespace()
                .filter_map(|t| t.parse().ok())
                .collect();
            if vals.len() == 14 {
                let mut arr = [0.0f32; 14];
                arr.copy_from_slice(&vals);
                shapes.push(Shape::from_array(arr));
            }
        }
        println!("loaded {} shapes from {COMP_PATH}", shapes.len());
        self.shapes = shapes;
        self.selection = None;
        self.drag = None;
    }
}

fn dist(a: [f32; 2], b: [f32; 2]) -> f32 {
    ((b[0] - a[0]).powi(2) + (b[1] - a[1]).powi(2)).sqrt()
}

fn draw_shape(tool: Tool, press: [f32; 2], cursor: [f32; 2], sides: u32, rgb: [f32; 3]) -> Shape {
    let d = dist(press, cursor).max(3.0);
    let shape = match tool {
        Tool::Circle => Shape::circle(press, d).stroke(4.0),
        Tool::Box => Shape::rect(
            press,
            [
                (cursor[0] - press[0]).abs().max(3.0),
                (cursor[1] - press[1]).abs().max(3.0),
            ],
        )
        .stroke(4.0),
        Tool::Polygon => Shape::ngon(press, d, sides).stroke(4.0),
        Tool::Line => Shape::line(press, cursor, 3.0),
        Tool::Select => unreachable!("draw_shape is never called with Select"),
    };
    shape
        .color(rgb[0], rgb[1], rgb[2])
        .intensity(1.4)
        .glow(if tool == Tool::Line { 24.0 } else { 30.0 })
}
