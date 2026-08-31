//! The viewpoint: which camera the viewport looks through, how its pixels
//! map into the scene, and the cursor's journey from window px to canvas
//! units — the one conversion every click and drag passes through.
//!
//! Two views. The **comp viewer** looks through the render camera: the
//! canvas, aspect-fit, zoomed and panned by the CanvasView — the video as
//! it will be. The **fly view** (`Tab`, or View > Fly View) looks through
//! an editor-only camera you fly around the scene, game-editor style —
//! Alva's hands know Ember's: drag empty space to look around, WASD to
//! fly (Q/E down and up, Shift to sprint), the wheel to move forward and
//! back, right- or middle-drag to pan — with the floor, the canvas's frame
//! and the render camera's frustum drawn in, so where a thing is, and
//! which way it faces, is something you can see rather than infer. Nothing
//! about the document knows which view is up: the stage renders through
//! whichever camera it is handed.

use std::time::Instant;

use spark_render::{CANVAS, Camera, Framing, Vec3};
use spark_ui::Layout;
use winit::keyboard::{KeyCode, PhysicalKey};

use crate::Studio;
use crate::gizmo::{self, Gizmo};
use crate::overlay::{self, Overlay};

/// Radians of look per pixel of drag.
const LOOK_RATE: f32 = 0.0025;
/// Canvas units per second of held key.
const FLY_SPEED: f32 = 2000.0;
/// How much faster with Shift held.
const SPRINT: f32 = 4.0;
/// Canvas units per wheel notch — a constant step, so the wheel neither
/// crawls up close nor rockets far out.
const WHEEL_STEP: f32 = 500.0;
/// Pitch stops short of straight up or down, where yaw would lose its
/// meaning.
const PITCH_LIMIT: f32 = 1.45;

/// The fly view's camera: an eye somewhere in the scene, looking along a
/// yaw and a pitch. Zero and zero looks the way the render camera does —
/// straight at the canvas from in front of it; yaw turns to the right,
/// pitch looks down (y is down).
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Fly {
    pub eye: Vec3,
    pub yaw: f32,
    pub pitch: f32,
}

impl Fly {
    /// A first look: the centre of `canvas`, from a little above and to
    /// the left, far enough back to see the whole canvas and its floor.
    pub fn new(canvas: [f32; 2]) -> Self {
        let centre = Vec3::new(canvas[0] * 0.5, canvas[1] * 0.5, 0.0);
        Self::looking_at(Camera::orbit(centre, -0.55, 0.32, 3400.0, canvas).eye, centre)
    }

    /// At `eye`, aimed at `at`.
    pub fn looking_at(eye: Vec3, at: Vec3) -> Self {
        let f = (at - eye).normalized();
        Self {
            eye,
            yaw: f.x.atan2(-f.z),
            pitch: f.y.clamp(-1.0, 1.0).asin(),
        }
    }

    /// The direction the eye looks along, unit length.
    pub fn forward(&self) -> Vec3 {
        let (sy, cy) = self.yaw.sin_cos();
        let (sp, cp) = self.pitch.sin_cos();
        Vec3::new(sy * cp, sp, -cy * cp)
    }

    /// The camera, with the render camera's lens and film gate for
    /// `canvas` — the same one the comp is looked at through, moved.
    pub fn camera(&self, canvas: [f32; 2]) -> Camera {
        Camera {
            eye: self.eye,
            target: self.eye + self.forward() * 1000.0,
            ..Camera::stage(canvas)
        }
    }

    /// The view's axes: right, down, forward. The gate doesn't bend
    /// them, so any canvas gives the same answer.
    fn axes(&self) -> (Vec3, Vec3, Vec3) {
        self.camera(CANVAS).basis()
    }

    /// A drag on empty space grabs the world: the scene follows the
    /// cursor, so dragging right turns the view left and dragging down
    /// looks up — inverted on both axes, at Alva's request (2026-08-31),
    /// the way a drag on a canvas always moves what is under it. The eye
    /// stays put.
    pub fn look(&mut self, dx: f32, dy: f32) {
        self.yaw -= dx * LOOK_RATE;
        self.pitch = (self.pitch - dy * LOOK_RATE).clamp(-PITCH_LIMIT, PITCH_LIMIT);
    }

    /// Right- or middle-drag: slide the eye across the view, the scene
    /// following the cursor. Scaled by the eye's height over the canvas
    /// plane, a fair proxy for how far away the things in view are.
    pub fn pan(&mut self, dx: f32, dy: f32) {
        let (right, down, _) = self.axes();
        let k = self.eye.z.abs().max(300.0) * 0.0011;
        self.eye = self.eye - right * (dx * k) - down * (dy * k);
    }

    /// The held keys carry the eye for `dt` seconds: W/S along the look,
    /// A/D across it, Q/E straight down and up the world.
    pub fn fly(&mut self, keys: FlyKeys, dt: f32, sprint: bool) {
        let f = self.forward();
        let (right, _, _) = self.axes();
        let up = Vec3::new(0.0, -1.0, 0.0);
        let mut m = Vec3::ZERO;
        if keys.forward {
            m = m + f;
        }
        if keys.back {
            m = m - f;
        }
        if keys.right {
            m = m + right;
        }
        if keys.left {
            m = m - right;
        }
        if keys.up {
            m = m + up;
        }
        if keys.down {
            m = m - up;
        }
        if m.length() > 0.0 {
            let speed = if sprint { FLY_SPEED * SPRINT } else { FLY_SPEED };
            self.eye = self.eye + m.normalized() * (speed * dt);
        }
    }

    /// The wheel: `notches` up is forward, down is back, a fixed step
    /// each.
    pub fn wheel(&mut self, notches: f32) {
        self.eye = self.eye + self.forward() * (notches * WHEEL_STEP);
    }
}

impl Default for Fly {
    fn default() -> Self {
        Self::new(CANVAS)
    }
}

/// Which of the fly keys are held.
#[derive(Clone, Copy, Default, PartialEq, Eq, Debug)]
pub struct FlyKeys {
    pub forward: bool,
    pub back: bool,
    pub left: bool,
    pub right: bool,
    pub up: bool,
    pub down: bool,
}

impl FlyKeys {
    /// The flag a physical key drives, if it is one of the six. Physical,
    /// so the cluster is under the left hand on any layout.
    fn slot(&mut self, code: KeyCode) -> Option<&mut bool> {
        Some(match code {
            KeyCode::KeyW => &mut self.forward,
            KeyCode::KeyS => &mut self.back,
            KeyCode::KeyA => &mut self.left,
            KeyCode::KeyD => &mut self.right,
            KeyCode::KeyE => &mut self.up,
            KeyCode::KeyQ => &mut self.down,
            _ => return None,
        })
    }

    pub fn is_fly_key(code: KeyCode) -> bool {
        Self::default().slot(code).is_some()
    }

    /// Record a press or release; whether the key was one of ours.
    pub fn set(&mut self, code: KeyCode, down: bool) -> bool {
        match self.slot(code) {
            Some(k) => {
                *k = down;
                true
            }
            None => false,
        }
    }

    pub fn any(&self) -> bool {
        self.forward || self.back || self.left || self.right || self.up || self.down
    }
}

/// A left press on empty space in the fly view. It is nothing until the
/// cursor has travelled `THRESHOLD` px — then it is a look, and stays one
/// until release. A press that never travelled was a click.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Look {
    press: (f32, f32),
    last: (f32, f32),
    active: bool,
}

impl Look {
    pub const THRESHOLD: f32 = 4.0;

    pub fn begin(px: f32, py: f32) -> Self {
        Self {
            press: (px, py),
            last: (px, py),
            active: false,
        }
    }

    /// The cursor moved: the look delta to apply, once the drag is real.
    pub fn moved(&mut self, px: f32, py: f32) -> Option<(f32, f32)> {
        let d = (px - self.last.0, py - self.last.1);
        self.last = (px, py);
        if !self.active {
            let (ox, oy) = (px - self.press.0, py - self.press.1);
            if (ox * ox + oy * oy).sqrt() < Self::THRESHOLD {
                return None;
            }
            self.active = true;
        }
        Some(d)
    }

    pub fn active(&self) -> bool {
        self.active
    }
}

impl Studio {
    /// The camera this frame is drawn through.
    pub(crate) fn camera(&self) -> Camera {
        let canvas = self.editor.canvas();
        match &self.fly {
            Some(f) => f.camera(canvas),
            None => Camera::stage(canvas),
        }
    }

    /// How that camera's picture is placed in the viewport.
    pub(crate) fn framing(&self, layout: &Layout) -> Framing {
        match self.fly {
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
        gizmo::build(
            &self.editor,
            &self.camera(),
            &self.framing(layout),
            res,
            self.scale(),
            self.gizmo_mode,
        )
    }

    /// `Tab` / View > Fly View: fly around the scene, or come back to the
    /// comp viewer. The camera parks where it was left, so coming back
    /// lands on the same view.
    pub(crate) fn toggle_fly(&mut self) -> bool {
        self.fly = match self.fly.take() {
            Some(f) => {
                self.fly_park = f;
                None
            }
            None => Some(self.fly_park),
        };
        self.gizmo_hover = None;
        self.look = None;
        self.canvas_pan = None;
        self.fly_keys = FlyKeys::default();
        self.fly_last = None;
        true
    }

    fn over_viewport(&self) -> bool {
        let (cx, cy) = (self.cursor_px.0 as f32, self.cursor_px.1 as f32);
        self.layout().is_some_and(|l| l.viewport.contains(cx, cy))
    }

    /// A text field owns the keyboard.
    fn typing(&self) -> bool {
        self.field_edit.is_some()
            || self.material_edit.is_some()
            || self.bpm_edit.is_some()
            || self.rename.is_some()
    }

    /// A key went down or up. Whether it was a fly key that the fly view
    /// took: a press counts only with the view up, the cursor over the
    /// viewport and nobody typing — in the comp viewer W is still the
    /// brightness nudge — while a release always clears its flag, so a
    /// key can't stick down.
    pub(crate) fn fly_key(&mut self, key: &PhysicalKey, down: bool) -> bool {
        let PhysicalKey::Code(code) = key else {
            return false;
        };
        if !FlyKeys::is_fly_key(*code) || self.export.is_some() {
            return false;
        }
        if !down {
            self.fly_keys.set(*code, false);
            if !self.fly_keys.any() {
                self.fly_last = None;
            }
            return true;
        }
        if self.fly.is_none() || !self.over_viewport() || self.typing() {
            return false;
        }
        self.fly_keys.set(*code, true);
        if self.fly_last.is_none() {
            self.fly_last = Some(Instant::now());
        }
        self.request_redraw();
        true
    }

    /// Focus left the window: no key is held any more, whatever the OS
    /// forgets to tell us.
    pub(crate) fn drop_fly_keys(&mut self) {
        self.fly_keys = FlyKeys::default();
        self.fly_last = None;
    }

    /// Keys are carrying the camera: the frame loop has to keep running.
    pub(crate) fn flying(&self) -> bool {
        self.fly.is_some() && self.fly_keys.any()
    }

    /// Once a frame, before the camera is read: the held keys move the
    /// eye by the time since they last did. A stall (a file dialog, a
    /// long frame) is capped so the eye doesn't leap when it ends.
    pub(crate) fn fly_tick(&mut self) {
        if !self.flying() {
            self.fly_last = None;
            return;
        }
        let now = Instant::now();
        let Some(last) = self.fly_last.replace(now) else {
            return;
        };
        if !self.over_viewport() || self.typing() {
            return;
        }
        let dt = now.duration_since(last).as_secs_f32().min(0.1);
        let sprint = self.modifiers.shift_key();
        if let Some(f) = &mut self.fly {
            f.fly(self.fly_keys, dt, sprint);
        }
    }

    /// The right button went down: in the fly view, over the viewport,
    /// that starts a pan.
    pub(crate) fn pan_press(&mut self) -> bool {
        if self.fly.is_some() && self.over_viewport() {
            self.canvas_pan = Some(self.cursor_px);
            return true;
        }
        false
    }

    /// A right- or middle-drag by (`dx`, `dy`) px in the fly view.
    pub(crate) fn pan_drag(&mut self, dx: f32, dy: f32) -> bool {
        match &mut self.fly {
            Some(f) => {
                f.pan(dx, dy);
                true
            }
            None => false,
        }
    }

    /// A left press on empty space in the fly view.
    pub(crate) fn look_press(&mut self) {
        let (cx, cy) = (self.cursor_px.0 as f32, self.cursor_px.1 as f32);
        self.look = Some(Look::begin(cx, cy));
    }

    /// The cursor moved during one; whether the view turned.
    pub(crate) fn look_moved(&mut self, px: f64, py: f64) -> bool {
        let Some(l) = &mut self.look else {
            return false;
        };
        let Some((dx, dy)) = l.moved(px as f32, py as f32) else {
            return false;
        };
        match &mut self.fly {
            Some(f) => {
                f.look(dx, dy);
                true
            }
            None => false,
        }
    }

    /// The button came up: a press that never became a look was a click
    /// on empty space, which drops the selection (unless Ctrl held it).
    /// Whether the release was ours.
    pub(crate) fn look_release(&mut self) -> bool {
        let Some(l) = self.look.take() else {
            return false;
        };
        if !l.active() && !self.modifiers.control_key() {
            self.editor.deselect();
        }
        true
    }

    /// The wheel over the viewport in the fly view: forward and back.
    pub(crate) fn fly_wheel(&mut self, notches: f32) -> bool {
        match &mut self.fly {
            Some(f) => {
                f.wheel(notches);
                true
            }
            None => false,
        }
    }

    /// What the view itself draws in the scene: the floor when asked for
    /// (and always while flying), and in the fly view the canvas's frame
    /// and the render camera.
    pub(crate) fn view_overlays(&self, camera: &Camera) -> Vec<Overlay> {
        let canvas = self.editor.canvas();
        let mut out = Vec::new();
        if self.floor || self.fly.is_some() {
            out.extend(overlay::floor_grid(canvas));
        }
        if self.fly.is_some() {
            out.extend(overlay::canvas_frame(canvas));
            out.extend(overlay::frustum(&Camera::stage(canvas), camera));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn near(a: Vec3, b: Vec3) -> bool {
        (a - b).length() < 1e-3
    }

    #[test]
    fn zero_looks_at_the_canvas_the_way_the_stage_camera_does() {
        let f = Fly {
            eye: Vec3::new(0.0, 0.0, 1000.0),
            yaw: 0.0,
            pitch: 0.0,
        };
        assert!(near(f.forward(), Vec3::new(0.0, 0.0, -1.0)));
        // Screen right is +x, screen down is +y: the canvas's own axes.
        let (right, down, _) = f.camera(CANVAS).basis();
        assert!(near(right, Vec3::new(1.0, 0.0, 0.0)));
        assert!(near(down, Vec3::new(0.0, 1.0, 0.0)));
    }

    #[test]
    fn the_first_look_is_aimed_at_the_canvas_centre() {
        let f = Fly::new(CANVAS);
        let centre = Vec3::new(CANVAS[0] * 0.5, CANVAS[1] * 0.5, 0.0);
        let to = (centre - f.eye).normalized();
        assert!(near(f.forward(), to));
        // From in front of the canvas, above and to the left of it.
        assert!(f.eye.z > 0.0 && f.eye.x < centre.x && f.eye.y < centre.y);
        // And `looking_at` round-trips through the angles.
        let g = Fly::looking_at(f.eye, f.eye + f.forward() * 500.0);
        assert!((g.yaw - f.yaw).abs() < 1e-4 && (g.pitch - f.pitch).abs() < 1e-4);
    }

    #[test]
    fn a_drag_grabs_the_world_so_the_view_turns_against_it() {
        let mut f = Fly {
            eye: Vec3::ZERO,
            yaw: 0.0,
            pitch: 0.0,
        };
        f.look(100.0, 0.0);
        assert!(f.forward().x < 0.0, "drag right: the scene slides right, the view turns left");
        assert_eq!(f.forward().y, 0.0);
        f.look(0.0, 100.0);
        assert!(f.forward().y < 0.0, "drag down: the scene slides down, the view looks up");
        // The eye never moves while looking.
        assert_eq!(f.eye, Vec3::ZERO);
        // Pitch stops short of the pole.
        f.look(0.0, -1e6);
        assert_eq!(f.pitch, PITCH_LIMIT);
        f.look(0.0, 1e7);
        assert_eq!(f.pitch, -PITCH_LIMIT);
    }

    #[test]
    fn keys_carry_the_eye_along_the_view_and_shift_sprints() {
        let mut f = Fly::looking_at(Vec3::new(50.0, 60.0, 900.0), Vec3::new(50.0, 60.0, 0.0));
        let start = f.eye;
        let mut k = FlyKeys::default();
        k.forward = true;
        f.fly(k, 0.5, false);
        assert!(near(f.eye, start + Vec3::new(0.0, 0.0, -FLY_SPEED * 0.5)));
        f.eye = start;
        f.fly(k, 0.5, true);
        assert!(near(f.eye, start + Vec3::new(0.0, 0.0, -FLY_SPEED * SPRINT * 0.5)));
        // D goes screen-right, E goes up the world (−y), and two keys
        // together don't go faster than one.
        f.eye = start;
        let mut k = FlyKeys::default();
        k.right = true;
        k.up = true;
        f.fly(k, 1.0, false);
        let d = f.eye - start;
        assert!(d.x > 0.0 && d.y < 0.0 && d.z.abs() < 1e-3);
        assert!((d.length() - FLY_SPEED).abs() < 1e-2);
        // Opposed keys cancel to nothing.
        f.eye = start;
        let mut k = FlyKeys::default();
        k.forward = true;
        k.back = true;
        f.fly(k, 1.0, false);
        assert_eq!(f.eye, start);
    }

    #[test]
    fn the_wheel_steps_forward_by_a_fixed_amount() {
        let mut f = Fly::looking_at(Vec3::new(0.0, 0.0, 5000.0), Vec3::ZERO);
        f.wheel(1.0);
        assert!(near(f.eye, Vec3::new(0.0, 0.0, 5000.0 - WHEEL_STEP)));
        f.wheel(-2.0);
        assert!(near(f.eye, Vec3::new(0.0, 0.0, 5000.0 + WHEEL_STEP)));
    }

    #[test]
    fn a_pan_slides_the_eye_against_the_drag_and_keeps_its_aim() {
        let mut f = Fly::looking_at(Vec3::new(0.0, 0.0, 1000.0), Vec3::ZERO);
        let fwd = f.forward();
        f.pan(100.0, 0.0);
        // Dragging right, the scene follows the cursor: the eye goes left.
        assert!(f.eye.x < 0.0 && f.eye.y == 0.0);
        f.pan(0.0, 100.0);
        assert!(f.eye.y < 0.0);
        assert!(near(f.forward(), fwd));
    }

    #[test]
    fn fly_keys_know_their_six_and_nothing_else() {
        let mut k = FlyKeys::default();
        assert!(!k.any());
        assert!(k.set(KeyCode::KeyW, true));
        assert!(k.forward && k.any());
        assert!(!k.set(KeyCode::KeyX, true));
        assert!(!FlyKeys::is_fly_key(KeyCode::Space));
        assert!(FlyKeys::is_fly_key(KeyCode::KeyQ));
        assert!(k.set(KeyCode::KeyW, false));
        assert!(!k.any());
    }

    #[test]
    fn a_look_is_a_click_until_the_cursor_has_travelled() {
        let mut l = Look::begin(100.0, 100.0);
        assert_eq!(l.moved(101.0, 101.0), None);
        assert_eq!(l.moved(102.0, 100.0), None);
        assert!(!l.active());
        // Past the threshold: a look, reporting the step since the last
        // move — and every move after that, however small.
        assert_eq!(l.moved(106.0, 100.0), Some((4.0, 0.0)));
        assert!(l.active());
        assert_eq!(l.moved(106.0, 101.0), Some((0.0, 1.0)));
        assert_eq!(l.moved(100.0, 100.0), Some((-6.0, -1.0)));
        assert!(l.active(), "a look that returns to its press is still a look");
    }
}
