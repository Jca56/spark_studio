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
    let sc = build(&panel, &view, 1.0, &ed, &subs, None, 0.0, Some("INFERNO.wav"));
    // Object row first, comp row, then the song.
    assert!(matches!(sc.rows[0].kind, RowKind::Object(0)));
    assert!(matches!(sc.rows[1].kind, RowKind::CompTrack(0)));
    assert!(matches!(sc.rows[2].kind, RowKind::Audio));
    assert_eq!(sc.rows[2].label, "INFERNO.wav");
    assert!(sc.wave_band.is_some(), "the song row carries the waveform");
    assert!(sc.rows[0].eye.is_some() && sc.rows[0].glyph.is_some());
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
    let sc2 = build(&panel, &view, 1.0, &ed, &subs, None, 0.0, None);
    assert!(sc2.rows[0].dim, "no clip under the playhead");
}

/// Edges trim, the middle moves, the sidebar answers heads and eyes.
#[test]
fn the_grips_are_edges_then_body_then_rows() {
    let (panel, view, ed) = fixture();
    let subs = HashMap::new();
    let sc = build(&panel, &view, 1.0, &ed, &subs, None, 0.0, None);
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
