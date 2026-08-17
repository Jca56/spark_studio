//! Keyframe tests: retiming without collapsing keys, and clipboard
//! round-trips that keep values and relative timing.

mod retime {
    use super::super::*;
    use crate::props::Prop;
    use spark_render::Shape;

    /// Shorthand: every fixture here keys shape 0.
    const S0: Owner = Owner::Shape(0);

    /// One shape carrying an X track keyed at `times`.
    fn keyed_at(times: &[f32]) -> Editor {
        let mut e = Editor::empty();
        e.shapes.push(Shape::circle([0.0, 0.0], 10.0));
        e.names.push(String::new());
        e.anim.push(ShapeAnim {
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
        });
        e.react.push([1.0; 3]);
        e.group.push(0);
        e.hidden.push(false);
        e.folder.push(0);
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

mod clipboard {
    use super::super::*;
    use crate::props::Prop;
    use spark_render::Shape;

    fn one_keyed() -> Editor {
        let mut e = Editor::empty();
        let mut sh = Shape::circle([100.0, 200.0], 20.0);
        sh.set_center([100.0, 200.0]);
        e.shapes.push(sh);
        e.names.push(String::new());
        e.anim.push(ShapeAnim::default());
        e.react.push([1.0; 3]);
        e.group.push(0);
        e.hidden.push(false);
        e.folder.push(0);
        e.selection = vec![0];
        e.set_time(1.0);
        e.stamp_key();
        e
    }

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
        assert_eq!(key_at(&e, Owner::Shape(0), Prop::X, 1.0), Some(100.0));
        e.copy_keys_multi(&[(Owner::Shape(0), 1.0)]);
        let pasted = e.paste_keys(&[5.0], 60.0).expect("paste landed");
        assert_eq!(pasted, vec![(Owner::Shape(0), 5.0)]);
        // The pasted key must carry the copied value, not an empty stamp.
        assert_eq!(key_at(&e, Owner::Shape(0), Prop::X, 5.0), Some(100.0));
        assert_eq!(key_at(&e, Owner::Shape(0), Prop::Y, 5.0), Some(200.0));
    }

    #[test]
    fn paste_keeps_relative_timing_across_a_group() {
        let mut e = one_keyed();
        e.set_time(2.0);
        e.shapes[0].set_center([400.0, 200.0]);
        e.stamp_key();
        e.copy_keys_multi(&[(Owner::Shape(0), 1.0), (Owner::Shape(0), 2.0)]);
        e.paste_keys(&[10.0], 60.0).expect("paste landed");
        // 1s apart at the source, 1s apart at the destination.
        assert_eq!(key_at(&e, Owner::Shape(0), Prop::X, 10.0), Some(100.0));
        assert_eq!(key_at(&e, Owner::Shape(0), Prop::X, 11.0), Some(400.0));
    }

    #[test]
    fn paste_past_the_track_end_is_dropped() {
        let mut e = one_keyed();
        e.copy_keys_multi(&[(Owner::Shape(0), 1.0)]);
        assert!(e.paste_keys(&[99.0], 60.0).is_none());
    }
}
