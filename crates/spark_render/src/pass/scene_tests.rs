//! Scene tests: the camera and the per-object matrices, checked by pixels.
//!
//! The 2D tests in `tests.rs` are the identity proof — every one of them
//! now renders through the camera and has to come out as it did. These
//! cover what only a scene can do: a shape turned out of the canvas plane
//! narrows, a shape pushed back shrinks toward the vanishing point, and a
//! nearer shape is drawn over a farther one whatever order they were
//! listed in.

use super::harness::{DIM, VIEW, render, render_scene};
use super::*;
use crate::shapes::{CANVAS_H, CANVAS_W};

/// A view that puts the canvas centre — the stage camera's vanishing point
/// — in the middle of the test target, so a pushed-back shape shrinks in
/// place instead of sliding off the edge.
const CENTRED: (f32, f32, f32) = (VIEW, -CANVAS_W * VIEW * 0.5 + 32.0, -CANVAS_H * VIEW * 0.5 + 32.0);

/// A 40×20 px white box at the vanishing point.
fn centre_box() -> Shape {
    Shape::rect([CANVAS_W * 0.5, CANVAS_H * 0.5], [200.0, 100.0])
}

fn pixel(pixels: &[u8], x: u32, y: u32) -> [u8; 4] {
    let i = ((y * DIM + x) * 4) as usize;
    [pixels[i], pixels[i + 1], pixels[i + 2], pixels[i + 3]]
}

/// Solid body: well over half covered, so the one-pixel anti-aliasing
/// fringe outside an edge (about 110–130 here) doesn't count and a 40 px
/// box is 40 columns.
fn lit(pixels: &[u8], x: u32, y: u32) -> bool {
    let p = pixel(pixels, x, y);
    p[0] > 160 || p[1] > 160 || p[2] > 160
}

/// Any light at all, halo included.
fn glows(pixels: &[u8], x: u32, y: u32) -> bool {
    let p = pixel(pixels, x, y);
    p[0] > 16 || p[1] > 16 || p[2] > 16
}

/// The lit columns along one row, as (first, last) inclusive.
fn span_x(pixels: &[u8], y: u32) -> Option<(u32, u32)> {
    let cols: Vec<u32> = (0..DIM).filter(|&x| lit(pixels, x, y)).collect();
    Some((*cols.first()?, *cols.last()?))
}

fn span_y(pixels: &[u8], x: u32) -> Option<(u32, u32)> {
    let rows: Vec<u32> = (0..DIM).filter(|&y| lit(pixels, x, y)).collect();
    Some((*rows.first()?, *rows.last()?))
}

fn width(span: Option<(u32, u32)>) -> u32 {
    span.map(|(a, b)| b - a + 1).unwrap_or(0)
}

#[test]
fn no_models_is_the_canvas_plane() {
    let shapes = [
        Shape::circle([200.0, 200.0], 120.0).color(1.0, 0.4, 0.1),
        Shape::rect([400.0, 300.0], [150.0, 60.0]).rot(0.4).color(0.2, 0.5, 1.0),
    ];
    let Some(flat) = render(&shapes, 0.0) else { return };
    let ids = vec![Mat4::IDENTITY; shapes.len()];
    let Some(placed) = render_scene(&shapes, &ids, (VIEW, 0.0, 0.0), 0.0) else { return };
    assert_eq!(flat, placed);
}

#[test]
fn a_box_at_the_vanishing_point_is_drawn_at_its_size() {
    let Some(px) = render_scene(&[centre_box()], &[], CENTRED, 0.0) else { return };
    let w = width(span_x(&px, 32));
    let h = width(span_y(&px, 32));
    assert!((39..=41).contains(&w), "width {w}");
    assert!((19..=21).contains(&h), "height {h}");
    assert!(lit(&px, 32, 32));
}

#[test]
fn turning_a_shape_narrows_it() {
    let b = centre_box();
    let c = b.center();
    let turn = Mat4::about(
        Vec3::new(c[0], c[1], 0.0),
        Mat4::rotation_y(60f32.to_radians()),
    );
    let Some(px) = render_scene(&[b], &[turn], CENTRED, 0.0) else { return };
    // cos 60° of forty is twenty; perspective pulls the near half wider
    // and the far half narrower by nearly the same amount.
    let w = width(span_x(&px, 32));
    assert!((16..=26).contains(&w), "turned width {w}");
    // Height is untouched by a turn about y.
    let h = width(span_y(&px, 32));
    assert!((19..=21).contains(&h), "turned height {h}");
}

#[test]
fn tilting_a_shape_shortens_it() {
    let b = centre_box();
    let c = b.center();
    let tilt = Mat4::about(
        Vec3::new(c[0], c[1], 0.0),
        Mat4::rotation_x(60f32.to_radians()),
    );
    let Some(px) = render_scene(&[b], &[tilt], CENTRED, 0.0) else { return };
    let h = width(span_y(&px, 32));
    assert!((7..=13).contains(&h), "tilted height {h}");
    let w = width(span_x(&px, 32));
    assert!((39..=41).contains(&w), "tilted width {w}");
}

#[test]
fn twice_as_far_is_half_as_big_and_still_centred() {
    let cam = Camera::stage();
    let d = (cam.target - cam.eye).length();
    let back = Mat4::translation(Vec3::new(0.0, 0.0, -d));
    let Some(px) = render_scene(&[centre_box()], &[back], CENTRED, 0.0) else { return };
    let w = width(span_x(&px, 32));
    let h = width(span_y(&px, 32));
    assert!((18..=22).contains(&w), "far width {w}");
    assert!((8..=12).contains(&h), "far height {h}");
    let (x0, x1) = span_x(&px, 32).unwrap();
    assert!((x0 + x1).abs_diff(64) <= 2, "far box drifted: {x0}..{x1}");
}

#[test]
fn halfway_to_the_camera_is_twice_as_big() {
    let cam = Camera::stage();
    let d = (cam.target - cam.eye).length();
    let toward = Mat4::translation(Vec3::new(0.0, 0.0, d * 0.5));
    let Some(px) = render_scene(&[centre_box()], &[toward], CENTRED, 0.0) else { return };
    let h = width(span_y(&px, 32));
    assert!((38..=42).contains(&h), "near height {h}");
}

#[test]
fn the_nearer_shape_wins_whatever_the_list_says() {
    let red = centre_box().color(1.0, 0.0, 0.0);
    let blue = centre_box().color(0.0, 0.0, 1.0);
    let nearer = Mat4::translation(Vec3::new(0.0, 0.0, 300.0));
    let farther = Mat4::translation(Vec3::new(0.0, 0.0, -300.0));
    // Red is listed first but sits nearer: it must be drawn last.
    let Some(px) = render_scene(&[red, blue], &[nearer, Mat4::IDENTITY], CENTRED, 0.0) else {
        return;
    };
    let p = pixel(&px, 32, 32);
    assert!(p[0] > 200 && p[2] < 30, "expected red on top, got {p:?}");
    // Red is listed last but sits farther: blue must be drawn over it.
    let Some(px) = render_scene(&[blue, red], &[Mat4::IDENTITY, farther], CENTRED, 0.0) else {
        return;
    };
    let p = pixel(&px, 32, 32);
    assert!(p[2] > 200 && p[0] < 30, "expected blue on top, got {p:?}");
}

#[test]
fn shapes_at_one_depth_keep_their_list_order() {
    let red = centre_box().color(1.0, 0.0, 0.0);
    let blue = centre_box().color(0.0, 0.0, 1.0);
    let Some(px) = render_scene(&[red, blue], &[], CENTRED, 0.0) else { return };
    let p = pixel(&px, 32, 32);
    assert!(p[2] > 200 && p[0] < 30, "the later shape is on top: {p:?}");
    let Some(px) = render_scene(&[blue, red], &[], CENTRED, 0.0) else { return };
    let p = pixel(&px, 32, 32);
    assert!(p[0] > 200 && p[2] < 30, "the later shape is on top: {p:?}");
}

#[test]
fn a_turned_shape_keeps_its_glow_on_its_plane() {
    // A glowing box turned edge-on-ish still spills light, and only near
    // its (now narrow) body — the halo is on the plane, not on the screen.
    let b = centre_box().glow(60.0);
    let c = b.center();
    let turn = Mat4::about(
        Vec3::new(c[0], c[1], 0.0),
        Mat4::rotation_y(75f32.to_radians()),
    );
    let Some(px) = render_scene(&[b], &[turn], CENTRED, 0.0) else { return };
    assert!(lit(&px, 32, 32), "body gone");
    // Light just outside the narrowed body on its near side (a positive
    // turn swings the left edge toward the camera), none far out along
    // the row.
    assert!(glows(&px, 32 - 9, 32), "no halo beside the body");
    assert!(!glows(&px, 63, 32), "halo reached the far edge");
}

#[test]
fn the_stage_sorts_the_same_way() {
    let red = centre_box().color(1.0, 0.0, 0.0);
    let blue = centre_box().color(0.0, 0.0, 1.0);
    let nearer = Mat4::translation(Vec3::new(0.0, 0.0, 300.0));
    let Some((px, _)) = super::stage_tests::render_staged_scene(
        &[red, blue],
        &[nearer, Mat4::IDENTITY],
        CENTRED,
        &[(0.0, false)],
    ) else {
        return;
    };
    let p = pixel(&px, 32, 32);
    assert!(p[0] > 200 && p[2] < 30, "expected red on top through the stage, got {p:?}");
}

