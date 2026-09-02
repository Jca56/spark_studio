//! The object/clip evaluation cycle and the stamp, held by tests: absorb
//! keeps hand edits, restore keeps curves off the document, apply poses
//! at clip-local looped time, and `K` stamps diffs into the active clip.

use super::*;
use crate::anim::Target;
use crate::editor::Editor;
use crate::props::{Prop, Tool};

/// Draw a circle at the playhead and return its index.
fn draw_at(e: &mut Editor, t: f32, center: [f32; 2]) -> usize {
    e.set_time(t);
    e.sync_to_time();
    e.choose_tool(Tool::Circle);
    e.set_cursor_canvas(center);
    e.mouse_down(false);
    e.set_cursor_canvas([center[0] + 80.0, center[1]]);
    e.mouse_up();
    e.choose_tool(Tool::Select);
    e.primary().expect("drawn")
}

/// Draw a line from `a` to `b` at the playhead and return its index.
fn draw_line_at(e: &mut Editor, t: f32, a: [f32; 2], b: [f32; 2]) -> usize {
    e.set_time(t);
    e.sync_to_time();
    e.choose_tool(Tool::Line);
    e.set_cursor_canvas(a);
    e.mouse_down(false);
    e.set_cursor_canvas(b);
    e.mouse_up();
    e.choose_tool(Tool::Select);
    e.primary().expect("drawn")
}

/// A line keys by its ends, never its centre (Alva, 2026-09-01: "Line
/// shapes only move around their middle point, I can't move just one
/// end around while the other stays in one spot"). The first pose is
/// X1·Y1·X2·Y2; dragging one end keys that end alone; and between the
/// keys the other end holds exactly where it was — the laser pivots on
/// the speaker. Moving the whole line by its X field keys both ends'
/// X, not X.
#[test]
fn a_line_keys_by_its_ends_and_one_end_can_hold() {
    let mut e = Editor::empty();
    let i = draw_line_at(&mut e, 0.0, [100.0, 100.0], [500.0, 100.0]);
    assert!(e.stamp_key());
    let ends = [Prop::X1, Prop::Y1, Prop::X2, Prop::Y2].map(Target::Shape);
    assert_eq!(e.obj_clips(i)[0].anim.targets(), ends, "the first pose is the ends");
    // Swing the far end down; the near end stays on the speaker.
    e.set_time(1.0);
    e.sync_to_time();
    assert!(e.drag_line_end(1, [500.0, 500.0]));
    assert_eq!(e.shapes()[i].line_ends(), ([100.0, 100.0], [500.0, 500.0]));
    assert!(e.stamp_key());
    let anim = &e.obj_clips(i)[0].anim;
    assert_eq!(anim.track(Target::Shape(Prop::Y2)).unwrap().keys.len(), 2, "Y2 moved");
    assert_eq!(anim.track(Target::Shape(Prop::X2)).unwrap().keys.len(), 1, "X2 did not");
    assert_eq!(anim.track(Target::Shape(Prop::X1)).unwrap().keys.len(), 1, "the near end held");
    for p in [Prop::X, Prop::Y, Prop::Rotation, Prop::Scale] {
        assert!(anim.track(Target::Shape(p)).is_none(), "{p:?} keyed on a line");
    }
    // Mid-swing: the near end is exactly where it was, the far end is
    // on its way — the line has pivoted, not slid.
    e.set_time(0.5);
    e.sync_to_time();
    let (a, b) = e.shapes()[i].line_ends();
    assert_eq!(a, [100.0, 100.0], "the pivot end drifted");
    assert!(b[1] > 120.0 && b[1] < 480.0, "the far end is mid-swing, got {b:?}");
    assert!((b[0] - 500.0).abs() < 1e-3);
    // Move the whole line by its X field, and stamp: both ends' X key.
    e.set_time(1.5);
    e.sync_to_time();
    assert!(e.set_prop(Prop::X, 400.0));
    assert!(e.stamp_key());
    let anim = &e.obj_clips(i)[0].anim;
    assert_eq!(anim.track(Target::Shape(Prop::X1)).unwrap().keys.len(), 2);
    assert_eq!(anim.track(Target::Shape(Prop::X2)).unwrap().keys.len(), 2);
    assert!(anim.track(Target::Shape(Prop::X)).is_none());
}

/// The clip view's `K` keys the shown setting and nothing else — not
/// what moved with it, not what is already keyed (Alva, 2026-09-01:
/// "it keeps making keyframes in other settings and makes a mess"). A
/// line's end dragged sideways moves X2 and Y2 both; with Y2 shown,
/// only Y2 lands. A setting the object can't key is skipped; no clip
/// under the playhead, nothing lands.
#[test]
fn the_views_stamp_keys_the_shown_setting_alone() {
    let mut e = Editor::empty();
    let i = draw_line_at(&mut e, 0.0, [100.0, 100.0], [500.0, 100.0]);
    let (x2, y2) = (Target::Shape(Prop::X2), Target::Shape(Prop::Y2));
    assert!(e.stamp_only(i, &[y2]));
    assert_eq!(e.obj_clips(i)[0].anim.targets(), vec![y2], "Y2 alone, unmoved");
    e.set_time(1.0);
    e.sync_to_time();
    assert!(e.drag_line_end(1, [700.0, 400.0]));
    assert!(e.stamp_only(i, &[y2]));
    let anim = &e.obj_clips(i)[0].anim;
    assert_eq!(anim.targets(), vec![y2], "X2 moved too and stayed off the curves");
    assert_eq!(anim.track(y2).unwrap().keys.len(), 2);
    // A centre prop on a line can't be keyed: skipped, nothing lands.
    assert!(!e.stamp_only(i, &[Target::Shape(Prop::X)]));
    assert!(e.obj_clips(i)[0].anim.track(x2).is_none());
    // Off the clip, nowhere to land.
    e.set_time(40.0);
    e.sync_to_time();
    assert!(!e.stamp_only(i, &[y2]));
}

/// The clip view's stamp: a listed setting is keyed whether or not it
/// moved, one key and no hold behind it; nothing is volunteered — no
/// first pose on an unkeyed clip — and the arrangement's rule is
/// untouched.
#[test]
fn a_listed_setting_stamps_and_nothing_is_volunteered() {
    let mut e = Editor::empty();
    let i = draw_at(&mut e, 0.0, [300.0, 300.0]);
    let z = Target::Shape(Prop::Z);
    // In the view with nothing listed: K keys nothing at all.
    assert!(!e.stamp_keys(Some((i, &[])), false));
    assert!(!e.obj_clips(i)[0].anim.has_keys(), "no first pose in the view");
    // Z listed, untouched: one key at its value, nothing else.
    e.set_time(1.0);
    e.sync_to_time();
    assert!(e.stamp_keys(Some((i, &[z])), false));
    let anim = &e.obj_clips(i)[0].anim;
    assert_eq!(anim.targets(), vec![z]);
    assert_eq!(anim.track(z).unwrap().keys.len(), 1, "no flat pair behind it");
    // Move it later and stamp again: the ramp.
    e.set_time(1.5);
    e.sync_to_time();
    e.set_prop(Prop::Z, -400.0);
    assert!(e.stamp_keys(Some((i, &[z])), false));
    assert_eq!(e.obj_clips(i)[0].anim.track(z).unwrap().keys.len(), 2);
    // The arrangement's K on a fresh object still lays the pose anchor.
    let j = draw_at(&mut e, 0.0, [600.0, 300.0]);
    assert!(e.stamp_key());
    assert_eq!(e.obj_clips(j)[0].anim.targets().len(), 4, "X Y Rot S");
}

/// The K-at-two-moments loop, the app's heartbeat: pose, stamp, move the
/// playhead, pose again, stamp again — and the motion plays between them
/// at clip-local time.
#[test]
fn stamp_twice_and_the_motion_plays_inside_the_clip() {
    let mut e = Editor::empty();
    let i = draw_at(&mut e, 4.0, [300.0, 300.0]);
    // First K: the pose lands at clip-local 0 (playhead == clip start).
    assert!(e.stamp_key());
    // Move the playhead half a bar in, drag the circle, stamp.
    e.set_time(5.0);
    e.sync_to_time();
    e.set_prop(Prop::X, 900.0);
    assert!(e.stamp_key());
    // The keys live in the clip, in local time.
    let anim = &e.obj_clips(i)[0].anim;
    let track = anim.track(Target::Shape(Prop::X)).expect("an X track");
    assert_eq!(track.keys.len(), 2);
    assert!((track.keys[0].t - 0.0).abs() < 1e-4, "local zero");
    assert!((track.keys[1].t - 1.0).abs() < 1e-4, "local one second");
    // Halfway between, the pose is between (smooth ease: just not at
    // either end).
    e.set_time(4.5);
    e.sync_to_time();
    let x = e.shapes()[i].center()[0];
    assert!(x > 320.0 && x < 880.0, "mid-move, got {x}");
    // And the document base never moved: a save at any playhead writes
    // the same bytes.
    e.set_time(5.0);
    e.sync_to_time();
    let d1 = crate::doc::serialize(&e.to_doc());
    e.set_time(4.25);
    e.sync_to_time();
    let d2 = crate::doc::serialize(&e.to_doc());
    assert_eq!(d1, d2, "playback wrote into the document");
}

/// A looping clip replays its motion every loop length; stretching the
/// clip does not stretch the motion.
#[test]
fn a_looping_clip_replays_its_bar() {
    let mut e = Editor::empty();
    let i = draw_at(&mut e, 0.0, [300.0, 300.0]);
    assert!(e.stamp_key());
    e.set_time(1.0);
    e.sync_to_time();
    e.set_prop(Prop::X, 700.0);
    assert!(e.stamp_key());
    // Stretch the clip to four bars — the whole-clip loop follows the
    // edge — then shorten the loop back to one bar: a repeater.
    assert!(e.set_obj_clip_span(i, 0, 0.0, e.bar_s * 4.0));
    assert!((e.obj_clips(i)[0].loop_len - e.bar_s * 4.0).abs() < 1e-4);
    assert!(e.set_obj_clip_loop_len(i, 0, e.bar_s));
    e.set_time(0.5);
    e.sync_to_time();
    let first_pass = e.shapes()[i].center()[0];
    e.set_time(0.5 + e.bar_s);
    e.sync_to_time();
    let second_pass = e.shapes()[i].center()[0];
    assert!(
        (first_pass - second_pass).abs() < 1e-3,
        "the second bar replays the first: {first_pass} vs {second_pass}"
    );
    // Loop off: the motion plays once and holds its last pose.
    assert!(e.toggle_obj_clip_loop(i, 0));
    e.set_time(0.5 + e.bar_s * 2.0);
    e.sync_to_time();
    assert!(
        (e.shapes()[i].center()[0] - 700.0).abs() < 1e-3,
        "held the last key, got {}",
        e.shapes()[i].center()[0]
    );
}

/// No clip under the playhead: the object is absent — not drawn, not
/// pickable — and `K` has nowhere to stamp.
#[test]
fn between_clips_an_object_is_absent() {
    let mut e = Editor::empty();
    let i = draw_at(&mut e, 0.0, [300.0, 300.0]);
    assert!(e.exists_now(i));
    e.set_time(e.bar_s + 1.0);
    e.sync_to_time();
    assert!(!e.exists_now(i));
    // Not pickable where it isn't.
    e.set_cursor_canvas([300.0, 300.0]);
    assert!(!e.hit_at_cursor(), "picked an absent object");
    // Not drawn: its display slot is a speck off-canvas.
    let shapes = e.display_shapes(None);
    assert!(shapes[i].center()[0] < -1e4, "an absent object drew");
    // And a stamp lands nothing.
    assert!(!e.stamp_key());
}

/// The absorb half of the cycle: a hand edit on an *untracked* property
/// reaches the document truth and survives the playhead moving; a hand
/// pose on a *tracked* one is a preview that reverts unstamped.
#[test]
fn hand_edits_absorb_and_previews_revert() {
    let mut e = Editor::empty();
    let i = draw_at(&mut e, 0.0, [300.0, 300.0]);
    assert!(e.stamp_key());
    e.set_time(1.0);
    e.sync_to_time();
    e.set_prop(Prop::X, 700.0);
    assert!(e.stamp_key());
    // Brightness is untracked: nudging it is a plain edit.
    e.set_time(0.5);
    e.sync_to_time();
    e.set_prop(Prop::Brightness, 2.5);
    e.sync_to_time();
    e.set_time(1.0);
    e.sync_to_time();
    assert!(
        (e.shapes()[i].brightness() - 2.5).abs() < 1e-3,
        "the brightness edit was lost, got {}",
        e.shapes()[i].brightness()
    );
    // X is tracked: posing it without stamping is a preview...
    e.set_time(0.5);
    e.sync_to_time();
    let curve_x = e.shapes()[i].center()[0];
    e.set_prop(Prop::X, 111.0);
    e.sync_to_time();
    assert!(
        (e.shapes()[i].center()[0] - 111.0).abs() < 1e-3,
        "the preview should hold while the playhead parks"
    );
    // ...that reverts when the playhead moves.
    e.set_time(0.51);
    e.sync_to_time();
    let back = e.shapes()[i].center()[0];
    assert!(
        (back - 111.0).abs() > 100.0,
        "the unstamped pose stuck: {back}"
    );
    assert!(
        (back - curve_x).abs() < 40.0,
        "the curve should be back in charge: {back} vs {curve_x}"
    );
}

/// The first stamp poses; the second keys exactly what moved; a stamp
/// with nothing moved holds. The three-case rule, now per clip.
#[test]
fn the_stamp_keys_the_diff() {
    let mut e = Editor::empty();
    let i = draw_at(&mut e, 0.0, [300.0, 300.0]);
    assert!(e.stamp_key());
    let first: Vec<Target> = e.obj_clips(i)[0].anim.targets();
    assert!(
        first.contains(&Target::Shape(Prop::X)) && first.contains(&Target::Shape(Prop::Rotation)),
        "the first pose lays down the pose props"
    );
    assert!(
        !first.contains(&Target::Shape(Prop::Glow)),
        "the first pose is a pose, not a freeze"
    );
    // Rotate only; the second stamp keys rotation alone.
    e.set_time(1.0);
    e.sync_to_time();
    e.set_prop(Prop::Rotation, 1.0);
    assert!(e.stamp_key());
    let rot = e.obj_clips(i)[0]
        .anim
        .track(Target::Shape(Prop::Rotation))
        .unwrap();
    assert_eq!(rot.keys.len(), 2, "rotation earned its second key");
    let x = e.obj_clips(i)[0].anim.track(Target::Shape(Prop::X)).unwrap();
    assert_eq!(x.keys.len(), 1, "x did not move and was not re-keyed");
    // Nothing moved: a hold re-stamps what is animated. (1.5, not 2.0 —
    // a clip's end is exclusive, and the clip is one 2s bar.)
    e.set_time(1.5);
    e.sync_to_time();
    assert!(e.stamp_key());
    let rot = e.obj_clips(i)[0]
        .anim
        .track(Target::Shape(Prop::Rotation))
        .unwrap();
    assert_eq!(rot.keys.len(), 3, "the hold stamped stillness");
}

/// Trimming a clip's left edge eats content: the motion keeps its grid
/// position instead of sliding with the edge.
#[test]
fn a_left_trim_eats_content() {
    let mut e = Editor::empty();
    let i = draw_at(&mut e, 0.0, [300.0, 300.0]);
    assert!(e.stamp_key());
    e.set_time(1.0);
    e.sync_to_time();
    e.set_prop(Prop::X, 700.0);
    assert!(e.stamp_key());
    e.set_time(0.9);
    e.sync_to_time();
    let before = e.shapes()[i].center()[0];
    // Trim half a second off the front.
    let end = e.obj_clips(i)[0].end();
    assert!(e.set_obj_clip_span(i, 0, 0.5, end - 0.5));
    e.sync_to_time();
    assert!(
        (e.shapes()[i].center()[0] - before).abs() < 1e-3,
        "t=0.9 changed across a left trim: {} vs {before}",
        e.shapes()[i].center()[0]
    );
}

/// Undo restores document truth — base state and clips together.
#[test]
fn undo_covers_base_and_clips() {
    let mut e = Editor::empty();
    let i = draw_at(&mut e, 0.0, [300.0, 300.0]);
    assert!(e.stamp_key());
    let keyed = e.obj_clips(i)[0].anim.tracks.len();
    assert!(keyed > 0);
    e.undo();
    assert_eq!(e.obj_clips(i)[0].anim.tracks.len(), 0, "the stamp undid");
    e.undo();
    assert!(e.shapes().is_empty(), "the draw undid");
    e.redo();
    assert_eq!(e.shapes().len(), 1);
    assert_eq!(e.obj_clips(0).len(), 1, "the clip came back with the object");
}
