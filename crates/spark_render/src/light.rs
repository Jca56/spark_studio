//! Lights: what the mesh pass shades with.
//!
//! A light is an object in the scene like anything else — the studio
//! keeps it as a shape with a card and hands the pass the plain numbers
//! here. Three kinds: a **sun** is a direction and nothing else; a
//! **point** sits somewhere and fades to nothing at its range; a **spot**
//! is a point with a cone. Colour carries intensity (it is the light's
//! linear rgb times how hard it burns), so a light's brightness slider and
//! its audio reaction are one multiply, the same as a shape's.
//!
//! A comp with no lights of its own gets [`Light::default_sun`], which is
//! the sun every mesh was lit by before lights existed: from the upper
//! left, in front of the canvas.

use crate::math::Vec3;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum LightKind {
    Sun,
    Point,
    Spot,
    /// Light from everywhere at once — the scene's ambient level and
    /// colour, and the strength of the Fresnel rim — as an object with
    /// a card, so it can be keyed and made to breathe with the track
    /// (2026-08-31). A comp without one gets the defaults.
    Ambient,
}

/// Kind names, in `LightKind::index` order — the card's picker.
pub const LIGHT_KINDS: [&str; 4] = ["Sun", "Point", "Spot", "Ambient"];

impl LightKind {
    pub fn index(self) -> usize {
        match self {
            LightKind::Sun => 0,
            LightKind::Point => 1,
            LightKind::Spot => 2,
            LightKind::Ambient => 3,
        }
    }

    pub fn from_index(i: usize) -> Self {
        match i {
            1 => LightKind::Point,
            2 => LightKind::Spot,
            3 => LightKind::Ambient,
            _ => LightKind::Sun,
        }
    }

    /// Whether the light comes from somewhere — a sun, point or spot —
    /// as opposed to everywhere.
    pub fn is_directional(self) -> bool {
        self != LightKind::Ambient
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Light {
    pub kind: LightKind,
    pub position: Vec3,
    /// Unit length: the way the light travels (sun and spot).
    pub direction: Vec3,
    /// Linear rgb × intensity.
    pub color: [f32; 3],
    /// Point and spot: the distance at which the light shines at its
    /// nominal intensity — inverse square from there, so twice as far is
    /// a quarter as bright and half as far is four times. It never fades
    /// to nothing.
    pub range: f32,
    /// Spot: the cone's half-angle at its edge, radians.
    pub cone: f32,
    /// Spot: how much of the cone is the fade at its edge, 0 (hard) to 1
    /// (fading from the axis out).
    pub soft: f32,
    /// Ambient: the Fresnel rim's strength, 0 to 1.
    pub rim: f32,
}

/// The most lights a scene hands the shader at once.
pub const MAX_LIGHTS: usize = 8;

/// The default sun's direction of travel: right, down and away.
const SUN_DIR: [f32; 3] = [0.3, 0.5, -0.8];

impl Light {
    /// The ambient level a comp has until it has an ambient light of its
    /// own, and the rim strength.
    pub const DEFAULT_AMBIENT: f32 = 0.22;
    pub const DEFAULT_RIM: f32 = 0.35;

    /// The sun a comp is lit by until it has a light of its own.
    pub fn default_sun() -> Self {
        Self {
            kind: LightKind::Sun,
            position: Vec3::ZERO,
            direction: Vec3::new(SUN_DIR[0], SUN_DIR[1], SUN_DIR[2]).normalized(),
            color: [1.0; 3],
            range: 0.0,
            cone: 0.0,
            soft: 0.0,
            rim: 0.0,
        }
    }

    /// The shader's layout: four `vec4`s. `shadow` is the light's shadow
    /// map slot, or -1 for none.
    pub(crate) fn gpu(&self, shadow: i32) -> LightData {
        let d = self.direction.normalized();
        let cos_outer = self.cone.cos();
        let cos_inner = (self.cone * (1.0 - self.soft.clamp(0.0, 1.0))).cos();
        LightData {
            pos_kind: [
                self.position.x,
                self.position.y,
                self.position.z,
                self.kind.index() as f32,
            ],
            dir_range: [d.x, d.y, d.z, self.range.max(0.0)],
            color_cos: [self.color[0], self.color[1], self.color[2], cos_outer],
            // A hard cone still needs its inner edge a hair inside the
            // outer, or the smoothstep between them is a step at nothing.
            params: [cos_inner.max(cos_outer + 1e-4), self.rim, shadow as f32, 0.0],
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct LightData {
    pos_kind: [f32; 4],
    dir_range: [f32; 4],
    color_cos: [f32; 4],
    params: [f32; 4],
}

/// The whole lights uniform: a count, then the slots.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct LightsUniform {
    count: [f32; 4],
    lights: [LightData; MAX_LIGHTS],
}

/// The lights a scene is actually lit by: `lights`, with the default sun
/// added when none of them comes from anywhere — an ambient light alone
/// sets the level, it doesn't put the sun out — capped at the slots.
pub(crate) fn resolve(lights: &[Light]) -> Vec<Light> {
    let mut src: Vec<Light> = lights.to_vec();
    if !src.iter().any(|l| l.kind.is_directional()) {
        src.insert(0, Light::default_sun());
    }
    src.truncate(MAX_LIGHTS);
    src
}

impl LightsUniform {
    /// Pack `lights`, resolved, none of them casting a shadow.
    #[cfg(test)]
    pub(crate) fn pack(lights: &[Light]) -> Self {
        Self::pack_resolved(&resolve(lights), &[])
    }

    /// Pack an already resolved list; `shadow[i]` is light `i`'s shadow
    /// map slot, or -1 (and missing is -1).
    pub(crate) fn pack_resolved(resolved: &[Light], shadow: &[i32]) -> Self {
        let n = resolved.len().min(MAX_LIGHTS);
        let mut out = Self {
            count: [n as f32, 0.0, 0.0, 0.0],
            lights: [Light::default_sun().gpu(-1); MAX_LIGHTS],
        };
        for (i, (slot, l)) in out.lights.iter_mut().zip(resolved).enumerate() {
            *slot = l.gpu(shadow.get(i).copied().unwrap_or(-1));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_lights_packs_the_default_sun() {
        let u = LightsUniform::pack(&[]);
        assert_eq!(u.count[0], 1.0);
        assert_eq!(u.lights[0].pos_kind[3], 0.0);
        let d = u.lights[0].dir_range;
        assert!((d[0] * d[0] + d[1] * d[1] + d[2] * d[2] - 1.0).abs() < 1e-5);
        assert!(d[2] < 0.0, "the default sun travels away from the camera");
    }

    #[test]
    fn a_spot_packs_its_cone_as_cosines() {
        let l = Light {
            kind: LightKind::Spot,
            position: Vec3::new(1.0, 2.0, 3.0),
            direction: Vec3::new(0.0, 0.0, -2.0),
            color: [1.0, 0.5, 0.0],
            range: 400.0,
            cone: 30f32.to_radians(),
            soft: 0.5,
            rim: 0.0,
        };
        let g = l.gpu(1);
        assert_eq!(g.pos_kind, [1.0, 2.0, 3.0, 2.0]);
        assert_eq!(g.params[2], 1.0, "its shadow slot");
        assert_eq!(g.dir_range, [0.0, 0.0, -1.0, 400.0]);
        assert!((g.color_cos[3] - 30f32.to_radians().cos()).abs() < 1e-6);
        assert!((g.params[0] - 15f32.to_radians().cos()).abs() < 1e-6);
        // A hard cone keeps a sliver of fade so the edge is a step, not
        // a division by nothing.
        let hard = Light { soft: 0.0, ..l }.gpu(-1);
        assert_eq!(hard.params[2], -1.0);
        assert!(hard.params[0] > hard.color_cos[3]);
    }

    #[test]
    fn an_ambient_alone_keeps_the_default_sun() {
        let amb = Light {
            kind: LightKind::Ambient,
            color: [0.3; 3],
            rim: 0.6,
            ..Light::default_sun()
        };
        let u = LightsUniform::pack(&[amb]);
        // The sun first, then the ambient with its rim in the params.
        assert_eq!(u.count[0], 2.0);
        assert_eq!(u.lights[0].pos_kind[3], 0.0);
        assert_eq!(u.lights[1].pos_kind[3], 3.0);
        assert_eq!(u.lights[1].params[1], 0.6);
        // A real sun beside it: no default added.
        let u = LightsUniform::pack(&[amb, Light::default_sun()]);
        assert_eq!(u.count[0], 2.0);
        assert_eq!(u.lights[0].pos_kind[3], 3.0);
        assert_eq!(LightKind::from_index(3), LightKind::Ambient);
        assert!(!LightKind::Ambient.is_directional() && LightKind::Spot.is_directional());
    }

    #[test]
    fn more_than_the_slots_is_capped() {
        let many = vec![Light::default_sun(); MAX_LIGHTS + 3];
        assert_eq!(LightsUniform::pack(&many).count[0], MAX_LIGHTS as f32);
        assert_eq!(LightKind::from_index(7), LightKind::Sun);
        assert_eq!(LightKind::from_index(2).index(), 2);
    }
}
