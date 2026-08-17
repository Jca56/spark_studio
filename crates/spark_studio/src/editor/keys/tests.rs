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
