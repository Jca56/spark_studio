//! Mesh-pass tests, by pixels: a quad drawn through the stage has to be
//! lit as the sun says, hide what is behind it and nothing in front of
//! it, come out with soft edges, wear its texture, and stop a halo behind
//! it from glowing through.

use super::super::harness::{DIM, FORMAT, VIEW, clear_black, device, exclusive, framing, readback, target};
use super::super::{Quality, Scene, ShapePass, Stage};
use super::*;
use crate::camera::Camera;
use crate::light::{Light, LightKind};
use crate::math::{Mat4, Vec3};
use crate::shapes::{CANVAS, CANVAS_H, CANVAS_W, Shape};

/// The canvas centre — the vanishing point — in the middle of the target.
const CENTRED: (f32, f32, f32) = (
    VIEW,
    -CANVAS_W * VIEW * 0.5 + 32.0,
    -CANVAS_H * VIEW * 0.5 + 32.0,
);

/// A 200×200 quad on the canvas plane about the origin, facing the camera,
/// UVs running left→right and top→bottom.
pub(super) fn quad() -> MeshData {
    MeshData {
        positions: vec![
            [-100.0, -100.0, 0.0],
            [100.0, -100.0, 0.0],
            [100.0, 100.0, 0.0],
            [-100.0, 100.0, 0.0],
        ],
        normals: vec![[0.0, 0.0, 1.0]; 4],
        uvs: vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]],
        indices: vec![0, 1, 2, 0, 2, 3],
    }
}

/// The quad at the canvas centre, shifted by `d`.
pub(super) fn at(d: Vec3) -> Mat4 {
    Mat4::translation(Vec3::new(CANVAS_W * 0.5, CANVAS_H * 0.5, 0.0) + d)
}

pub(super) type Placement = (Mat4, [f32; 4], bool);

/// Render `shapes` (placed by `models`) and the quad at each `placement`
/// through the stage, one round per entry of `rounds`; the pixels after
/// the last round and whether each round re-rendered.
fn render(
    shapes: &[Shape],
    models: &[Mat4],
    texture: Option<TextureData>,
    rounds: &[&[Placement]],
) -> Option<(Vec<u8>, Vec<bool>)> {
    render_lit(shapes, models, texture, &[], rounds)
}

/// The same, under `lights` rather than the default sun.
fn render_lit(
    shapes: &[Shape],
    models: &[Mat4],
    texture: Option<TextureData>,
    lights: &[Light],
    rounds: &[&[Placement]],
) -> Option<(Vec<u8>, Vec<bool>)> {
    render_scene(shapes, models, texture, lights, 0, rounds)
}

/// The same, with the last `over` shapes drawn over everything.
fn render_scene(
    shapes: &[Shape],
    models: &[Mat4],
    texture: Option<TextureData>,
    lights: &[Light],
    over: usize,
    rounds: &[&[Placement]],
) -> Option<(Vec<u8>, Vec<bool>)> {
    let rounds: Vec<Vec<Slotted>> = rounds
        .iter()
        .map(|r| r.iter().map(|&p| (p, None)).collect())
        .collect();
    render_full(shapes, models, texture, lights, over, &quad(), &rounds)
}

/// A placement and the shape in the scene it is drawn for.
pub(super) type Slotted = (Placement, Option<usize>);

/// One round of `data` at each slotted placement, the pixels back.
pub(super) fn render_stack(
    shapes: &[Shape],
    models: &[Mat4],
    data: &MeshData,
    instances: &[Slotted],
) -> Option<Vec<u8>> {
    render_full(shapes, models, None, &[], 0, data, &[instances.to_vec()]).map(|(px, _)| px)
}

/// Everything: `data` uploaded once with `texture`, drawn at each round's
/// slotted placements under `lights`, the last `over` shapes over all.
pub(super) fn render_full(
    shapes: &[Shape],
    models: &[Mat4],
    texture: Option<TextureData>,
    lights: &[Light],
    over: usize,
    data: &MeshData,
    rounds: &[Vec<Slotted>],
) -> Option<(Vec<u8>, Vec<bool>)> {
    let (device, queue) = device()?;
    let _held = exclusive();
    let mut pass = ShapePass::new(device, FORMAT);
    let mut stage = Stage::new(device, queue, FORMAT);
    let (texture_out, view) = target(device);
    let mesh = stage.upload_mesh(device, queue, data, texture.as_ref());
    let camera = Camera::stage(CANVAS);
    let mut encoder = device.create_command_encoder(&Default::default());
    let mut fresh = Vec::new();
    for placements in rounds {
        clear_black(&mut encoder, &view);
        let instances: Vec<MeshInstance> = placements
            .iter()
            .map(|&((model, color, unlit), slot)| MeshInstance {
                mesh: &mesh,
                model,
                color,
                unlit,
                slot,
            })
            .collect();
        fresh.push(stage.draw(
            device,
            queue,
            &mut encoder,
            &view,
            &mut pass,
            &Scene {
                shapes,
                models,
                paths: &[],
                meshes: &instances,
                lights,
                camera: &camera,
                time: 0.0,
                clocks: &[],
                over,
            },
            (DIM, DIM),
            framing(CENTRED),
            Quality::Live,
        ));
    }
    Some((readback(device, queue, encoder, &texture_out), fresh))
}

pub(super) fn pixel(px: &[u8], x: u32, y: u32) -> [u8; 3] {
    let i = ((y * DIM + x) * 4) as usize;
    [px[i], px[i + 1], px[i + 2]]
}

pub(super) fn near(a: [u8; 3], b: [u8; 3], tol: u8) -> bool {
    (0..3).all(|i| a[i].abs_diff(b[i]) <= tol)
}

#[test]
fn an_unlit_quad_is_exactly_its_tint() {
    let Some((px, _)) = render(&[], &[], None, &[&[(at(Vec3::ZERO), [1.0, 0.5, 0.25, 1.0], true)]])
    else {
        return;
    };
    // sRGB of (1, 0.5, 0.25): 255, 188, 137.
    assert!(near(pixel(&px, 32, 32), [255, 188, 137], 2), "{:?}", pixel(&px, 32, 32));
    assert_eq!(pixel(&px, 10, 10), [0, 0, 0]);
    // 200 units is 20 px: lit from 22 to 41 inclusive, dark at 21 and 42.
    assert!(near(pixel(&px, 22, 32), [255, 188, 137], 2));
    assert!(near(pixel(&px, 41, 32), [255, 188, 137], 2));
    assert_eq!(pixel(&px, 21, 32), [0, 0, 0]);
    assert_eq!(pixel(&px, 42, 32), [0, 0, 0]);
}

#[test]
fn the_sun_lights_a_face_turned_to_the_camera() {
    let Some((px, _)) = render(&[], &[], None, &[&[(at(Vec3::ZERO), [0.5, 0.5, 0.5, 1.0], false)]])
    else {
        return;
    };
    // n·-sun for a camera-facing face = 0.8/|(0.3, 0.5, 0.8)| ≈ 0.808, plus
    // ambient 0.22, no rim head-on: 0.5 × 1.028 ≈ 0.514 linear ≈ 188.
    let p = pixel(&px, 32, 32);
    assert!(near(p, [188, 188, 188], 4), "{p:?}");
}

#[test]
fn turning_the_quad_dims_it() {
    // The quad is built about the origin: turn it there, then place it.
    let turned = at(Vec3::ZERO) * Mat4::rotation_y(70f32.to_radians());
    let Some((px, _)) = render(&[], &[], None, &[&[(turned, [0.5, 0.5, 0.5, 1.0], false)]]) else {
        return;
    };
    // Head-on read 188; turned 70° the sun grazes it (or misses it, and
    // only ambient and the rim are left).
    let p = pixel(&px, 32, 32);
    assert!(p[0] < 184 && p[0] > 40, "turned face should be dimmer but lit: {p:?}");
}

#[test]
fn a_mesh_hides_what_is_behind_it_and_not_what_is_in_front() {
    let red = Shape::rect([CANVAS_W * 0.5, CANVAS_H * 0.5], [200.0, 100.0]).color(1.0, 0.0, 0.0);
    let blue = Shape::rect([CANVAS_W * 0.5, CANVAS_H * 0.5], [200.0, 100.0]).color(0.0, 0.0, 1.0);
    let grey = [(at(Vec3::ZERO), [0.5, 0.5, 0.5, 1.0], true)];
    let behind = Mat4::translation(Vec3::new(0.0, 0.0, -300.0));
    let Some((px, _)) = render(&[red], &[behind], None, &[&grey]) else { return };
    let p = pixel(&px, 32, 32);
    assert!(near(p, [188, 188, 188], 3), "red behind the mesh showed: {p:?}");
    // Beside the quad the red box is still there (pushed back, it spans
    // about ±17 px; the quad ±10).
    let p = pixel(&px, 45, 32);
    assert!(p[0] > 200 && p[2] < 30, "red beside the mesh: {p:?}");
    let front = Mat4::translation(Vec3::new(0.0, 0.0, 300.0));
    let Some((px, _)) = render(&[blue], &[front], None, &[&grey]) else { return };
    let p = pixel(&px, 32, 32);
    assert!(p[2] > 200 && p[0] < 30, "blue in front should cover the mesh: {p:?}");
}

#[test]
fn mesh_edges_are_antialiased() {
    // Shifted half a pixel: the right edge lands on a pixel centre.
    let Some((px, _)) = render(
        &[],
        &[],
        None,
        &[&[(at(Vec3::new(5.0, 0.0, 0.0)), [1.0, 1.0, 1.0, 1.0], true)]],
    ) else {
        return;
    };
    let edge = pixel(&px, 42, 32)[0];
    assert!(edge > 60 && edge < 230, "edge pixel {edge} should be partial");
    assert_eq!(pixel(&px, 41, 32), [255, 255, 255]);
    assert_eq!(pixel(&px, 43, 32), [0, 0, 0]);
}

#[test]
fn a_texture_lands_on_the_quad() {
    let base = vec![
        255, 0, 0, 255, 0, 255, 0, 255, //
        0, 0, 255, 255, 255, 255, 255, 255,
    ];
    let tex = TextureData {
        width: 2,
        height: 2,
        levels: vec![base, vec![128, 128, 128, 255]],
    };
    // Shifted half a pixel so the quadrant centres land on pixel centres:
    // there the samples sit on texel centres and come back pure. (Half a
    // texel off, the 5% bilinear bleed of a neighbour reads as 64 in
    // sRGB — the dark end of the curve is steep.)
    let Some((px, _)) = render(
        &[],
        &[],
        Some(tex),
        &[&[(at(Vec3::new(5.0, 5.0, 0.0)), [1.0; 4], true)]],
    ) else {
        return;
    };
    assert!(near(pixel(&px, 27, 27), [255, 0, 0], 3), "{:?}", pixel(&px, 27, 27));
    assert!(near(pixel(&px, 37, 27), [0, 255, 0], 3), "{:?}", pixel(&px, 37, 27));
    assert!(near(pixel(&px, 27, 37), [0, 0, 255], 3), "{:?}", pixel(&px, 27, 37));
    assert!(near(pixel(&px, 37, 37), [255, 255, 255], 3), "{:?}", pixel(&px, 37, 37));
}

#[test]
fn a_halo_behind_a_mesh_does_not_glow_through() {
    let lamp = Shape::rect([CANVAS_W * 0.5, CANVAS_H * 0.5], [60.0, 60.0])
        .color(1.0, 0.0, 0.0)
        .glow(80.0);
    let behind = Mat4::translation(Vec3::new(0.0, 0.0, -300.0));
    // Without the mesh, the lamp's red light reaches well past its body.
    // Probe inside the quad's footprint (±10 px) but outside the lamp's
    // pushed-back body (about ±5 px), where only halo light can be.
    let Some((px, _)) = render(&[lamp], &[behind], None, &[&[]]) else { return };
    let p = pixel(&px, 32, 32 + 8);
    assert!(p[0] > 20 && p[1] == 0, "the halo should be red light here: {p:?}");
    // With a grey quad in front, that pixel is the quad and nothing else.
    let grey = [(at(Vec3::ZERO), [0.5, 0.5, 0.5, 1.0], true)];
    let Some((px, _)) = render(&[lamp], &[behind], None, &[&grey]) else { return };
    let p = pixel(&px, 32, 32 + 8);
    assert!(p[0] == p[1] && p[1] == p[2], "red glowed through the mesh: {p:?}");
}

/// A mark drawn over everything — the transform gizmo — is still there
/// when it sits inside a mesh: a red mark behind a grey quad is hidden
/// in the scene, and on top of the quad once it is counted `over`.
#[test]
fn a_mark_drawn_over_everything_shows_through_a_mesh() {
    let mark = Shape::rect([CANVAS_W * 0.5, CANVAS_H * 0.5], [60.0, 60.0]).color(1.0, 0.0, 0.0);
    let behind = Mat4::translation(Vec3::new(0.0, 0.0, -300.0));
    let grey = [(at(Vec3::ZERO), [0.5, 0.5, 0.5, 1.0], true)];
    let Some((px, _)) = render_scene(&[mark], &[behind], None, &[], 0, &[&grey]) else { return };
    let p = pixel(&px, 32, 32);
    assert!(p[0] == p[1] && p[1] == p[2], "in the scene the quad hides the mark: {p:?}");
    let Some((px, _)) = render_scene(&[mark], &[behind], None, &[], 1, &[&grey]) else { return };
    let p = pixel(&px, 32, 32);
    assert!(p[0] > 200 && p[1] < 30, "drawn over, the mark is on top of the quad: {p:?}");
    // And the change of standing alone is a cache miss, then a hit.
    let Some((_, fresh)) = render_scene(&[mark], &[behind], None, &[], 1, &[&grey, &grey]) else {
        return;
    };
    assert_eq!(fresh, vec![true, false]);
}

/// A grey receiver three quads wide on the canvas plane, and a grey
/// quad `z` in front of it to cast onto it.
pub(super) fn receiver_and_blocker(z: f32) -> [Placement; 2] {
    let grey = [0.5, 0.5, 0.5, 1.0];
    [
        (at(Vec3::ZERO) * Mat4::scaling(Vec3::new(3.0, 3.0, 1.0)), grey, false),
        (at(Vec3::new(0.0, 0.0, z)), grey, false),
    ]
}

#[test]
fn a_sun_casts_a_meshs_shadow_onto_a_mesh() {
    // The sun travels right and away, 3:4, so a quad 300 in front of the
    // receiver shadows it 225 to the right — 22 px here, clear of the
    // quad's own footprint (±12 px about the centre). Lit is the sun's
    // 0.8 head-on: 0.5 × (0.22 + 0.8) → 188; shadowed is ambient: 93.
    let sun = Light {
        direction: Vec3::new(0.6, 0.0, -0.8),
        ..Light::default_sun()
    };
    let Some((px, _)) = render_lit(&[], &[], None, &[sun], &[&receiver_and_blocker(300.0)]) else {
        return;
    };
    let lit = pixel(&px, 10, 32)[0];
    let shadowed = pixel(&px, 54, 32)[0];
    assert!((180..=196).contains(&lit), "in the sun: {lit}");
    assert!((86..=100).contains(&shadowed), "in the shadow: {shadowed}");
    // The quad in front lights cleanly — no acne from its own map.
    let front = pixel(&px, 32, 32)[0];
    assert!((180..=196).contains(&front), "the caster itself: {front}");
}

#[test]
fn a_spot_casts_a_shadow_too() {
    // From up-left at 45°, 300 above the receiver: a quad 100 in front
    // shadows it 150 to the right (15 px), 300 wide. Lit at 15 px left:
    // n·l 0.894, 335 away with a range of 400 (1.05×): 0.5 × (0.22 +
    // 0.94) → 201. Shadowed: ambient alone, 93.
    let spot = Light {
        kind: LightKind::Spot,
        position: Vec3::new(CANVAS_W * 0.5 - 300.0, CANVAS_H * 0.5, 300.0),
        direction: Vec3::new(1.0, 0.0, -1.0).normalized(),
        color: [1.0; 3],
        range: 400.0,
        cone: 60f32.to_radians(),
        soft: 0.0,
        rim: 0.0,
    };
    let Some((px, _)) = render_lit(&[], &[], None, &[spot], &[&receiver_and_blocker(100.0)]) else {
        return;
    };
    let lit = pixel(&px, 17, 32)[0];
    let shadowed = pixel(&px, 47, 32)[0];
    assert!(lit > 160, "in the spot: {lit}");
    assert!(shadowed < 110, "in its shadow: {shadowed}");
}

#[test]
fn a_moved_mesh_is_a_cache_miss() {
    let a = [(at(Vec3::ZERO), [1.0; 4], true)];
    let b = [(at(Vec3::new(50.0, 0.0, 0.0)), [1.0; 4], true)];
    let Some((_, fresh)) = render(&[], &[], None, &[&a, &a, &b, &b]) else { return };
    assert_eq!(fresh, vec![true, false, true, false]);
}

fn grey() -> [Placement; 1] {
    [(at(Vec3::ZERO), [0.5, 0.5, 0.5, 1.0], false)]
}

fn point_at(z: f32, range: f32) -> Light {
    Light {
        kind: LightKind::Point,
        position: Vec3::new(CANVAS_W * 0.5, CANVAS_H * 0.5, z),
        direction: Vec3::new(0.0, 0.0, -1.0),
        color: [1.0; 3],
        range,
        cone: 0.0,
        soft: 0.0,
        rim: 0.0,
    }
}

/// Light from everywhere at `level`, no rim.
fn ambient(level: f32) -> Light {
    Light {
        kind: LightKind::Ambient,
        color: [level; 3],
        ..point_at(0.0, 0.0)
    }
}

#[test]
fn a_point_light_is_inverse_square_from_its_range() {
    // 300 in front of the face, at its range of 300: attenuation
    // 1 / (1 + 0.25) = 0.8, head-on, so 0.5 × (0.22 + 0.8) = 0.51 → 188.
    let Some((px, _)) = render_lit(&[], &[], None, &[point_at(300.0, 300.0)], &[&grey()]) else {
        return;
    };
    let lit = pixel(&px, 32, 32)[0];
    assert!((180..=196).contains(&lit), "at its range: {lit}");
    // Twice as far past its range: a quarter as bright, not gone —
    // 90000 / (360000 + 22500) ≈ 0.235, so 0.5 × 0.455 ≈ 0.228 → 131.
    let Some((px, _)) = render_lit(&[], &[], None, &[point_at(600.0, 300.0)], &[&grey()]) else {
        return;
    };
    let far = pixel(&px, 32, 32)[0];
    assert!((124..=138).contains(&far), "twice the range: {far}");
    // Close in it blows out: 300 away with a range of 900 is 2.77×.
    let Some((px, _)) = render_lit(&[], &[], None, &[point_at(300.0, 900.0)], &[&grey()]) else {
        return;
    };
    assert_eq!(pixel(&px, 32, 32)[0], 255);
}

#[test]
fn a_spot_lights_its_cone_and_not_beside_it() {
    let spot = Light {
        kind: LightKind::Spot,
        cone: 10f32.to_radians(),
        soft: 0.2,
        ..point_at(300.0, 300.0)
    };
    let Some((px, _)) = render_lit(&[], &[], None, &[spot], &[&grey()]) else { return };
    let on_axis = pixel(&px, 32, 32)[0];
    assert!((180..=196).contains(&on_axis), "on axis: {on_axis}");
    // 80 units off the axis, 300 away: 15° out, past the 10° cone.
    let beside = pixel(&px, 32 + 8, 32)[0];
    assert!((86..=100).contains(&beside), "beside the cone: {beside}");
}

/// An ambient light sets the scene's level in place of the default, and
/// the default sun keeps shining beside it — adding one is not turning
/// the lights out.
#[test]
fn an_ambient_light_sets_the_level_and_keeps_the_sun() {
    // 0.5 × (0.5 + the sun's ≈0.8 head-on) ≈ 0.65 → 211.
    let Some((px, _)) = render_lit(&[], &[], None, &[ambient(0.5)], &[&grey()]) else { return };
    let lit = pixel(&px, 32, 32)[0];
    assert!((203..=219).contains(&lit), "ambient 0.5 + sun: {lit}");
    // A black ambient: the sun alone, 0.5 × 0.8 = 0.4 → 170.
    let Some((px, _)) = render_lit(&[], &[], None, &[ambient(0.0)], &[&grey()]) else { return };
    let dark = pixel(&px, 32, 32)[0];
    assert!((162..=178).contains(&dark), "ambient 0: {dark}");
}

#[test]
fn a_coloured_sun_tints_the_face() {
    let red = Light {
        color: [1.0, 0.0, 0.0],
        direction: Vec3::new(0.0, 0.0, -1.0),
        ..Light::default_sun()
    };
    let Some((px, _)) = render_lit(&[], &[], None, &[red], &[&grey()]) else { return };
    let p = pixel(&px, 32, 32);
    // r: 0.5 × (0.22 + 1.0) = 0.61 → 205; g, b: ambient alone → 93.
    assert!((198..=212).contains(&p[0]) && (86..=100).contains(&p[1]), "{p:?}");
}

#[test]
fn a_moved_light_is_a_cache_miss() {
    let (device, queue) = match device() {
        Some(d) => d,
        None => return,
    };
    let _ = (device, queue);
    let a = point_at(300.0, 300.0);
    let b = point_at(500.0, 300.0);
    // Two rounds under one light hit the cache; a moved light misses.
    let Some((_, fresh)) = render_lit(&[], &[], None, &[a], &[&grey(), &grey()]) else { return };
    assert_eq!(fresh, vec![true, false]);
    let Some((px_a, _)) = render_lit(&[], &[], None, &[a], &[&grey()]) else { return };
    let Some((px_b, _)) = render_lit(&[], &[], None, &[b], &[&grey()]) else { return };
    assert_ne!(pixel(&px_a, 32, 32), pixel(&px_b, 32, 32));
}
