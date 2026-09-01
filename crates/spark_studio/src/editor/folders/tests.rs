//! Folder model tests: the stack invariant, the eye gate, reordering,
//! and the transform parent's composition math.

pub(super) mod tests_support {
    use super::super::*;
    use spark_render::Shape;

    pub(crate) fn stack(n: usize) -> Editor {
        let mut e = Editor::empty();
        for k in 0..n {
            let i = e.push_shape(Shape::circle([k as f32 * 10.0, 0.0], 10.0));
            e.names[i] = format!("s{k}");
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

    /// A folder's fade multiplies into its members rather than replacing
    /// their own: a shape already at 40% inside a folder at 50% is at 20%,
    /// not back up at 50%. (The honest limitation this buys: members
    /// composite one by one, so overlapping ones show through each other
    /// halfway down a fade. Doing it properly means rendering the folder to
    /// its own texture.)
    #[test]
    fn a_folder_fade_multiplies_into_its_members() {
        let mut e = stack(2);
        e.select(Some(0));
        e.toggle_select(1);
        e.new_folder_from_selection();
        let id = e.folders[0].id;
        e.shapes[1].set_opacity(0.4);

        assert!(e.folders[0].is_identity(), "a fresh folder is not identity");
        e.set_folder_prop(id, Prop::Opacity, 0.5);
        assert!(
            !e.folders[0].is_identity(),
            "a faded folder still composed as identity"
        );

        assert_eq!(e.posed_shape(0, e.shapes[0]).opacity(), 0.5);
        let both = e.posed_shape(1, e.shapes[1]).opacity();
        assert!(
            (both - 0.2).abs() < 1e-6,
            "40% inside 50% came out at {both}"
        );
        // The document itself is untouched — folders pose the display copy.
        assert_eq!(e.shapes[1].opacity(), 0.4);
    }
}
