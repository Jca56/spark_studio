//! Every curve verb with its undo — nobody who can run these can
//! drag a diamond.

use super::*;
use crate::props::{Prop, Tool};

/// A circle with an X curve of three keys on its one clip.
fn keyed() -> (Editor, usize) {
    let mut e = Editor::empty();
    e.set_time(0.0);
    e.sync_to_time();
    e.choose_tool(Tool::Circle);
    e.set_cursor_canvas([300.0, 300.0]);
    e.mouse_down(false);
    e.set_cursor_canvas([380.0, 300.0]);
    e.mouse_up();
    e.choose_tool(Tool::Select);
    let i = e.primary().expect("drawn");
    for (t, x) in [(0.0, 300.0), (0.5, 600.0), (1.0, 900.0)] {
        e.set_time(t);
        e.sync_to_time();
        e.set_prop(Prop::X, x);
        assert!(e.stamp_key());
    }
    e.end_gesture();
    (e, i)
}

fn x_keys(e: &Editor, i: usize) -> Vec<(f32, f32)> {
    e.clip_anim(i, 0)
        .and_then(|a| a.track(Target::Shape(Prop::X)))
        .map(|tr| tr.keys.iter().map(|k| (k.t, k.v)).collect())
        .unwrap_or_default()
}

/// A dragged key moves in both time and value, stops at its
/// neighbours, and the whole drag is one undo step.
#[test]
fn a_key_drags_between_its_neighbours_as_one_step() {
    let (mut e, i) = keyed();
    let x = Target::Shape(Prop::X);
    assert!(e.move_key(i, 0, x, 1, 0.6, 650.0));
    assert!(e.move_key(i, 0, x, 1, 0.7, 700.0));
    // Past the last key: stops just short of it.
    assert!(e.move_key(i, 0, x, 1, 5.0, 700.0));
    e.end_gesture();
    let keys = x_keys(&e, i);
    assert!(
        keys[1].0 < 1.0 && keys[1].0 > 0.99,
        "clamped at {}",
        keys[1].0
    );
    assert_eq!(keys[1].1, 700.0);
    // Before the first key: stops just after it.
    assert!(e.move_key(i, 0, x, 1, -3.0, 700.0));
    e.end_gesture();
    assert!(x_keys(&e, i)[1].0 > 0.0);
    e.undo();
    assert!(x_keys(&e, i)[1].0 > 0.99, "the second drag undid");
    e.undo();
    assert_eq!(x_keys(&e, i)[1], (0.5, 600.0), "the first drag undid whole");
}

/// The curve re-poses the object the moment a key moves — no
/// stamping, no playhead nudge needed.
#[test]
fn moving_a_key_moves_the_shape() {
    let (mut e, i) = keyed();
    e.set_time(0.5);
    e.sync_to_time();
    assert!((e.shapes()[i].center()[0] - 600.0).abs() < 1e-3);
    assert!(e.move_key(i, 0, Target::Shape(Prop::X), 1, 0.5, 100.0));
    e.sync_to_time();
    assert!((e.shapes()[i].center()[0] - 100.0).abs() < 1e-3);
}

/// The strip retimes every key at a moment together, and stops where
/// the tightest track would cross a neighbour.
#[test]
fn a_moment_retimes_across_every_track() {
    let (mut e, i) = keyed();
    // A second track with a key at 0.5 and a tight neighbour at 0.6.
    e.clip_anim_mut(i, 0).tracks.push(crate::anim::Track {
        target: Target::Shape(Prop::Opacity),
        keys: vec![
            Key {
                t: 0.5,
                v: 1.0,
                ease: Ease::Smooth,
            },
            Key {
                t: 0.6,
                v: 2.0,
                ease: Ease::Smooth,
            },
        ],
    });
    let landed = e.retime_keys_at(i, 0, 0.5, 0.9).expect("keys at 0.5");
    assert!(
        landed < 0.6 && landed > 0.59,
        "stopped at Opacity's neighbour: {landed}"
    );
    let xs = x_keys(&e, i);
    assert!((xs[1].0 - landed).abs() < 1e-6, "X moved with it");
    assert_eq!(e.retime_keys_at(i, 0, 3.0, 4.0), None, "nothing there");
    e.end_gesture();
    e.undo();
    assert_eq!(x_keys(&e, i)[1].0, 0.5);
}

/// A key added on the line lands on the curve's own value, so the
/// motion is unchanged until it is dragged; the last key deleted
/// takes the track away.
#[test]
fn keys_add_on_the_line_and_delete_down_to_nothing() {
    let (mut e, i) = keyed();
    let x = Target::Shape(Prop::X);
    let before = e
        .clip_anim(i, 0)
        .unwrap()
        .track(x)
        .unwrap()
        .sample(0.25)
        .unwrap();
    let k = e.add_key(i, 0, x, 0.25).expect("added");
    assert_eq!(k, 1);
    let keys = x_keys(&e, i);
    assert_eq!(keys.len(), 4);
    assert!((keys[1].1 - before).abs() < 1e-4, "on the line");
    assert_eq!(e.add_key(i, 0, x, 0.25), None, "not twice");
    for _ in 0..4 {
        assert!(e.delete_key(i, 0, x, 0));
    }
    assert!(e.clip_anim(i, 0).unwrap().track(x).is_none(), "track gone");
    assert!(!e.delete_key(i, 0, x, 0));
    e.undo();
    assert_eq!(x_keys(&e, i).len(), 1, "undo brings the last key back");
}

/// A setting with no curve yet gets one from a double-click: its
/// first key holds the object's value as it stands.
#[test]
fn a_first_key_lands_on_the_objects_value() {
    let (mut e, i) = keyed();
    let op = Target::Shape(Prop::Opacity);
    assert!(e.clip_anim(i, 0).unwrap().track(op).is_none());
    e.set_time(0.5);
    e.sync_to_time();
    e.set_prop(Prop::Opacity, 0.4);
    let k = e.add_key(i, 0, op, 0.5).expect("a fresh track");
    assert_eq!(k, 0);
    let tr = e.clip_anim(i, 0).unwrap().track(op).unwrap();
    assert_eq!(tr.keys.len(), 1);
    assert!((tr.keys[0].v - 0.4).abs() < 1e-5, "the value on screen");
    e.undo();
    assert!(
        e.clip_anim(i, 0).unwrap().track(op).is_none(),
        "undo takes the track"
    );
}

/// Delete at a moment clears every track's key there.
#[test]
fn a_moment_deletes_across_every_track() {
    let (mut e, i) = keyed();
    e.clip_anim_mut(i, 0).tracks.push(crate::anim::Track {
        target: Target::Shape(Prop::Opacity),
        keys: vec![Key {
            t: 0.5,
            v: 1.0,
            ease: Ease::Smooth,
        }],
    });
    assert!(e.delete_keys_at(i, 0, 0.5));
    assert_eq!(x_keys(&e, i).len(), 2);
    assert!(
        e.clip_anim(i, 0)
            .unwrap()
            .track(Target::Shape(Prop::Opacity))
            .is_none()
    );
    assert!(!e.delete_keys_at(i, 0, 0.5));
}

/// Keys are linear from birth; ease flips to smooth and back, and
/// undoes.
#[test]
fn keys_are_linear_by_default_and_ease_toggles() {
    let (mut e, i) = keyed();
    let x = Target::Shape(Prop::X);
    let tr = e.clip_anim(i, 0).unwrap().track(x).unwrap();
    assert!(
        tr.keys.iter().all(|k| k.ease == Ease::Linear),
        "linear from K"
    );
    assert!(
        (tr.sample(0.25).unwrap() - 450.0).abs() < 1e-3,
        "a straight line"
    );
    assert!(e.toggle_key_ease(i, 0, x, 0));
    let tr = e.clip_anim(i, 0).unwrap().track(x).unwrap();
    assert_eq!(tr.keys[0].ease, Ease::Smooth);
    assert!((tr.sample(0.25).unwrap() - 450.0).abs() > 1.0, "eased now");
    assert!(e.toggle_key_ease(i, 0, x, 0));
    assert_eq!(
        e.clip_anim(i, 0).unwrap().track(x).unwrap().keys[0].ease,
        Ease::Linear
    );
    e.undo();
    assert_eq!(
        e.clip_anim(i, 0).unwrap().track(x).unwrap().keys[0].ease,
        Ease::Smooth
    );
    let k = e.add_key(i, 0, x, 0.25).expect("added");
    assert_eq!(
        e.clip_anim(i, 0).unwrap().track(x).unwrap().keys[k].ease,
        Ease::Linear,
        "an added key is linear too"
    );
}

/// `K` with a key picked: the key takes the setting's value as it
/// stands, wherever the playhead is; a moment picked on the strip
/// updates every key there.
#[test]
fn a_picked_key_restamps_at_the_value_as_it_stands() {
    let (mut e, i) = keyed();
    let x = Target::Shape(Prop::X);
    // Playhead at 1.0, but the picked key is the middle one (0.5).
    e.set_time(1.0);
    e.sync_to_time();
    e.set_prop(Prop::X, 123.0);
    assert!(e.restamp_key(i, 0, x, 1));
    let keys = x_keys(&e, i);
    assert_eq!(keys[1], (0.5, 123.0), "the picked key took it");
    assert_eq!(keys.len(), 3, "no key planted at the playhead");
    assert!(!e.restamp_key(i, 0, x, 1), "already there: nothing to do");
    e.set_prop(Prop::X, 77.0);
    assert!(e.restamp_keys_at(i, 0, 0.0));
    assert_eq!(x_keys(&e, i)[0], (0.0, 77.0));
    e.undo();
    assert_eq!(x_keys(&e, i)[0], (0.0, 300.0));
}

/// A transform key goes where the drag puts it — the old Z ceiling
/// snapped a key the gizmo had placed at 2800 back down to 1400 the
/// moment it was touched.
#[test]
fn transform_keys_have_no_walls() {
    let (mut e, i) = keyed();
    let z = Target::Shape(Prop::Z);
    e.set_time(0.5);
    e.sync_to_time();
    e.set_prop(Prop::Z, 2800.0);
    assert!(e.stamp_keys(Some((i, &[z])), false));
    assert!(e.move_key(i, 0, z, 0, 0.5, 2800.0 + 100.0));
    let v = e.clip_anim(i, 0).unwrap().track(z).unwrap().keys[0].v;
    assert_eq!(v, 2900.0);
    assert!(e.move_key(i, 0, Target::Shape(Prop::X), 0, 0.0, -500.0));
    let x = e
        .clip_anim(i, 0)
        .unwrap()
        .track(Target::Shape(Prop::X))
        .unwrap()
        .keys[0]
        .v;
    assert_eq!(x, -500.0, "off the canvas is a place");
}

/// Effect parameters fit their declared range on a drag.
#[test]
fn effect_keys_fit_their_range() {
    let (mut e, i) = keyed();
    e.select(Some(i));
    assert!(e.add_effect(crate::fx::EffectKind::Gradient));
    let id = e
        .fx_of(i)
        .find_kind(crate::fx::EffectKind::Gradient)
        .unwrap()
        .id;
    e.set_time(0.0);
    e.sync_to_time();
    e.set_effect_param(i, id, 0, 0.5);
    assert!(e.stamp_key());
    let tg = Target::Effect { id, param: 0 };
    assert!(e.move_key(i, 0, tg, 0, 0.0, 99.0));
    let v = e.clip_anim(i, 0).unwrap().track(tg).unwrap().keys[0].v;
    assert_eq!(v, 1.0, "clamped to the channel's ceiling");
}
