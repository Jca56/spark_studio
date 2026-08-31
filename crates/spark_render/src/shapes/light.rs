//! Lights on the shape side.
//!
//! A light is an object in the outliner like any shape: a centre, a
//! place in space, a colour, a brightness, keyframes, a card. What it
//! *does* is light the meshes, and `as_light` is the whole of that: the
//! plain numbers the mesh pass wants, read off the shape. `b` holds the
//! range (so `size()`, scaling and the bass React all reach it), `extra`
//! the kind, the cone and its softness. The light shines along its
//! plane's normal into the scene — turn and tilt aim it, the same two
//! numbers that aim everything else.

use crate::light::{Light, LightKind};
use crate::math::{Mat4, Vec3};

use super::{KIND_LIGHT, Shape};

/// A fresh light's reach, canvas units.
const RANGE: f32 = 700.0;
/// A fresh spot's cone half-angle, degrees.
const CONE: f32 = 30.0;
const SOFT: f32 = 0.5;
/// How far from a light's position a click still picks it, and the size
/// of its gizmo.
pub const LIGHT_PICK: f32 = 26.0;

impl Shape {
    /// A light of `kind` at `center`, white, shining straight into the
    /// scene.
    pub fn light(center: [f32; 2], kind: LightKind) -> Self {
        let mut s = Self::base(KIND_LIGHT, center, [RANGE, RANGE]);
        s.extra = [kind.index() as f32, CONE, SOFT, 0.0];
        s
    }

    /// The default sun as an object: aimed exactly where the sun every
    /// comp is lit by until it has one of its own is aimed.
    pub fn sun(center: [f32; 2]) -> Self {
        let mut s = Self::light(center, LightKind::Sun);
        let d = Light::default_sun().direction;
        // The inverse of `light_direction`: tilt sets y, turn the rest.
        s.set_tilt(d.y.asin());
        s.set_turn((-d.x).atan2(-d.z));
        s
    }

    pub fn is_light(&self) -> bool {
        self.kind_rot[0] == KIND_LIGHT
    }

    pub fn light_kind(&self) -> Option<LightKind> {
        self.is_light()
            .then(|| LightKind::from_index(self.extra[0] as usize))
    }

    pub fn set_light_kind(&mut self, kind: LightKind) {
        if self.is_light() {
            self.extra[0] = kind.index() as f32;
        }
    }

    /// A spot's cone half-angle in degrees; `None` off a spot.
    pub fn cone(&self) -> Option<f32> {
        (self.light_kind() == Some(LightKind::Spot)).then_some(self.extra[1])
    }

    pub fn set_cone(&mut self, degrees: f32) {
        if self.is_light() {
            self.extra[1] = degrees.clamp(2.0, 120.0);
        }
    }

    /// The way the light travels: into the scene along its plane's
    /// normal, turned and tilted with the plane.
    pub fn light_direction(&self) -> Vec3 {
        (Mat4::rotation_y(self.turn()) * Mat4::rotation_x(self.tilt()))
            .transform_vec(Vec3::new(0.0, 0.0, -1.0))
            .normalized()
    }

    pub fn light_position(&self) -> Vec3 {
        let c = self.center();
        Vec3::new(c[0], c[1], self.z())
    }

    /// What the mesh pass shades with, or `None` off a light. Colour
    /// carries brightness, so the slider and the audio React are one
    /// multiply.
    pub fn as_light(&self) -> Option<Light> {
        let kind = self.light_kind()?;
        let rgb = self.rgb();
        let e = self.brightness();
        Some(Light {
            kind,
            position: self.light_position(),
            direction: self.light_direction(),
            color: [rgb[0] * e, rgb[1] * e, rgb[2] * e],
            range: self.size(),
            cone: self.extra[1].to_radians(),
            soft: self.extra[2],
        })
    }
}

#[cfg(test)]
mod tests {
    use super::super::ShapeKind;
    use super::*;
    use std::f32::consts::FRAC_PI_2;

    fn close(a: Vec3, b: Vec3) -> bool {
        (a - b).length() < 1e-4
    }

    #[test]
    fn a_light_is_a_kind_a_reach_and_an_aim() {
        let mut l = Shape::light([100.0, 200.0], LightKind::Spot);
        assert!(l.is_light());
        assert_eq!(l.kind(), ShapeKind::Light);
        assert_eq!(l.light_kind(), Some(LightKind::Spot));
        assert_eq!(l.cone(), Some(CONE));
        assert_eq!(l.size(), RANGE);
        assert_eq!((l.outline(), l.box_size(), l.thickness()), (None, None, None));
        // Straight into the scene until aimed.
        assert!(close(l.light_direction(), Vec3::new(0.0, 0.0, -1.0)));
        // A quarter tilt points it down the canvas; a quarter turn, left.
        l.set_tilt(FRAC_PI_2);
        assert!(close(l.light_direction(), Vec3::new(0.0, 1.0, 0.0)), "{:?}", l.light_direction());
        l.set_tilt(0.0);
        l.set_turn(FRAC_PI_2);
        assert!(close(l.light_direction(), Vec3::new(-1.0, 0.0, 0.0)), "{:?}", l.light_direction());
        // A point has no cone to speak of.
        l.set_light_kind(LightKind::Point);
        assert_eq!(l.cone(), None);
        // Off a light, nothing.
        let c = Shape::circle([0.0; 2], 5.0);
        assert!(c.light_kind().is_none() && c.as_light().is_none() && c.cone().is_none());
    }

    #[test]
    fn as_light_carries_colour_times_brightness_and_the_range() {
        let mut l = Shape::light([100.0, 200.0], LightKind::Point).color(1.0, 0.5, 0.0);
        l.set_brightness(2.0);
        l.set_z(300.0);
        l.scale_by(0.5);
        let light = l.as_light().unwrap();
        assert_eq!(light.kind, LightKind::Point);
        assert_eq!(light.color, [2.0, 1.0, 0.0]);
        assert_eq!(light.range, RANGE * 0.5);
        assert!(close(light.position, Vec3::new(100.0, 200.0, 300.0)));
    }

    #[test]
    fn the_sun_object_points_where_the_default_sun_does() {
        let s = Shape::sun([200.0, 150.0]);
        assert_eq!(s.light_kind(), Some(LightKind::Sun));
        let want = Light::default_sun().direction;
        assert!(close(s.as_light().unwrap().direction, want), "{:?} vs {want:?}", s.light_direction());
    }

    #[test]
    fn a_light_picks_around_its_position() {
        let l = Shape::light([100.0, 200.0], LightKind::Sun);
        assert!(l.distance([100.0, 200.0]) < 0.0);
        assert!(l.distance([100.0 + LIGHT_PICK - 1.0, 200.0]) < 0.0);
        assert!(l.distance([100.0 + LIGHT_PICK + 5.0, 200.0]) > 0.0);
        assert_eq!(l.selection_halo().kind(), ShapeKind::Circle);
    }

    #[test]
    fn a_light_rides_the_serialized_line() {
        let mut l = Shape::sun([1.0, 2.0]);
        l.set_cone(45.0);
        let back = Shape::from_array(l.to_array());
        assert_eq!(back, l);
        assert_eq!(back.light_kind(), Some(LightKind::Sun));
    }
}
