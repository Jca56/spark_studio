//! The viewpoint: which camera the viewport looks through, how its pixels
//! map into the scene, and the cursor's journey from window px to canvas
//! units — the one conversion every click and drag passes through.
//!
//! Two views. The **comp viewer** looks through the render camera: the
//! canvas, aspect-fit, zoomed and panned by the CanvasView — the video as
//! it will be. The **orbit view** (`Tab`, or View > Orbit View) looks
//! through an editor-only camera you fly around the scene — middle-drag
//! orbits, Shift+middle pans, Ctrl+wheel dollies — with the floor, the
//! canvas's frame and the render camera's frustum drawn in, so where a
//! thing is, and which way it faces, is something you can see rather than
//! infer. Nothing about the document knows which view is up: the stage
//! renders through whichever camera it is handed.

use spark_render::{CANVAS_H, CANVAS_W, Camera, Framing, Vec3};
use spark_ui::Layout;

use crate::Studio;
use crate::gizmo::{self, Gizmo};
use crate::overlay::{self, Overlay};

/// The orbit view's camera: swung about a target at a distance.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Orbit {
    pub target: Vec3,
    pub yaw: f32,
    pub pitch: f32,
    pub dist: f32,
}

impl Orbit {
    /// A first look: the canvas centre, from a little above and to the
    /// left, far enough back to see the whole canvas and its floor.
    pub fn new() -> Self {
        Self {
            target: Vec3::new(CANVAS_W * 0.5, CANVAS_H * 0.5, 0.0),
            yaw: -0.55,
            pitch: 0.32,
            dist: 3400.0,
        }
    }

    pub fn camera(&self) -> Camera {
        Camera::orbit(self.target, self.yaw, self.pitch, self.dist)
    }

    /// Middle-drag: grab the scene and turn it — the eye swings the other
    /// way from the cursor. Pitch stops short of straight up or down.
    pub fn turn(&mut self, dx: f32, dy: f32) {
        self.yaw -= dx * 0.006;
        self.pitch = (self.pitch + dy * 0.006).clamp(-1.45, 1.45);
    }

    /// Shift+middle-drag: slide the target across the view, the scene
    /// following the cursor.
    pub fn pan(&mut self, dx: f32, dy: f32) {
        let (right, down, _) = self.camera().basis();
        let k = self.dist * 0.0011;
        self.target = self.target - right * (dx * k) - down * (dy * k);
    }

    /// Ctrl+wheel: closer or farther.
    pub fn dolly(&mut self, factor: f32) {
        self.dist = (self.dist * factor).clamp(150.0, 40_000.0);
    }
}

impl Default for Orbit {
    fn default() -> Self {
        Self::new()
    }
}

impl Studio {
    /// The camera this frame is drawn through.
    pub(crate) fn camera(&self) -> Camera {
        match &self.orbit {
            Some(o) => o.camera(),
            None => Camera::stage(),
        }
    }

    /// How that camera's picture is placed in the viewport.
    pub(crate) fn framing(&self, layout: &Layout) -> Framing {
        match self.orbit {
            Some(_) => Framing::Free(layout.viewport),
            None => Framing::Canvas {
                cview: self.canvas_map(layout),
                clip: layout.viewport,
            },
        }
    }

    /// The cursor in canvas units: where its ray meets the canvas plane,
    /// whatever the camera. `None` when it can't — a free view looking
    /// past the canvas's edge.
    pub(crate) fn cursor_canvas(&self, px: f64, py: f64, layout: &Layout) -> Option<[f32; 2]> {
        let res = self.gpu.as_ref()?.size();
        self.camera()
            .canvas_hit(&self.framing(layout), res, [px as f32, py as f32])
    }

    /// The transform gizmo on the primary selection, as seen this frame.
    pub(crate) fn gizmo(&self, layout: &Layout) -> Option<Gizmo> {
        let res = self.gpu.as_ref()?.size();
        gizmo::build(&self.editor, &self.camera(), &self.framing(layout), res)
    }

    /// `Tab` / View > Orbit View: fly around the scene, or come back to
    /// the comp viewer.
    pub(crate) fn toggle_orbit(&mut self) -> bool {
        self.orbit = match self.orbit {
            Some(_) => None,
            None => Some(Orbit::new()),
        };
        self.gizmo_hover = None;
        true
    }

    /// A middle-drag by (`dx`, `dy`) px in the orbit view.
    pub(crate) fn orbit_drag(&mut self, dx: f32, dy: f32, pan: bool) -> bool {
        let Some(o) = &mut self.orbit else {
            return false;
        };
        if pan {
            o.pan(dx, dy);
        } else {
            o.turn(dx, dy);
        }
        true
    }

    pub(crate) fn dolly(&mut self, factor: f32) -> bool {
        match &mut self.orbit {
            Some(o) => {
                o.dolly(factor);
                true
            }
            None => false,
        }
    }

    /// What the view itself draws in the scene: the floor when asked for
    /// (and always while orbiting), and in the orbit view the canvas's
    /// frame and the render camera.
    pub(crate) fn view_overlays(&self, camera: &Camera) -> Vec<Overlay> {
        let mut out = Vec::new();
        if self.floor || self.orbit.is_some() {
            out.extend(overlay::floor_grid());
        }
        if self.orbit.is_some() {
            out.extend(overlay::canvas_frame());
            out.extend(overlay::frustum(&Camera::stage(), camera));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_orbit_camera_keeps_its_distance_and_turns_with_the_drag() {
        let mut o = Orbit::new();
        let d0 = (o.camera().eye - o.target).length();
        assert!((d0 - o.dist).abs() < 1e-2);
        let before = o.camera().eye;
        o.turn(100.0, 0.0);
        let after = o.camera().eye;
        assert!(after != before);
        assert!(((after - o.target).length() - o.dist).abs() < 1e-2);
        // Pitch is clamped short of the pole.
        o.turn(0.0, 100_000.0);
        assert!(o.pitch <= 1.45);
        // Dolly shrinks the distance, within reason.
        o.dolly(0.5);
        assert!((o.dist - 1700.0).abs() < 1e-2);
        o.dolly(1e-9);
        assert_eq!(o.dist, 150.0);
        // A pan moves the target, not the distance.
        let t = o.target;
        o.pan(50.0, 0.0);
        assert!(o.target != t);
        assert!(((o.camera().eye - o.target).length() - o.dist).abs() < 1e-2);
    }
}
