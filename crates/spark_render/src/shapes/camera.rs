//! The camera on the shape side.
//!
//! A camera is an object in the outliner like any shape: a card, a clip,
//! keyframes, React. What it *does* — for now — is shake: while its clip
//! plays, the render camera is jolted by its Amount at its Speed (see
//! `spark_studio::shake`). It has no place on the canvas yet: the render
//! camera stays where it has always been, and the frustum the fly view
//! draws is that one. Moving it is the roadmap's next step, and this is
//! the object it will hang off. `extra` holds `[amount, speed, 0, 0]`.

use super::{KIND_CAMERA, Shape};

/// A fresh camera's shake: how far the picture jolts, canvas units, and
/// how many times a second it changes its mind — a rumble, not a sway.
const SHAKE: f32 = 16.0;
const RATE: f32 = 12.0;

impl Shape {
    /// A camera object, marked at `center` for its card, shaking at the
    /// default amount and speed the moment its clip plays.
    pub fn camera(center: [f32; 2]) -> Self {
        let mut s = Self::base(KIND_CAMERA, center, [1.0, 1.0]);
        s.extra = [SHAKE, RATE, 0.0, 0.0];
        s
    }

    pub fn is_camera(&self) -> bool {
        self.kind_rot[0] == KIND_CAMERA
    }

    /// How far the picture jolts, canvas units; `None` off a camera.
    pub fn shake(&self) -> Option<f32> {
        self.is_camera().then_some(self.extra[0])
    }

    pub fn set_shake(&mut self, amount: f32) {
        if self.is_camera() {
            self.extra[0] = amount.max(0.0);
        }
    }

    /// Shakes a second; `None` off a camera.
    pub fn shake_rate(&self) -> Option<f32> {
        self.is_camera().then_some(self.extra[1])
    }

    pub fn set_shake_rate(&mut self, per_second: f32) {
        if self.is_camera() {
            self.extra[1] = per_second.max(0.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::ShapeKind;
    use super::*;

    #[test]
    fn a_camera_is_an_amount_and_a_speed() {
        let mut c = Shape::camera([960.0, 540.0]);
        assert!(c.is_camera());
        assert_eq!(c.kind(), ShapeKind::Camera);
        assert_eq!((c.shake(), c.shake_rate()), (Some(SHAKE), Some(RATE)));
        assert_eq!((c.outline(), c.box_size(), c.thickness()), (None, None, None));
        c.set_shake(40.0);
        c.set_shake_rate(-3.0);
        assert_eq!((c.shake(), c.shake_rate()), (Some(40.0), Some(0.0)));
        // Off a camera, nothing — and a setter is a no-op.
        let mut k = Shape::circle([0.0; 2], 5.0);
        k.set_shake(40.0);
        assert!(k.shake().is_none() && k.shake_rate().is_none());
        assert_eq!(k, Shape::circle([0.0; 2], 5.0));
    }

    /// No place on the canvas yet: a click never lands on it.
    #[test]
    fn a_camera_is_never_picked_on_the_canvas() {
        let c = Shape::camera([960.0, 540.0]);
        assert!(c.distance([960.0, 540.0]) > 1e6);
    }

    #[test]
    fn a_camera_rides_the_serialized_line() {
        let mut c = Shape::camera([1.0, 2.0]);
        c.set_shake(33.0);
        c.set_shake_rate(4.5);
        let back = Shape::from_array(c.to_array());
        assert_eq!(back, c);
        assert_eq!((back.shake(), back.shake_rate()), (Some(33.0), Some(4.5)));
    }
}
