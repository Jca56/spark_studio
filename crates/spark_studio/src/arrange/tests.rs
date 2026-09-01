//! Arrangement layout tests — nobody who can run these can see the rows.

use super::*;
use crate::timeline;

fn fixture() -> (Panel, TimeView, Editor) {
    let panel = timeline::panel(
        Viewport {
            x: 0.0,
            y: 500.0,
            w: 3000.0,
            h: 400.0,
        },
        1.0,
    );
    let view = TimeView::new(0.0, 60.0);
    let mut ed = Editor::empty();
    let id = ed.add_comp_asset("/x/spin.spark".into());
    ed.place_clip(id, 0, 10.0, 20.0);
    (panel, view, ed)
}

/// A comp clip bar sits where its times say on its comp row, and the
/// loop seams land every period inside it.
#[test]
fn a_comp_clip_maps_time_and_marks_its_loops() {
    let (panel, view, ed) = fixture();
    let mut subs = HashMap::new();
    let doc = crate::doc::Doc {
        duration: Some(2.0),
        ..Default::default()
    };
    subs.insert(1, PlacedComp::new("/x/spin.spark".into(), doc, Vec::new()));
    let sc = build(
        &panel,
        &view,
        1.0,
        &ed,
        &subs,
        Some(ClipRef::Comp(0)),
        0.0,
        None,
        None,
    );
    assert_eq!(sc.clips.len(), 1);
    let c = &sc.clips[0];
    assert!((c.bar.x - view.x_of(10.0, panel.axis)).abs() < 0.5);
    assert!((c.bar.x + c.bar.w - view.x_of(30.0, panel.axis)).abs() < 0.5);
    // 20 s of a 2 s comp: nine interior seams.
    assert_eq!(c.loop_xs.len(), 9);
    assert!(c.selected && !c.missing);
    assert_eq!(c.label, "spin");
}

/// An object's row carries its clips, its name, an eye and a glyph;
/// the song closes the list.
#[test]
fn objects_are_tracks_and_the_song_is_one_too(  ) {
    let (panel, view, mut ed) = fixture();
    ed.set_time(4.0);
    ed.set_cursor_canvas([300.0, 300.0]);
    ed.choose_tool(crate::props::Tool::Circle);
    ed.mouse_down(false);
    ed.set_cursor_canvas([400.0, 300.0]);
    ed.mouse_up();
    let subs = HashMap::new();
    let sc = build(
        &panel,
        &view,
        1.0,
        &ed,
        &subs,
        None,
        0.0,
        Some("INFERNO.wav"),
        None,
    );
    // The song on top — it can't move, so it stays in view — then the
    // object row, then the comp row.
    assert!(matches!(sc.rows[0].kind, RowKind::Audio));
    assert_eq!(sc.rows[0].label, "INFERNO.wav");
    assert!(matches!(sc.rows[1].kind, RowKind::Object(0)));
    assert!(matches!(sc.rows[2].kind, RowKind::CompTrack(0)));
    assert!(sc.wave_band.is_some(), "the song row carries the waveform");
    assert!(sc.rows[1].eye.is_some() && sc.rows[1].glyph.is_some());
    // The object's clip bar rides its row, tinted its colour.
    let ob = sc
        .clips
        .iter()
        .find(|c| matches!(c.r, ClipRef::Obj { .. }))
        .expect("the newborn clip");
    assert!((ob.bar.x - view.x_of(4.0, panel.axis)).abs() < 0.5);
    assert!(ob.color.is_some());
    // Selected object dims nothing; scrub the playhead away and the
    // row dims instead.
    ed.set_time(30.0);
    let sc2 = build(&panel, &view, 1.0, &ed, &subs, None, 0.0, None, None);
    assert!(sc2.rows[0].dim, "no clip under the playhead");
    // With the song on top, a drop slot still counts from the first
    // object: the seam under the song's row is slot 0.
    assert_eq!(head_rows(true), 1);
    assert_eq!(
        drop_slot(&panel, 1.0, 0.0, panel.lanes.y + ROW_STEP + 4.0, 1, 1),
        0
    );
    assert_eq!(drop_slot(&panel, 1.0, 0.0, panel.lanes.y + 4.0, 1, 1), 0);
    assert_eq!(
        drop_slot(&panel, 1.0, 0.0, panel.lanes.y + ROW_STEP * 2.0, 1, 1),
        1
    );
    let subs = HashMap::new();
    let with_song = build(
        &panel,
        &view,
        1.0,
        &ed,
        &subs,
        None,
        0.0,
        Some("INFERNO.wav"),
        Some(RowDragView {
            kind: RowKind::Object(0),
            dy: 0.0,
            slot: 0,
        }),
    );
    assert!(
        (with_song.drop_y.unwrap() - (panel.lanes.y + ROW_STEP)).abs() < 0.5,
        "the line for slot 0 sits under the song"
    );
}

/// Draw a circle at the playhead; the editor's index for it.
fn draw(ed: &mut Editor, x: f32) -> usize {
    ed.set_cursor_canvas([x, 300.0]);
    ed.choose_tool(crate::props::Tool::Circle);
    ed.mouse_down(false);
    ed.set_cursor_canvas([x + 60.0, 300.0]);
    ed.mouse_up();
    ed.primary().expect("drawn")
}

/// Rows run in stack order: the first object drawn is the top row and a
/// newborn lands at the bottom — lower in the list draws in front.
#[test]
fn a_new_object_lands_at_the_bottom_of_the_list() {
    let (panel, view, mut ed) = fixture();
    ed.set_time(0.0);
    let a = draw(&mut ed, 100.0);
    let b = draw(&mut ed, 400.0);
    let c = draw(&mut ed, 700.0);
    assert!(a < b && b < c, "later drawn, higher in the stack");
    let subs = HashMap::new();
    let sc = build(&panel, &view, 1.0, &ed, &subs, None, 0.0, None, None);
    let kinds: Vec<RowKind> = sc.rows.iter().map(|r| r.kind).collect();
    assert_eq!(
        &kinds[..3],
        &[RowKind::Object(a), RowKind::Object(b), RowKind::Object(c)]
    );
    assert!(sc.rows[0].cell.y < sc.rows[2].cell.y, "the first drawn is on top");
    assert!(matches!(kinds[3], RowKind::CompTrack(0)), "comps close the list");
}

/// A dragged row rides the cursor, the gold line marks the seam it will
/// drop into, and the drop maths lands it there — before an object,
/// before a folder's first member, or at the end of the stack.
#[test]
fn a_row_drag_lands_at_the_gold_line() {
    let (panel, view, mut ed) = fixture();
    ed.set_time(0.0);
    let a = draw(&mut ed, 100.0);
    let b = draw(&mut ed, 400.0);
    let c = draw(&mut ed, 700.0);
    let pitch = ROW_STEP;
    // Seams: 0 above a, 1 between a and b, 2 between b and c, 3 after c
    // — and no further: the comp track and the song don't take drops.
    let seam = |k: usize| panel.lanes.y + k as f32 * pitch;
    assert_eq!(drop_slot(&panel, 1.0, 0.0, seam(0) + 4.0, 3, 0), 0);
    assert_eq!(drop_slot(&panel, 1.0, 0.0, seam(2) - 10.0, 3, 0), 2);
    assert_eq!(drop_slot(&panel, 1.0, 0.0, seam(3) + 200.0, 3, 0), 3);
    // Scrolled down a row, the same cursor y is one seam later.
    assert_eq!(drop_slot(&panel, 1.0, pitch, seam(2) - 10.0, 3, 0), 3);
    assert_eq!(drop_dest(&ed, 0), a);
    assert_eq!(drop_dest(&ed, 2), c);
    assert_eq!(drop_dest(&ed, 3), ed.shapes().len(), "past the last row: the end");
    // The frame lifts the dragged row by the cursor's travel and puts the
    // line at the seam.
    let subs = HashMap::new();
    let drag = RowDragView {
        kind: RowKind::Object(a),
        dy: 70.0,
        slot: 2,
    };
    let sc = build(&panel, &view, 1.0, &ed, &subs, None, 0.0, None, Some(drag));
    assert_eq!(sc.dragged, Some(0));
    let still = build(&panel, &view, 1.0, &ed, &subs, None, 0.0, None, None);
    assert!((sc.rows[0].cell.y - still.rows[0].cell.y - 70.0).abs() < 0.5);
    assert!((sc.rows[1].cell.y - still.rows[1].cell.y).abs() < 0.5, "the rest hold");
    assert!((sc.drop_y.unwrap() - seam(2)).abs() < 0.5);
    // And the move itself: a to the seam after b puts it between b and c
    // — the row's slot is the stack's slot.
    let (ida, idb) = (ed.shape_id(a), ed.shape_id(b));
    assert!(ed.move_layer(a, 1));
    assert_eq!(ed.index_of(idb), Some(0));
    assert_eq!(ed.index_of(ida), Some(1));
}

/// Edges trim, the middle moves, the sidebar answers heads and eyes.
#[test]
fn the_grips_are_edges_then_body_then_rows() {
    let (panel, view, ed) = fixture();
    let subs = HashMap::new();
    let sc = build(&panel, &view, 1.0, &ed, &subs, None, 0.0, None, None);
    let c = &sc.clips[0];
    let y = c.bar.y + c.bar.h * 0.5;
    let r = c.r;
    assert_eq!(
        hit(&sc, c.bar.x + 3.0, y, 1.0),
        Some(ArrHit::Clip(r, Zone::Left))
    );
    assert_eq!(
        hit(&sc, c.bar.x + c.bar.w - 3.0, y, 1.0),
        Some(ArrHit::Clip(r, Zone::Right))
    );
    assert_eq!(
        hit(&sc, c.bar.x + c.bar.w * 0.5, y, 1.0),
        Some(ArrHit::Clip(r, Zone::Move))
    );
    let row = &sc.rows[0];
    assert_eq!(
        hit(&sc, row.label_pos[0] + 4.0, row.cell.y + row.cell.h * 0.5, 1.0),
        Some(ArrHit::Head(row.kind))
    );
    assert_eq!(hit(&sc, c.bar.x - 800.0, y - 200.0, 1.0), None);
}
