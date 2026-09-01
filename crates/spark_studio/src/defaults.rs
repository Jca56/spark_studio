//! Draw defaults: what a shape looks like the moment it is drawn — one
//! set per tool, configurable *before* drawing, which nothing in Spark
//! allowed until the context menu's tool pages (Alva's spec, 2026-08-31).
//! Until then every shape was born one way and restyled afterwards with
//! the keyboard; the birth values here are exactly those looks, so a
//! fresh session draws what it always drew.
//!
//! Session state — a mode of the hand, like the dice and the snap
//! toggles — never in the document. Persistence is a follow-up.

use spark_render::{STAR_FORMS, Shape};

use crate::props::{Prop, Tool};

/// The look a tool draws with. One struct for every tool; a field a tool
/// has no use for is never read (a line has no fill to switch, a circle
/// has no sides).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ToolDefaults {
    /// Outline rather than fill — circle, box, polygon.
    pub outline: bool,
    /// Stroke half-width (an outline's, a line's) — and a star's radius,
    /// which rides the same slot on a field (`Shape::set_thickness`).
    pub thickness: f32,
    /// Glow radius. Zero is no Glow effect at all, not an effect at zero.
    pub glow: f32,
    pub brightness: f32,
    /// How much of the shape reaches the frame; 1 is solid.
    pub opacity: f32,
    /// Polygon side count.
    pub sides: u32,
    /// Star field: stars across the canvas, twinkle depth, twinkle rate,
    /// and which star it scatters (an index into [`STAR_FORMS`]).
    pub density: f32,
    pub twinkle: f32,
    pub rate: f32,
    pub form: usize,
}

impl ToolDefaults {
    /// The look a tool has always drawn with — plain: the colour you
    /// picked, at brightness 1, no halo, a 4-unit outline. A star field
    /// asks the renderer's own fresh field for its numbers, so there is
    /// one source of truth for what a sky starts as.
    pub fn birth(tool: Tool) -> Self {
        let plain = Self {
            outline: true,
            thickness: 4.0,
            glow: 0.0,
            brightness: 1.0,
            opacity: 1.0,
            sides: 5,
            density: 10.0,
            twinkle: 0.6,
            rate: 3.0,
            form: 0,
        };
        match tool {
            Tool::Line => Self {
                thickness: 3.0,
                ..plain
            },
            Tool::Stars => {
                let s = Shape::stars([0.0; 2], [1.0; 2], 0.0);
                Self {
                    thickness: s.thickness().unwrap_or(plain.thickness),
                    glow: s.glow_radius(),
                    // Tuned so the first drag already reads as a sky.
                    brightness: 1.4,
                    density: s.density().unwrap_or(plain.density),
                    twinkle: s.twinkle().unwrap_or(plain.twinkle),
                    rate: s.twinkle_rate().unwrap_or(plain.rate),
                    form: s.star_form().unwrap_or(0),
                    ..plain
                }
            }
            _ => plain,
        }
    }

    /// A slider's number, by the property it moves. Properties no default
    /// carries read as zero.
    pub fn get(&self, prop: Prop) -> f32 {
        match prop {
            Prop::Thickness => self.thickness,
            Prop::Glow => self.glow,
            Prop::Brightness => self.brightness,
            Prop::Opacity => self.opacity,
            Prop::Sides => self.sides as f32,
            Prop::Density => self.density,
            Prop::Twinkle => self.twinkle,
            Prop::TwinkleRate => self.rate,
            _ => 0.0,
        }
    }

    /// Move a slider. Fitted to the property's range the way a typed value
    /// is; sides land on a whole number.
    pub fn set(&mut self, prop: Prop, v: f32, canvas: [f32; 2]) {
        let v = crate::props::fit(prop, v, canvas);
        match prop {
            Prop::Thickness => self.thickness = v,
            Prop::Glow => self.glow = v,
            Prop::Brightness => self.brightness = v,
            Prop::Opacity => self.opacity = v,
            Prop::Sides => self.sides = v.round() as u32,
            Prop::Density => self.density = v,
            Prop::Twinkle => self.twinkle = v,
            Prop::TwinkleRate => self.rate = v,
            _ => {}
        }
    }

    /// Whether a slider does anything right now: an outline's thickness is
    /// nothing on a fill.
    pub fn slider_live(&self, tool: Tool, prop: Prop) -> bool {
        match (tool, prop) {
            (Tool::Circle | Tool::Box | Tool::Polygon, Prop::Thickness) => self.outline,
            _ => true,
        }
    }
}

/// One slider on a tool's page: the number it moves and what it is called.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SliderSpec {
    pub prop: Prop,
    pub label: &'static str,
}

const fn slider(prop: Prop, label: &'static str) -> SliderSpec {
    SliderSpec { prop, label }
}

// Alva's order (2026-08-31): Sides, Opacity, Brightness, Thickness, Glow
// — a star field's sky after.
const SHAPE_SLIDERS: [SliderSpec; 4] = [
    slider(Prop::Opacity, "Opacity"),
    slider(Prop::Brightness, "Brightness"),
    slider(Prop::Thickness, "Thickness"),
    slider(Prop::Glow, "Glow"),
];
const POLYGON_SLIDERS: [SliderSpec; 5] = [
    slider(Prop::Sides, "Sides"),
    slider(Prop::Opacity, "Opacity"),
    slider(Prop::Brightness, "Brightness"),
    slider(Prop::Thickness, "Thickness"),
    slider(Prop::Glow, "Glow"),
];
const STAR_SLIDERS: [SliderSpec; 7] = [
    slider(Prop::Opacity, "Opacity"),
    slider(Prop::Brightness, "Brightness"),
    slider(Prop::Thickness, "Size"),
    slider(Prop::Glow, "Glow"),
    slider(Prop::Density, "Density"),
    slider(Prop::Twinkle, "Twinkle"),
    slider(Prop::TwinkleRate, "Rate"),
];

/// The sliders a tool's page carries, in reading order. Lean on purpose
/// (Alva's call): what the keyboard already sets after the fact, and
/// nothing a shape can't carry.
pub fn sliders(tool: Tool) -> &'static [SliderSpec] {
    match tool {
        Tool::Select => &[],
        Tool::Circle | Tool::Box | Tool::Line => &SHAPE_SLIDERS,
        Tool::Polygon => &POLYGON_SLIDERS,
        Tool::Stars => &STAR_SLIDERS,
    }
}


/// A page's segmented switch, where the tool has a choice that isn't a
/// number.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Switch {
    /// Fill or outline — circle, box, polygon.
    FillOutline,
    /// Which star a field scatters.
    StarForm,
}

impl Switch {
    pub fn for_tool(tool: Tool) -> Option<Self> {
        match tool {
            Tool::Circle | Tool::Box | Tool::Polygon => Some(Self::FillOutline),
            Tool::Stars => Some(Self::StarForm),
            Tool::Select | Tool::Line => None,
        }
    }

    pub fn labels(self) -> &'static [&'static str] {
        match self {
            Self::FillOutline => &["Fill", "Outline"],
            Self::StarForm => &STAR_FORMS,
        }
    }

    /// Which segment is lit for these defaults.
    pub fn active(self, d: &ToolDefaults) -> usize {
        match self {
            Self::FillOutline => usize::from(d.outline),
            Self::StarForm => d.form.min(STAR_FORMS.len() - 1),
        }
    }

    /// A segment click.
    pub fn pick(self, d: &mut ToolDefaults, i: usize) {
        match self {
            Self::FillOutline => d.outline = i == 1,
            Self::StarForm => d.form = i.min(STAR_FORMS.len() - 1),
        }
    }
}

/// How a slider's readout prints its number: whole for counts and radii
/// the eye can't split, a decimal where it can.
pub fn readout(prop: Prop, v: f32) -> String {
    match prop {
        Prop::Sides | Prop::Density | Prop::Glow | Prop::Cone => format!("{v:.0}"),
        Prop::Thickness | Prop::TwinkleRate => format!("{v:.1}"),
        _ => format!("{v:.2}"),
    }
}

/// The drawing tools, in rail order — the ones that have defaults.
pub const DRAW_TOOLS: [Tool; 5] = [
    Tool::Circle,
    Tool::Box,
    Tool::Polygon,
    Tool::Line,
    Tool::Stars,
];

/// Every tool's defaults.
#[derive(Clone, Debug, PartialEq)]
pub struct Defaults {
    tools: [ToolDefaults; DRAW_TOOLS.len()],
}

impl Default for Defaults {
    fn default() -> Self {
        Self {
            tools: DRAW_TOOLS.map(ToolDefaults::birth),
        }
    }
}

impl Defaults {
    /// Move has none; asking for its slot hands back the circle's, which
    /// nothing draws with — `draw_shape` is never called for Move.
    fn slot(tool: Tool) -> usize {
        DRAW_TOOLS.iter().position(|&t| t == tool).unwrap_or(0)
    }

    pub fn get(&self, tool: Tool) -> &ToolDefaults {
        &self.tools[Self::slot(tool)]
    }

    pub fn get_mut(&mut self, tool: Tool) -> &mut ToolDefaults {
        &mut self.tools[Self::slot(tool)]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::props::draw_shape;
    use spark_render::ShapeKind;

    /// The defaults are the birth looks the tools always had: an outline
    /// at 4, brightness 1, no halo; a line at 3; a star field exactly what
    /// the renderer's own fresh field is, at 1.4.
    #[test]
    fn birth_defaults_draw_what_the_tools_always_drew() {
        let d = Defaults::default();
        let c = draw_shape(
            Tool::Circle,
            [300.0; 2],
            [400.0, 300.0],
            d.get(Tool::Circle),
            [1.0; 3],
        );
        assert_eq!(c.outline(), Some(true));
        assert_eq!(c.thickness(), Some(4.0));
        assert_eq!(c.brightness(), 1.0);
        assert_eq!(c.glow_radius(), 0.0, "glow is the effect's job at birth");
        let p = draw_shape(
            Tool::Polygon,
            [300.0; 2],
            [400.0, 300.0],
            d.get(Tool::Polygon),
            [1.0; 3],
        );
        assert_eq!(p.sides(), Some(5));
        let l = draw_shape(
            Tool::Line,
            [300.0; 2],
            [400.0, 300.0],
            d.get(Tool::Line),
            [1.0; 3],
        );
        assert_eq!(l.thickness(), Some(3.0));
        let s = draw_shape(
            Tool::Stars,
            [300.0; 2],
            [400.0, 380.0],
            d.get(Tool::Stars),
            [1.0; 3],
        );
        let fresh = Shape::stars([300.0; 2], [100.0, 80.0], 0.0);
        assert_eq!(s.kind(), ShapeKind::Stars);
        assert_eq!(s.density(), fresh.density());
        assert_eq!(s.thickness(), fresh.thickness());
        assert_eq!(s.twinkle(), fresh.twinkle());
        assert_eq!(s.twinkle_rate(), fresh.twinkle_rate());
        assert_eq!(s.star_form(), fresh.star_form());
        assert!((s.brightness() - 1.4).abs() < 1e-6);
        // The field's own glow is what its Glow effect is born with.
        assert_eq!(d.get(Tool::Stars).glow, fresh.glow_radius());
        assert!(d.get(Tool::Stars).glow > 0.0);
    }

    /// Moving a slider changes exactly what the next drawing does.
    #[test]
    fn a_moved_slider_reaches_the_next_shape() {
        let mut d = Defaults::default();
        let canvas = spark_render::CANVAS;
        let b = d.get_mut(Tool::Box);
        b.outline = false;
        b.set(Prop::Brightness, 2.0, canvas);
        let s = draw_shape(
            Tool::Box,
            [0.0; 2],
            [50.0, 30.0],
            d.get(Tool::Box),
            [1.0; 3],
        );
        assert_eq!(s.outline(), Some(false), "fill");
        assert_eq!(s.brightness(), 2.0);
        // Sides land whole, and a thickness on a fill is nothing.
        let p = d.get_mut(Tool::Polygon);
        p.set(Prop::Sides, 7.4, canvas);
        assert_eq!(p.sides, 7);
        assert!(p.slider_live(Tool::Polygon, Prop::Thickness));
        p.outline = false;
        assert!(!p.slider_live(Tool::Polygon, Prop::Thickness));
        // The circle's defaults are untouched by the box's.
        assert_eq!(*d.get(Tool::Circle), ToolDefaults::birth(Tool::Circle));
    }

    /// Every slider on a page moves a number that page's shape actually
    /// carries — a circle page with a Sides slider is a dead control.
    #[test]
    fn every_slider_moves_something_its_shape_has() {
        for tool in DRAW_TOOLS {
            let d = ToolDefaults::birth(tool);
            let s = draw_shape(tool, [300.0; 2], [400.0, 360.0], &d, [1.0; 3]);
            for k in sliders(tool) {
                let has = match k.prop {
                    Prop::Sides => s.sides().is_some(),
                    Prop::Thickness => s.thickness().is_some(),
                    Prop::Density => s.density().is_some(),
                    Prop::Twinkle => s.twinkle().is_some(),
                    Prop::TwinkleRate => s.twinkle_rate().is_some(),
                    Prop::Glow | Prop::Brightness | Prop::Opacity => true,
                    other => panic!("{tool:?} page has an unexpected {other:?} slider"),
                };
                assert!(
                    has,
                    "{tool:?} page moves {:?}, which its shape lacks",
                    k.prop
                );
            }
        }
        assert!(sliders(Tool::Select).is_empty(), "Move has no defaults");
    }

    /// The switch flips the choice it names, and the readout prints a
    /// number a person can read back into the slider.
    #[test]
    fn switches_flip_and_readouts_print() {
        let mut d = ToolDefaults::birth(Tool::Circle);
        let sw = Switch::for_tool(Tool::Circle).unwrap();
        assert_eq!(sw.active(&d), 1, "born an outline");
        sw.pick(&mut d, 0);
        assert!(!d.outline);
        let mut s = ToolDefaults::birth(Tool::Stars);
        let sf = Switch::for_tool(Tool::Stars).unwrap();
        assert_eq!(sf.labels().len(), STAR_FORMS.len());
        sf.pick(&mut s, 99);
        assert_eq!(s.form, STAR_FORMS.len() - 1, "clamped to a real form");
        assert_eq!(Switch::for_tool(Tool::Line), None);
        assert_eq!(readout(Prop::Sides, 7.0), "7");
        assert_eq!(readout(Prop::Brightness, 1.0), "1.00");
        assert_eq!(readout(Prop::Thickness, 4.0), "4.0");
    }
}
