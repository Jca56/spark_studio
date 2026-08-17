//! Keyframe tests: retiming without collapsing keys, and clipboard
//! round-trips that keep values and relative timing.

mod retime {
    use super::super::*;
    use crate::props::Prop;
    use spark_render::Shape;

    /// Shorthand: every fixture here builds exactly one shape, so it carries
    /// the first id handed out.
    const S0: Owner = Owner::Shape(crate::editor::FIRST_SHAPE_ID);

    /// One shape carrying an X track keyed at `times`.
    fn keyed_at(times: &[f32]) -> Editor {
        let mut e = Editor::empty();
        let i = e.push_shape(Shape::circle([0.0, 0.0], 10.0));
        e.anim[i] = ShapeAnim {
            tracks: vec![Track {
                prop: Prop::X,
                keys: times
                    .iter()
                    .map(|&t| Key {
                        t,
                        v: t * 100.0,
                        ease: Ease::Smooth,
                    })
                    .collect(),
            }],
        };
        e
    }

    fn times(e: &Editor) -> Vec<f32> {
        e.anim[0].tracks[0].keys.iter().map(|k| k.t).collect()
    }

    #[test]
    fn retime_group_slides_each_key_once() {
        // Two keys a 16th apart dragged by exactly one 16th: the leading key
        // must not be caught a second time by the trailing key's pass.
        let mut e = keyed_at(&[1.0, 1.25]);
        assert!(e.retime_group(&[(S0, 1.0), (S0, 1.25)], 0.25));
        assert_eq!(times(&e), vec![1.25, 1.5]);
    }

    #[test]
    fn retime_group_refuses_collision_outside_the_set() {
        // 1.5 isn't moving, so sliding 1.0 onto it would silently merge them.
        let mut e = keyed_at(&[1.0, 1.5]);
        assert!(!e.retime_group(&[(S0, 1.0)], 0.5));
        assert_eq!(times(&e), vec![1.0, 1.5]);
    }

    #[test]
    fn retime_group_keeps_keys_sorted() {
        let mut e = keyed_at(&[1.0, 2.0, 3.0]);
        // Slide the earliest key past the other two.
        assert!(e.retime_group(&[(S0, 1.0)], 2.5));
        assert_eq!(times(&e), vec![2.0, 3.0, 3.5]);
    }
}

/// Owners name shapes by identity. A lane, a key selection and the keyframe
/// clipboard all outlive the frame they were made in, and stack indices do
/// not survive a reorder or a delete.
mod identity {
    use super::super::*;
    use crate::anim::KEY_EPS;
    use crate::props::Prop;
    use spark_render::Shape;

    /// Two shapes, the lower one keyed at t=1 so the two are told apart by
    /// whether they carry keys at all.
    fn two(keyed: usize) -> Editor {
        let mut e = Editor::empty();
        e.push_shape(Shape::circle([10.0, 0.0], 10.0));
        e.push_shape(Shape::circle([20.0, 0.0], 10.0));
        e.selection = vec![keyed];
        e.set_time(1.0);
        e.stamp_key();
        e.selection.clear();
        e
    }

    #[test]
    fn a_reorder_does_not_repoint_a_key_owner() {
        // The bug: a lane owner held a stack index, so dragging a layer past
        // another moved that lane's keys onto a different shape.
        let mut e = two(0);
        let id = e.shape_id(0);
        let keyed = Owner::Shape(id);
        assert!(e.owner_anim(keyed).is_some_and(|a| a.has_keys()));
        assert!(e.move_layer(0, 1), "drag the keyed shape up the stack");
        assert_eq!(e.index_of(id), Some(1), "it moved");
        assert!(
            e.owner_anim(keyed).is_some_and(|a| a.has_keys()),
            "the owner still names the shape it was made for"
        );
        // ...and the shape that slid down did not inherit its keys.
        assert!(!e.owner_anim(e.owner(0)).is_some_and(|a| a.has_keys()));
    }

    #[test]
    fn keys_of_a_deleted_shape_are_unreachable_not_misdirected() {
        // Deleting the keyed shape must leave its owner resolving to nothing
        // — never to whatever shape took its slot.
        let mut e = two(0);
        let gone = e.owner(0);
        e.selection = vec![0];
        assert!(e.delete_selected());
        assert!(
            e.owner_anim(gone).is_none(),
            "a dead owner resolves to nothing"
        );
        // Operations on the stale owner are refused rather than misapplied.
        assert!(!e.delete_keys_at(gone, 1.0));
        assert!(!e.retime_group(&[(gone, 1.0)], 0.5));
    }

    #[test]
    fn the_key_clipboard_survives_a_reorder() {
        // It used to be thrown away on every reorder, because it held stack
        // indices. Identity means a copy outlives rearranging the stack.
        let mut e = two(0);
        let src = e.owner(0);
        e.copy_keys_multi(&[(src, 1.0)]);
        assert!(e.move_layer(0, 1));
        assert!(e.has_key_clip(), "the copy is still good");
        let pasted = e.paste_keys(&[5.0], 60.0).expect("paste landed");
        assert_eq!(pasted, vec![(src, 5.0)], "onto the shape it came from");
        let v = e
            .owner_anim(src)
            .and_then(|a| a.tracks.iter().find(|tr| tr.prop == Prop::X))
            .and_then(|tr| tr.keys.iter().find(|k| (k.t - 5.0).abs() < KEY_EPS))
            .map(|k| k.v);
        assert_eq!(v, Some(10.0), "carrying the value it was copied with");
    }
}

mod clipboard {
    use super::super::*;
    use crate::anim::KEY_EPS;
    use crate::props::Prop;
    use spark_render::Shape;

    fn one_keyed() -> Editor {
        let mut e = Editor::empty();
        let mut sh = Shape::circle([100.0, 200.0], 20.0);
        sh.set_center([100.0, 200.0]);
        e.push_shape(sh);
        e.selection = vec![0];
        e.set_time(1.0);
        e.stamp_key();
        e
    }

    /// The one shape every fixture here builds carries the first id.
    const S0: Owner = Owner::Shape(crate::editor::FIRST_SHAPE_ID);

    fn key_at(e: &Editor, o: Owner, prop: Prop, t: f32) -> Option<f32> {
        e.owner_anim(o)?
            .tracks
            .iter()
            .find(|tr| tr.prop == prop)?
            .keys
            .iter()
            .find(|k| (k.t - t).abs() < KEY_EPS)
            .map(|k| k.v)
    }

    #[test]
    fn copied_keys_paste_with_their_values() {
        let mut e = one_keyed();
        assert_eq!(key_at(&e, S0, Prop::X, 1.0), Some(100.0));
        e.copy_keys_multi(&[(S0, 1.0)]);
        let pasted = e.paste_keys(&[5.0], 60.0).expect("paste landed");
        assert_eq!(pasted, vec![(S0, 5.0)]);
        // The pasted key must carry the copied value, not an empty stamp.
        assert_eq!(key_at(&e, S0, Prop::X, 5.0), Some(100.0));
        assert_eq!(key_at(&e, S0, Prop::Y, 5.0), Some(200.0));
    }

    #[test]
    fn paste_keeps_relative_timing_across_a_group() {
        let mut e = one_keyed();
        e.set_time(2.0);
        e.shapes[0].set_center([400.0, 200.0]);
        e.stamp_key();
        e.copy_keys_multi(&[(S0, 1.0), (S0, 2.0)]);
        e.paste_keys(&[10.0], 60.0).expect("paste landed");
        // 1s apart at the source, 1s apart at the destination.
        assert_eq!(key_at(&e, S0, Prop::X, 10.0), Some(100.0));
        assert_eq!(key_at(&e, S0, Prop::X, 11.0), Some(400.0));
    }

    #[test]
    fn paste_past_the_track_end_is_dropped() {
        let mut e = one_keyed();
        e.copy_keys_multi(&[(S0, 1.0)]);
        assert!(e.paste_keys(&[99.0], 60.0).is_none());
    }
}

/// Stamping keys what the hand changed, not the whole shape. One `K` used
/// to write every applicable property, which meant a single keyframe froze
/// the shape forever: from then on the curves drove glow, sides, thickness
/// and the rest too, so posing by hand could only ever preview.
mod stamp_what_changed {
    use super::super::*;
    use crate::props::Prop;
    use spark_render::Shape;

    /// A circle with `K` already pressed once at t=0.
    fn posed() -> Editor {
        let mut e = Editor::empty();
        e.push_shape(Shape::circle([100.0, 100.0], 40.0).stroke(4.0));
        e.selection = vec![0];
        e.set_time(0.0);
        e.sync_to_time();
        e.stamp_key();
        e
    }

    fn keyed(e: &Editor) -> Vec<Prop> {
        let mut v: Vec<Prop> = e.anim[0].tracks.iter().map(|t| t.prop).collect();
        v.sort_by_key(|p| anim::PROP_ORDER.iter().position(|q| q == p).unwrap_or(99));
        v
    }

    fn times_of(e: &Editor, prop: Prop) -> Vec<f32> {
        e.anim[0]
            .tracks
            .iter()
            .find(|t| t.prop == prop)
            .map(|t| t.keys.iter().map(|k| k.t).collect())
            .unwrap_or_default()
    }

    /// The first stamp lays down a pose — where it is, how it's turned, how
    /// big — and nothing else. Glow and thickness are not part of "where
    /// this shape is", so they stay free to be edited by hand afterwards.
    #[test]
    fn the_first_stamp_is_just_a_pose() {
        let e = posed();
        assert_eq!(
            keyed(&e),
            vec![Prop::X, Prop::Y, Prop::Rotation, Prop::Scale]
        );
    }

    /// Move it and stamp: only the axis that moved earns a key. Y, rotation
    /// and scale keep the two-key shape they had, untouched.
    #[test]
    fn only_the_moved_property_is_keyed() {
        let mut e = posed();
        e.set_time(4.0);
        e.sync_to_time();
        e.shapes[0].set_center([500.0, 100.0]);
        e.mark_posed(&[0]);
        e.stamp_key();
        assert_eq!(times_of(&e, Prop::X), vec![0.0, 4.0], "X moved");
        assert_eq!(times_of(&e, Prop::Y), vec![0.0], "Y did not");
        assert_eq!(times_of(&e, Prop::Rotation), vec![0.0]);
        assert_eq!(times_of(&e, Prop::Scale), vec![0.0]);
    }

    /// A keyed shape can still be restyled. Glow was never keyed, so it is
    /// an ordinary editable value — and stamping X must not quietly capture
    /// it into a curve.
    #[test]
    fn stamping_does_not_capture_untouched_look() {
        let mut e = posed();
        e.set_time(4.0);
        e.sync_to_time();
        e.shapes[0].set_center([500.0, 100.0]);
        e.mark_posed(&[0]);
        e.stamp_key();
        assert!(
            !keyed(&e).contains(&Prop::Glow),
            "moving the shape keyed its glow"
        );
        // And the hand-set glow survives being posed at the playhead.
        e.shapes[0].set_glow(30.0);
        e.set_time(2.0);
        e.sync_to_time();
        assert_eq!(e.shapes[0].glow_radius(), 30.0, "the curve overwrote glow");
    }

    /// The backfill: a property earning its first key needs something to
    /// move *from*, or the change is a flat line. Turning the glow up at
    /// bar 5 must ramp from where it was at the previous key, not jump.
    #[test]
    fn a_new_property_ramps_from_the_last_key() {
        let mut e = posed();
        e.set_time(4.0);
        e.sync_to_time();
        e.shapes[0].set_glow(60.0);
        e.mark_posed(&[0]);
        e.stamp_key();
        assert_eq!(
            times_of(&e, Prop::Glow),
            vec![0.0, 4.0],
            "glow got a holding key at the previous stamp"
        );
        let track = e.anim[0]
            .tracks
            .iter()
            .find(|t| t.prop == Prop::Glow)
            .unwrap();
        assert_eq!(track.keys[0].v, 0.0, "held at the glow it had before");
        assert_eq!(track.keys[1].v, 60.0, "and arrives at the new one");
        // Which means it actually moves in between.
        let mid = track.sample(2.0).unwrap();
        assert!(mid > 0.0 && mid < 60.0, "glow sat flat at {mid}");
    }

    /// With no earlier key to anchor to there is nothing to ramp from, and
    /// inventing one would be a lie about where the shape was.
    #[test]
    fn a_new_property_before_every_key_gets_no_backfill() {
        let mut e = posed();
        e.set_time(0.0);
        e.sync_to_time();
        e.shapes[0].set_glow(60.0);
        e.mark_posed(&[0]);
        e.stamp_key();
        assert_eq!(times_of(&e, Prop::Glow), vec![0.0]);
    }

    /// Nothing moved: hold. Pressing `K` at a second moment without
    /// touching anything is how you ask a shape to sit still, so every
    /// track it already has gets a key at its current value.
    #[test]
    fn an_unchanged_stamp_holds_what_is_animated() {
        let mut e = posed();
        e.set_time(4.0);
        e.sync_to_time();
        e.stamp_key();
        for prop in [Prop::X, Prop::Y, Prop::Rotation, Prop::Scale] {
            assert_eq!(times_of(&e, prop), vec![0.0, 4.0], "{prop:?} did not hold");
        }
        // Still nothing it wasn't already animating.
        assert!(!keyed(&e).contains(&Prop::Glow));
        // And the shape genuinely doesn't move across the held span.
        let x = e.anim[0].tracks.iter().find(|t| t.prop == Prop::X).unwrap();
        assert_eq!(x.sample(2.0), Some(100.0));
    }

    /// Stretching one axis moves `Scale` too when it is the longer one,
    /// because both read the same extents — the diff has to catch the pair
    /// or the flat Scale curve would squash the stretch back on playback.
    #[test]
    fn stretching_keys_the_extent_and_the_scale_together() {
        let mut e = posed();
        e.set_time(4.0);
        e.sync_to_time();
        e.shapes[0].set_box_width(400.0);
        e.mark_posed(&[0]);
        e.stamp_key();
        let k = keyed(&e);
        assert!(k.contains(&Prop::Width), "the stretched axis");
        assert!(k.contains(&Prop::Scale), "and the size it derives from");
        // Playback lands on the width that was stamped, not back at the old
        // one — the pair has to agree at t.
        let mut shape = e.shapes[0];
        e.anim[0].apply(&mut shape, 4.0);
        assert!(
            (shape.box_size().unwrap()[0] - 400.0).abs() < 1.0,
            "played back at {:?}",
            shape.box_size()
        );
    }
}

/// The chain from a real edit through to the stamp. The tests above mark
/// the shape posed by hand; this one goes through the editor's own API and
/// past a redraw, which is where a path that forgot to mark it posed would
/// lose the baseline and silently stamp a hold instead of the edit.
#[cfg(test)]
mod end_to_end {
    use super::super::*;
    use crate::props::Prop;
    use spark_render::Shape;

    #[test]
    fn an_edit_survives_the_redraw_between_it_and_the_stamp() {
        let mut e = Editor::empty();
        e.push_shape(Shape::circle([100.0, 100.0], 40.0));
        e.selection = vec![0];
        e.set_time(0.0);
        e.sync_to_time();
        e.stamp_key();

        e.set_time(4.0);
        e.sync_to_time();
        e.set_prop(Prop::Glow, 60.0);
        // The frame that lands between the edit and pressing K.
        e.sync_to_time();
        e.stamp_key();

        let glow = e.anim[0].tracks.iter().find(|t| t.prop == Prop::Glow);
        let times: Vec<f32> = glow
            .map(|t| t.keys.iter().map(|k| k.t).collect())
            .unwrap_or_default();
        assert_eq!(times, vec![0.0, 4.0], "the glow edit was not detected");
        assert_eq!(glow.unwrap().keys[1].v, 60.0);
    }
}
