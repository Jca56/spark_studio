//! Folder model tests: the stack invariant, the eye gate, reordering,
//! and the transform parent's composition math.

pub(super) mod tests_support {
    use super::super::*;
    use spark_render::Shape;

    pub(crate) fn stack(n: usize) -> Editor {
        let mut e = Editor::empty();
        for k in 0..n {
            e.shapes.push(Shape::circle([k as f32 * 10.0, 0.0], 10.0));
            e.names.push(format!("s{k}"));
            e.anim.push(crate::anim::ShapeAnim::default());
            e.react.push([1.0; 3]);
            e.group.push(0);
            e.hidden.push(false);
            e.folder.push(0);
        }
        e
    }

    pub(crate) fn names(e: &Editor) -> Vec<&str> {
        e.names.iter().map(String::as_str).collect()
    }

    #[test]
    fn foldering_pulls_members_contiguous() {
        // Scattered picks must end up as one run, anchored at the lowest.
        let mut e = stack(6);
        e.selection = vec![1, 3, 5];
        assert!(e.new_folder_from_selection());
        assert_eq!(names(&e), vec!["s0", "s1", "s3", "s5", "s2", "s4"]);
        assert_eq!(e.folder_members(1), vec![1, 2, 3]);
    }

    #[test]
    fn foldering_remaps_the_selection() {
        let mut e = stack(6);
        e.selection = vec![1, 3, 5];
        e.new_folder_from_selection();
        // Same shapes, new indices — not the stale 1/3/5.
        let mut sel = e.selection.clone();
        sel.sort_unstable();
        assert_eq!(sel, vec![1, 2, 3]);
    }

    #[test]
    fn contiguous_folders_are_left_alone() {
        let mut e = stack(4);
        e.selection = vec![1, 2];
        e.new_folder_from_selection();
        assert_eq!(names(&e), vec!["s0", "s1", "s2", "s3"]);
    }

    #[test]
    fn folder_hidden_gates_its_members() {
        let mut e = stack(3);
        e.selection = vec![0, 1];
        e.new_folder_from_selection();
        assert!(!e.shape_hidden(0));
        e.toggle_folder_hidden(1);
        assert!(e.shape_hidden(0) && e.shape_hidden(1));
        assert!(!e.shape_hidden(2), "loose layers are unaffected");
        // The member's own eye is untouched, so expanding restores it.
        e.toggle_folder_hidden(1);
        assert!(!e.shape_hidden(0));
    }

    #[test]
    fn dissolving_keeps_the_shapes() {
        let mut e = stack(4);
        e.selection = vec![0, 2];
        e.new_folder_from_selection();
        assert!(e.dissolve_folder(1));
        assert_eq!(e.shapes.len(), 4);
        assert!(e.folders.is_empty());
        assert!(e.folder.iter().all(|&f| f == 0));
    }

    #[test]
    fn emptying_a_folder_drops_it() {
        let mut e = stack(3);
        e.selection = vec![0];
        e.new_folder_from_selection();
        assert_eq!(e.folders.len(), 1);
        // Pull the only member back out.
        assert!(e.set_shape_folder(0, 0));
        assert!(e.folders.is_empty(), "an empty folder shouldn't linger");
    }

    #[test]
    fn moving_a_layer_in_joins_the_run() {
        let mut e = stack(5);
        e.selection = vec![0, 1];
        e.new_folder_from_selection();
        // s4 joins the folder and gets pulled down beside its members.
        assert!(e.set_shape_folder(4, 1));
        assert_eq!(names(&e), vec!["s0", "s1", "s4", "s2", "s3"]);
        assert_eq!(e.folder_members(1), vec![0, 1, 2]);
    }
}

mod reorder_tests {
    use super::tests_support::*;

    #[test]
    fn reordering_carries_the_folder_with_the_shape() {
        // The folder id has to travel with its shape, or the arrays desync
        // and layers silently change folders when you drag the list.
        let mut e = stack(5);
        e.selection = vec![3, 4];
        e.new_folder_from_selection();
        let before: Vec<_> = e
            .names
            .iter()
            .zip(&e.folder)
            .map(|(n, f)| (n.clone(), *f))
            .collect();
        e.move_layer(0, 2);
        let after: Vec<_> = e
            .names
            .iter()
            .zip(&e.folder)
            .map(|(n, f)| (n.clone(), *f))
            .collect();
        for (name, folder) in &before {
            let got = after.iter().find(|(n, _)| n == name).map(|(_, f)| *f);
            assert_eq!(got, Some(*folder), "{name} changed folder on reorder");
        }
    }

    #[test]
    fn reordering_into_a_folder_run_is_pulled_back_out() {
        let mut e = stack(5);
        e.selection = vec![3, 4];
        e.new_folder_from_selection();
        // Drop a loose layer into the middle of the folder's run.
        e.move_layer(0, 4);
        // It must not have joined the folder, and the run stays whole.
        let members = e.folder_members(1);
        assert_eq!(members.len(), 2);
        assert_eq!(
            members[1],
            members[0] + 1,
            "folder run must stay contiguous"
        );
    }
}

mod transform_tests {
    use super::tests_support::*;
    use crate::props::Prop;

    /// Two shapes at x=0 and x=100, y=0 — pivot lands at (50, 0).
    fn pair() -> crate::editor::Editor {
        let mut e = stack(2);
        e.shapes[0].set_center([0.0, 0.0]);
        e.shapes[1].set_center([100.0, 0.0]);
        e.selection = vec![0, 1];
        e.new_folder_from_selection();
        e
    }

    #[test]
    fn identity_folder_changes_nothing() {
        let e = pair();
        for i in 0..2 {
            let before = e.shapes[i];
            let after = e.posed_shape(i, before);
            assert_eq!(after.center(), before.center());
            assert_eq!(after.size(), before.size());
        }
    }

    #[test]
    fn folder_offset_moves_every_member() {
        let mut e = pair();
        e.set_folder_prop(1, Prop::X, 30.0);
        e.set_folder_prop(1, Prop::Y, -10.0);
        assert_eq!(e.posed_shape(0, e.shapes[0]).center(), [30.0, -10.0]);
        assert_eq!(e.posed_shape(1, e.shapes[1]).center(), [130.0, -10.0]);
    }

    #[test]
    fn folder_scale_works_about_the_pivot() {
        let mut e = pair();
        assert_eq!(e.folder_pivot(1), [50.0, 0.0]);
        e.set_folder_prop(1, Prop::Scale, 2.0);
        // Members spread from the pivot, and each grows.
        assert_eq!(e.posed_shape(0, e.shapes[0]).center(), [-50.0, 0.0]);
        assert_eq!(e.posed_shape(1, e.shapes[1]).center(), [150.0, 0.0]);
        assert!((e.posed_shape(0, e.shapes[0]).size() - e.shapes[0].size() * 2.0).abs() < 0.01);
    }

    #[test]
    fn folder_rotation_orbits_the_pivot() {
        let mut e = pair();
        e.set_folder_prop(1, Prop::Rotation, std::f32::consts::PI);
        // A half turn about (50,0) swaps the two members' positions.
        let a = e.posed_shape(0, e.shapes[0]).center();
        let b = e.posed_shape(1, e.shapes[1]).center();
        assert!((a[0] - 100.0).abs() < 0.01 && a[1].abs() < 0.01, "{a:?}");
        assert!((b[0] - 0.0).abs() < 0.01 && b[1].abs() < 0.01, "{b:?}");
    }

    #[test]
    fn folder_scale_never_collapses_to_zero() {
        let mut e = pair();
        e.set_folder_prop(1, Prop::Scale, -5.0);
        assert!(
            e.folder(1).unwrap().scale > 0.0,
            "a 0 scale can't be dragged back"
        );
    }

    #[test]
    fn folder_transform_keys_and_poses() {
        let mut e = pair();
        e.set_time(0.0);
        e.set_folder_prop(1, Prop::X, 0.0);
        e.stamp_key();
        e.set_time(2.0);
        e.set_folder_prop(1, Prop::X, 100.0);
        e.stamp_key();
        // Halfway between: smoothstep(0.5) = 0.5, so dead centre.
        e.set_time(1.0);
        e.sync_to_time();
        assert!((e.folder(1).unwrap().x - 50.0).abs() < 0.01);
    }

    #[test]
    fn moving_a_folder_takes_its_whole_run() {
        let mut e = stack(5);
        e.selection = vec![3, 4];
        e.new_folder_from_selection();
        // Folder holds s3,s4 at the top; drop it onto s0 at the bottom.
        assert!(e.move_folder(1, 0));
        assert_eq!(names(&e), vec!["s3", "s4", "s0", "s1", "s2"]);
        assert_eq!(e.folder_members(1), vec![0, 1]);
    }

    #[test]
    fn moving_a_folder_onto_itself_is_refused() {
        let mut e = stack(4);
        e.selection = vec![1, 2];
        e.new_folder_from_selection();
        assert!(
            !e.move_folder(1, 1),
            "dropping on your own contents is a no-op"
        );
    }
}

mod lane_tests {
    use super::tests_support::*;
    use crate::anim::Owner;
    use crate::lanes;
    use crate::props::Prop;

    /// A folder holding two shapes, with its transform keyed at t=0.
    fn keyed_folder() -> crate::editor::Editor {
        let mut e = stack(2);
        e.selection = vec![0, 1];
        e.new_folder_from_selection();
        e.set_time(0.0);
        e.set_folder_prop(1, Prop::X, 50.0);
        e.stamp_key();
        e
    }

    #[test]
    fn a_keyed_folder_always_earns_a_lane() {
        // The bug this guards: folder keys used to animate with no lane to
        // show them, so they could not be seen, selected or deleted.
        let mut e = keyed_folder();
        e.deselect();
        assert!(
            lanes::visible(&e, Owner::Folder(1)),
            "keyed folder must be listed even when nothing is selected"
        );
    }

    #[test]
    fn folder_lanes_sit_above_their_members() {
        let e = keyed_folder();
        let owners = e.key_owners();
        let folder = owners.iter().position(|&o| o == Owner::Folder(1));
        let member = owners.iter().position(|&o| o == Owner::Shape(1));
        assert!(folder < member, "the header leads its contents");
    }

    #[test]
    fn folder_keys_can_be_deleted() {
        let mut e = keyed_folder();
        assert!(e.delete_keys_at(Owner::Folder(1), 0.0));
        assert!(!e.owner_anim(Owner::Folder(1)).unwrap().has_keys());
        // And with the keys gone the lane goes too, once deselected.
        e.deselect();
        assert!(!lanes::visible(&e, Owner::Folder(1)));
    }

    #[test]
    fn folder_keys_retime_like_shape_keys() {
        let mut e = keyed_folder();
        assert!(e.retime_group(&[(Owner::Folder(1), 0.0)], 2.0));
        let times: Vec<f32> = e
            .owner_anim(Owner::Folder(1))
            .unwrap()
            .key_times()
            .iter()
            .map(|&(t, _)| t)
            .collect();
        assert_eq!(times, vec![2.0]);
    }

    #[test]
    fn an_unkeyed_unselected_folder_stays_out_of_the_way() {
        let mut e = stack(2);
        e.selection = vec![0, 1];
        e.new_folder_from_selection();
        e.deselect();
        assert!(!lanes::visible(&e, Owner::Folder(1)));
    }
}

mod composition {
    use super::tests_support::*;
    use crate::props::Prop;

    /// One shape in a folder, both keyed on X over 0..2s.
    fn both_keyed() -> crate::editor::Editor {
        let mut e = stack(2);
        e.shapes[0].set_center([0.0, 0.0]);
        e.shapes[1].set_center([100.0, 0.0]);
        e.selection = vec![0, 1];
        e.new_folder_from_selection();
        // Shape 0 walks 0 -> 200 in its own space.
        e.set_time(0.0);
        e.shapes[0].set_center([0.0, 0.0]);
        e.stamp_key();
        e.set_time(2.0);
        e.shapes[0].set_center([200.0, 0.0]);
        e.set_folder_prop(1, Prop::X, 1000.0);
        e.stamp_key();
        e
    }

    #[test]
    fn folder_and_layer_keys_compose_they_do_not_fight() {
        let mut e = both_keyed();
        e.set_time(2.0);
        e.sync_to_time();
        // The shape's own curve puts it at 200; the folder's curve adds
        // 1000 on top. Neither wins — they stack.
        assert_eq!(e.shapes[0].center()[0], 200.0, "the shape's own pose");
        assert_eq!(
            e.posed_shape(0, e.shapes[0]).center()[0],
            1200.0,
            "folder offset composed on top of it"
        );
    }

    #[test]
    fn a_folder_key_moves_members_that_have_no_keys_of_their_own() {
        let mut e = both_keyed();
        e.set_time(2.0);
        e.sync_to_time();
        // Shape 1 was never keyed, but it still travels with the folder.
        assert_eq!(e.shapes[1].center()[0], 100.0);
        assert_eq!(e.posed_shape(1, e.shapes[1]).center()[0], 1100.0);
    }

    #[test]
    fn the_pivot_drifts_when_members_animate() {
        // Documents current behaviour: the pivot is the members' *live*
        // centers, so a member's own keys move the folder's turning point.
        let mut e = both_keyed();
        e.set_time(0.0);
        e.sync_to_time();
        let at0 = e.folder_pivot(1);
        e.set_time(2.0);
        e.sync_to_time();
        let at2 = e.folder_pivot(1);
        assert_ne!(at0, at2, "pivot follows the animated members");
    }
}
