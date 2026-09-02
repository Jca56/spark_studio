//! See-through meshes, by pixels: Alva's ghost at half opacity has to
//! show the swirl and the plane behind it, sit under what is in front,
//! keep its own inside to itself, stack where the outliner puts it, and
//! cast until it is gone.

use super::tests::{Slotted, at, near, pixel, quad, receiver_and_blocker, render_full, render_stack};
use super::*;
use crate::light::Light;
use crate::math::{Mat4, Vec3};
use crate::shapes::{CANVAS_H, CANVAS_W, Shape};
/// A slab: the quad as a face at z = +50 and again at z = −50, one mesh
/// with an inside — what a fading solid must not show.
fn slab() -> MeshData {
    let q = quad();
    let face = |z: f32| q.positions.iter().map(move |p| [p[0], p[1], z]);
    MeshData {
        positions: face(50.0).chain(face(-50.0)).collect(),
        normals: vec![[0.0, 0.0, 1.0]; 8],
        uvs: q.uvs.iter().chain(q.uvs.iter()).copied().collect(),
        indices: q.indices.iter().copied().chain(q.indices.iter().map(|i| i + 4)).collect(),
    }
}

/// The grey quad at half opacity, unlit: 0.25 of light plus half of
/// whatever is behind it.
fn faded(d: Vec3, slot: Option<usize>) -> Slotted {
    ((at(d), [0.5, 0.5, 0.5, 0.5], true), slot)
}

fn centre_rect() -> Shape {
    Shape::rect([CANVAS_W * 0.5, CANVAS_H * 0.5], [200.0, 100.0])
}

/// Half grey over red, in sRGB: (0.75, 0.25, 0.25) → 225, 137, 137.
const GREY_OVER_RED: [u8; 3] = [225, 137, 137];

/// Alva's ghost: a mesh at half opacity in front of a shape shows the
/// shape through it at half — not the checkerboard, not the mesh alone.
#[test]
fn a_faded_mesh_shows_what_is_behind_it() {
    let red = centre_rect().color(1.0, 0.0, 0.0);
    let behind = Mat4::translation(Vec3::new(0.0, 0.0, -300.0));
    let Some(px) = render_stack(&[red], &[behind], &quad(), &[faded(Vec3::ZERO, None)]) else {
        return;
    };
    let p = pixel(&px, 32, 32);
    assert!(near(p, GREY_OVER_RED, 4), "half grey over red: {p:?}");
    // At zero it is not there at all.
    let gone = ((at(Vec3::ZERO), [0.5, 0.5, 0.5, 0.0], true), None);
    let Some(px) = render_stack(&[red], &[behind], &quad(), &[gone]) else { return };
    let p = pixel(&px, 32, 32);
    assert!(p[0] > 250 && p[1] < 5, "an invisible mesh leaves the red alone: {p:?}");
}

/// The plane behind the ghost: an opaque mesh behind a see-through one
/// shows through it too, whichever is first in the list.
#[test]
fn a_faded_mesh_shows_a_mesh_behind_it() {
    let plane = at(Vec3::new(0.0, 0.0, -300.0)) * Mat4::scaling(Vec3::new(3.0, 3.0, 1.0));
    let blue = ((plane, [0.0, 0.0, 1.0, 1.0], true), None);
    let Some(px) = render_stack(&[], &[], &quad(), &[faded(Vec3::ZERO, None), blue]) else {
        return;
    };
    let p = pixel(&px, 32, 32);
    assert!(near(p, [137, 137, 225], 4), "half grey over the blue plane: {p:?}");
    // Beside the ghost, the plane alone.
    let p = pixel(&px, 50, 32);
    assert!(p[2] > 250 && p[0] < 5, "the plane beside the ghost: {p:?}");
}

#[test]
fn a_shape_in_front_of_a_faded_mesh_stays_in_front() {
    let blue = centre_rect().color(0.0, 0.0, 1.0);
    let front = Mat4::translation(Vec3::new(0.0, 0.0, 300.0));
    let Some(px) = render_stack(&[blue], &[front], &quad(), &[faded(Vec3::ZERO, None)]) else {
        return;
    };
    let p = pixel(&px, 32, 32);
    assert!(p[2] > 250 && p[0] < 5, "blue in front of the faded mesh: {p:?}");
}

/// A fading solid is a solid: its nearest surface at its opacity over the
/// scene, and its own inside never shows through. Without the depth
/// prepass the back face would add another quarter (0.375 → 164).
#[test]
fn a_faded_solid_shows_the_scene_and_not_its_own_inside() {
    let Some(px) = render_stack(&[], &[], &slab(), &[faded(Vec3::ZERO, None)]) else {
        return;
    };
    let p = pixel(&px, 32, 32);
    assert!(near(p, [137, 137, 137], 3), "one face of grey over black: {p:?}");
    // A red shape behind the slab shows through it as through the quad.
    let red = centre_rect().color(1.0, 0.0, 0.0);
    let behind = Mat4::translation(Vec3::new(0.0, 0.0, -300.0));
    let Some(px) = render_stack(&[red], &[behind], &slab(), &[faded(Vec3::ZERO, None)]) else {
        return;
    };
    let p = pixel(&px, 32, 32);
    assert!(near(p, GREY_OVER_RED, 4), "half grey over red through a solid: {p:?}");
}

/// A 2D comp: a see-through mesh and a shape on the one plane stack the
/// way the outliner reads, because the mesh draws in its shape's place.
#[test]
fn a_see_through_mesh_stacks_in_its_outliner_place() {
    let red = centre_rect().color(1.0, 0.0, 0.0);
    let ghost = Shape::mesh([CANVAS_W * 0.5, CANVAS_H * 0.5], [100.0, 100.0], 0);
    // Red first, the ghost above it: half grey over red.
    let Some(px) = render_stack(&[red, ghost], &[], &quad(), &[faded(Vec3::ZERO, Some(1))]) else {
        return;
    };
    let p = pixel(&px, 32, 32);
    assert!(near(p, GREY_OVER_RED, 4), "the ghost above the red: {p:?}");
    // The ghost first, red above it: red, and no ghost to see.
    let Some(px) = render_stack(&[ghost, red], &[], &quad(), &[faded(Vec3::ZERO, Some(0))]) else {
        return;
    };
    let p = pixel(&px, 32, 32);
    assert!(p[0] > 250 && p[1] < 5, "the red above the ghost: {p:?}");
}

/// A shadow map is yes or no: a mesh casts while it is drawn at all, and
/// stops the moment it is gone.
#[test]
fn a_fading_mesh_still_casts_and_a_gone_one_does_not() {
    let sun = Light {
        direction: Vec3::new(0.6, 0.0, -0.8),
        ..Light::default_sun()
    };
    let [receiver, (model, _, unlit)] = receiver_and_blocker(300.0);
    let at_opacity = |a: f32| vec![(receiver, None), ((model, [0.5, 0.5, 0.5, a], unlit), None)];
    let Some((px, _)) = render_full(&[], &[], None, &[sun], 0, &quad(), &[at_opacity(0.5)]) else {
        return;
    };
    let shadowed = pixel(&px, 54, 32)[0];
    assert!((86..=100).contains(&shadowed), "half faded, still in its shadow: {shadowed}");
    let Some((px, _)) = render_full(&[], &[], None, &[sun], 0, &quad(), &[at_opacity(0.0)]) else {
        return;
    };
    let lit = pixel(&px, 54, 32)[0];
    assert!((180..=196).contains(&lit), "gone, and its shadow with it: {lit}");
}
