//! Arrangement layout tests — nobody who can run these can see the rows.

use super::*;
use crate::comps::PlacedComp;
use crate::timeline::{self, TimeView};
use std::collections::HashMap;

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

/// A song track as the studio would hand it over: one whole clip at
/// `start`, `span` long.
fn song(start: f32, span: f32) -> AudioTrack {
    AudioTrack {
        asset: crate::doc::SONG,
        name: "INFERNO.wav".into(),
        missing: false,
        volume: "+0.0 dB".into(),
        clips: vec![AudioBar { k: 0, start, span }],
    }
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
        &[ClipRef::Comp(0)],
        0.0,
        &[],
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
/// the song leads the list on a taller row with a volume box, and its
/// clip bar sits where the song was placed.
#[test]
fn objects_are_tracks_and_the_song_is_one_too() {
    let (panel, view, mut ed) = fixture();
    ed.set_time(4.0);
    ed.set_cursor_canvas([300.0, 300.0]);
    ed.choose_tool(crate::props::Tool::Circle);
    ed.mouse_down(false);
    ed.set_cursor_canvas([400.0, 300.0]);
    ed.mouse_up();
    let subs = HashMap::new();
    let audio = [song(6.0, 30.0)];
    let sc = build(&panel, &view, 1.0, &ed, &subs, &[], 0.0, &audio, None);
    // The song on top — it can't move, so it stays in view — then the
    // object row, then the comp row.
    assert!(matches!(sc.rows[0].kind, RowKind::Audio(0)));
    assert_eq!(sc.rows[0].label, "INFERNO.wav");
    assert!(matches!(sc.rows[1].kind, RowKind::Object(0)));
    assert!(matches!(sc.rows[2].kind, RowKind::CompTrack(0)));
    // The audio row is the tall one, and its volume box reads inside it.
    assert!(sc.rows[0].cell.h > sc.rows[1].cell.h);
    let (vb, reading) = sc.rows[0].volume.as_ref().expect("a volume box");
    assert_eq!(reading, "+0.0 dB");
    assert!(vb.y > sc.rows[0].label_pos[1], "the box sits under the name");
    assert!(vb.y + vb.h <= sc.rows[0].cell.y + sc.rows[0].cell.h);
    assert!(sc.rows[1].volume.is_none(), "objects have no volume");
    assert!(
        (sc.rows[1].cell.y - (sc.rows[0].cell.y + AUDIO_ROW_STEP)).abs() < 0.5,
        "the object row follows the tall row's pitch"
    );
    // The song's clip bar rides its row where the song sits, teal, and
    // says which file's waveform to draw.
    let sb = sc
        .clips
        .iter()
        .find(|c| matches!(c.r, ClipRef::Audio(0)))
        .expect("the song's clip");
    assert!((sb.bar.x - view.x_of(6.0, panel.axis)).abs() < 0.5);
    assert!((sb.bar.x + sb.bar.w - view.x_of(36.0, panel.axis)).abs() < 0.5);
    assert_eq!(sb.audio, Some(0));
    assert!(sb.bar.y >= sc.rows[0].cell.y - 4.0 && sb.bar.y < sc.rows[1].cell.y);
    assert!(sc.rows[1].eye.is_some() && sc.rows[1].glyph.is_some());
    // The object's clip bar rides its row, tinted its colour.
    let ob = sc
        .clips
        .iter()
        .find(|c| matches!(c.r, ClipRef::Obj { .. }))
        .expect("the newborn clip");
    assert!((ob.bar.x - view.x_of(4.0, panel.axis)).abs() < 0.5);
    assert!(ob.color.is_some());
    // The volume box answers a press; the name beside it is the head.
    assert_eq!(
        hit(&sc, vb.x + 4.0, vb.y + vb.h * 0.5, 1.0),
        Some(ArrHit::Volume(0))
    );
    assert_eq!(
        hit(&sc, sc.rows[0].label_pos[0] + 4.0, sc.rows[0].label_pos[1] + 4.0, 1.0),
        Some(ArrHit::Head(RowKind::Audio(0)))
    );
    // Selected object dims nothing; scrub the playhead away and the
    // row dims instead.
    ed.set_time(30.0);
    let sc2 = build(&panel, &view, 1.0, &ed, &subs, &[], 0.0, &[], None);
    assert!(sc2.rows[0].dim, "no clip under the playhead");
    // With the song on top, a drop slot still counts from the first
    // object: the seam under the song's tall row is slot 0.
    let head = head_px(&audio, 1.0);
    assert_eq!(head, AUDIO_ROW_STEP);
    assert_eq!(
        drop_slot(&panel, 1.0, 0.0, panel.lanes.y + head + 4.0, 1, head),
        0
    );
    assert_eq!(drop_slot(&panel, 1.0, 0.0, panel.lanes.y + 4.0, 1, head), 0);
    assert_eq!(
        drop_slot(&panel, 1.0, 0.0, panel.lanes.y + head + ROW_STEP, 1, head),
        1
    );
    let with_song = build(
        &panel,
        &view,
        1.0,
        &ed,
        &subs,
        &[],
        0.0,
        &audio,
        Some(RowDragView {
            kind: RowKind::Object(0),
            dy: 0.0,
            slot: 0,
        }),
    );
    assert!(
        (with_song.drop_y.unwrap() - (panel.lanes.y + head)).abs() < 0.5,
        "the line for slot 0 sits under the song"
    );
}

/// A missing sound keeps its row and its clips, both saying so.
#[test]
fn a_missing_sound_stays_on_the_arrangement_in_red() {
    let (panel, view, ed) = fixture();
    let subs = HashMap::new();
    let audio = [AudioTrack {
        asset: 3,
        name: "vo.wav".into(),
        missing: true,
        volume: "-6.0 dB".into(),
        clips: vec![AudioBar {
            k: 2,
            start: 1.0,
            span: 2.0,
        }],
    }];
    let sc = build(&panel, &view, 1.0, &ed, &subs, &[], 0.0, &audio, None);
    assert!(matches!(sc.rows[0].kind, RowKind::Audio(3)));
    assert_eq!(sc.rows[0].label, "! vo.wav");
    let c = sc.clips.iter().find(|c| c.r == ClipRef::Audio(2)).unwrap();
    assert!(c.missing);
    assert_eq!(c.label, "! missing: vo.wav");
    assert_eq!(content_height(&ed, &audio, 1.0), 3.0 * ROW_STEP, "at least three rows");
    assert_eq!(row_count(&ed, &audio), 2);
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
    let sc = build(&panel, &view, 1.0, &ed, &subs, &[], 0.0, &[], None);
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
    assert_eq!(drop_slot(&panel, 1.0, 0.0, seam(0) + 4.0, 3, 0.0), 0);
    assert_eq!(drop_slot(&panel, 1.0, 0.0, seam(2) - 10.0, 3, 0.0), 2);
    assert_eq!(drop_slot(&panel, 1.0, 0.0, seam(3) + 200.0, 3, 0.0), 3);
    // Scrolled down a row, the same cursor y is one seam later.
    assert_eq!(drop_slot(&panel, 1.0, pitch, seam(2) - 10.0, 3, 0.0), 3);
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
    let sc = build(&panel, &view, 1.0, &ed, &subs, &[], 0.0, &[], Some(drag));
    assert_eq!(sc.dragged, Some(0));
    let still = build(&panel, &view, 1.0, &ed, &subs, &[], 0.0, &[], None);
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
    let sc = build(&panel, &view, 1.0, &ed, &subs, &[], 0.0, &[], None);
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
