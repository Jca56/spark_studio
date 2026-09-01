//! The context menu's geometry, held by tests: nobody who can run these
//! can look at the window, so the panel is asserted, not eyeballed.

use super::home::{self, Tone, Verb};
use super::*;
use crate::defaults::{DRAW_TOOLS, ToolDefaults};
use crate::editor::Editor;
use crate::props::Prop;

fn win() -> Viewport {
    Viewport {
        x: 0.0,
        y: 0.0,
        w: 3840.0,
        h: 2160.0,
    }
}

fn inside(outer: Viewport, inner: Viewport, what: &str) {
    assert!(
        inner.x >= outer.x - 0.5
            && inner.y >= outer.y - 0.5
            && inner.x + inner.w <= outer.x + outer.w + 0.5
            && inner.y + inner.h <= outer.y + outer.h + 0.5,
        "{what} escapes the panel: {inner:?} vs {outer:?}"
    );
}

fn overlaps(a: Viewport, b: Viewport) -> bool {
    a.x < b.x + b.w && b.x < a.x + a.w && a.y < b.y + b.h && b.y < a.y + a.h
}

fn draw(e: &mut Editor, from: [f32; 2], to: [f32; 2]) -> usize {
    e.choose_tool(Tool::Circle);
    e.set_cursor_canvas(from);
    e.mouse_down(false);
    e.set_cursor_canvas(to);
    e.mouse_up();
    e.choose_tool(Tool::Select);
    e.primary().expect("drawn")
}

/// The rail is a fixed-size column of squares — the transport's, not a
/// sixth of whatever the panel is — top-aligned with the panel and fully
/// outside it; and the panel is never shorter than the rail.
#[test]
fn the_rail_is_a_fixed_column_the_panel_never_undercuts() {
    for scale in [1.0f32, 1.4] {
        for body_h in [10.0, 300.0, 900.0] {
            let c = build([1000.0, 600.0], scale, win(), body_h);
            let first = c.rail[0].2;
            assert!(
                (first.y - c.panel.y).abs() < 0.5,
                "scale {scale}: the rail doesn't start at the panel's top"
            );
            for (_, _, b) in &c.rail {
                assert!(b.x + b.w < c.panel.x, "scale {scale}: a button leaks in");
                assert!((b.w - RAIL_SIDE * scale).abs() < 0.5, "scale {scale}: not fixed");
                assert!((b.w - b.h).abs() < 0.5, "scale {scale}: not square");
                assert!((b.x - first.x).abs() < 0.5, "scale {scale}: not a column");
            }
            let last = c.rail[5].2;
            assert!(
                last.y + last.h <= c.panel.y + c.panel.h + 0.5,
                "scale {scale} body {body_h}: the rail hangs below the panel"
            );
            assert!(
                (c.panel.h - body_h.max(rail_h(scale))).abs() < 0.5,
                "scale {scale} body {body_h}: the panel is {} tall",
                c.panel.h
            );
        }
    }
}

/// A click in any corner keeps the whole assembly on screen — the
/// rail is part of the footprint, so a far-left click must not push
/// the satellites off the window.
#[test]
fn a_corner_click_keeps_the_whole_assembly_on_screen() {
    for scale in [1.0f32, 1.4] {
        for corner in [[0.0, 0.0], [3840.0, 0.0], [0.0, 2160.0], [3840.0, 2160.0]] {
            let c = build(corner, scale, win(), 500.0);
            assert!(
                c.rail[0].2.x >= 0.0,
                "scale {scale} corner {corner:?}: the rail fell off the left"
            );
            assert!(c.panel.x + c.panel.w <= 3840.0 + 0.5);
            assert!(c.panel.y >= 0.0 && c.panel.y + c.panel.h <= 2160.0 + 0.5);
        }
    }
}

/// Six tools, each once, `1` first — the same order the number keys
/// pick them.
#[test]
fn the_rail_lists_each_tool_once_in_key_order() {
    assert_eq!(RAIL[0].0, Tool::Select, "Move leads, and `1` picks it");
    for (i, (tool, _, _)) in RAIL.iter().enumerate() {
        assert!(
            !RAIL[..i].iter().any(|(t, _, _)| t == tool),
            "{tool:?} is on the rail twice"
        );
    }
    // Every drawing tool titles its defaults page; Move is the home.
    assert_eq!(tool_title(Tool::Select), None);
    assert_eq!(tool_title(Tool::Stars), Some("Star Field"));
}

/// Every tool's page fits a panel exactly its own height at both output
/// scales: the switch and every knob's grab box inside, no two knobs
/// overlapping, the last knob row ending at the bottom pad — and the
/// knob count is the spec's.
#[test]
fn every_tool_page_fits_a_panel_of_its_own_height() {
    for scale in [1.0f32, 1.4] {
        for tool in DRAW_TOOLS {
            let body = page::tool_height(tool, scale);
            let c = build([1000.0, 600.0], scale, win(), body);
            let d = ToolDefaults::birth(tool);
            let p = Page::tool(
                c.panel,
                scale,
                tool,
                tool_title(tool).unwrap(),
                &d,
                spark_render::CANVAS,
            );
            let tag = format!("{tool:?} at {scale}");
            assert_eq!(
                p.knobs.len(),
                crate::defaults::knobs(tool).len(),
                "{tag}: knob count"
            );
            let mut bottom = c.panel.y;
            for (k, slot) in p.knobs.iter().enumerate() {
                inside(c.panel, slot.hit, &format!("{tag}: knob {k}"));
                for other in &p.knobs[..k] {
                    assert!(
                        !overlaps(slot.hit, other.hit),
                        "{tag}: knobs {:?} and {:?} overlap",
                        slot.spec.label,
                        other.spec.label
                    );
                }
                assert!(slot.radius >= 40.0 * scale, "{tag}: a knob too small to grab");
                bottom = bottom.max(slot.hit.y + slot.hit.h);
            }
            if let Some((_, seg, _)) = &p.switch {
                for s in &seg.segments {
                    inside(c.panel, *s, &format!("{tag}: switch"));
                }
            }
            // The page is exactly as tall as it needs: the last row's
            // label room and the pad end at the panel's bottom.
            if body > rail_h(scale) {
                let room = (page::PAD + 34.0) * scale;
                assert!(
                    (bottom + room - (c.panel.y + c.panel.h)).abs() < 1.0,
                    "{tag}: the panel is {} px too tall",
                    c.panel.y + c.panel.h - bottom - room
                );
            }
        }
    }
    // The star field's two rows make the tallest page; a circle's is one.
    assert!(page::tool_height(Tool::Stars, 1.0) > page::tool_height(Tool::Circle, 1.0));
}

/// What lights is what clicks: a knob's centre hits it, a fill's
/// thickness knob is dimmed and does not, the segments hit in order,
/// and the air is nothing.
#[test]
fn hit_testing_matches_the_drawing() {
    let scale = 1.4;
    let c = build(
        [1000.0, 600.0],
        scale,
        win(),
        page::tool_height(Tool::Polygon, scale),
    );
    let mut d = ToolDefaults::birth(Tool::Polygon);
    let page = |d: &ToolDefaults| {
        Page::tool(
            c.panel,
            scale,
            Tool::Polygon,
            "Polygon",
            d,
            spark_render::CANVAS,
        )
    };
    let p = page(&d);
    for (k, slot) in p.knobs.iter().enumerate() {
        assert_eq!(p.hit(slot.center[0], slot.center[1]), Some(Hit::Knob(k)));
    }
    assert_eq!(p.knob_prop(1), Some(Prop::Thickness));
    // Fill: the thickness knob is still drawn but no longer grabs.
    d.outline = false;
    let p = page(&d);
    let thick = &p.knobs[1];
    assert!(!thick.live);
    assert_eq!(p.hit(thick.center[0], thick.center[1]), None);
    assert_eq!(
        p.hit(p.knobs[0].center[0], p.knobs[0].center[1]),
        Some(Hit::Knob(0))
    );
    let (_, seg, active) = p.switch.as_ref().unwrap();
    assert_eq!(*active, 0, "fill is lit");
    for (i, s) in seg.segments.iter().enumerate() {
        assert_eq!(p.hit(s.x + s.w * 0.5, s.y + s.h * 0.5), Some(Hit::Segment(i)));
    }
    assert_eq!(p.hit(c.panel.x + 2.0, c.panel.y + 2.0), None);
    assert_eq!(p.knobs[0].readout, "5");
}

/// Home knows its subject: empty space offers nothing (and no title);
/// an object offers Copy, Paste, Duplicate and Delete — Delete in red,
/// Paste lit only once something is copied — each row hittable exactly
/// when lit, and the panel exactly as tall as its rows.
#[test]
fn home_offers_what_the_target_has() {
    let scale = 1.0;
    let mut e = Editor::empty();
    // Empty space.
    assert!(home::actions(Target::Empty).is_empty());
    assert_eq!(home::title(Target::Empty, &e), "");
    let rows = home::rows(Target::Empty, &e);
    let c = build([1000.0, 600.0], scale, win(), page::rows_height(rows.len(), scale));
    let p = Page::home(c.panel, scale, "", &rows);
    assert!(p.verbs.is_empty());
    assert!(p.labels(&[]).is_empty(), "an empty page says nothing");
    assert!((c.panel.h - rail_h(scale)).abs() < 0.5, "the rail is the floor");

    // An object.
    let i = draw(&mut e, [300.0, 300.0], [360.0, 300.0]);
    let id = e.shape_id(i);
    let target = Target::Object(id);
    let acts = home::actions(target);
    assert_eq!(
        acts.iter().map(|a| a.verb).collect::<Vec<_>>(),
        [Verb::Copy, Verb::Paste, Verb::Duplicate, Verb::Delete]
    );
    assert_eq!(home::title(target, &e), e.display_name(i));
    let rows = home::rows(target, &e);
    let body = page::rows_height(rows.len(), scale);
    let c = build([1000.0, 600.0], scale, win(), body);
    let p = Page::home(c.panel, scale, "circle", &rows);
    let find = |p: &Page, v: Verb| p.verbs.iter().position(|r| r.row.verb == v).expect("row");
    let del = find(&p, Verb::Delete);
    assert_eq!(p.verbs[del].row.tone, Tone::Danger);
    assert!(p.verbs[del].row.enabled);
    let red = theme().red;
    assert!(
        p.labels(&[]).iter().any(|l| l.text == "Delete" && l.color == red),
        "Delete isn't red"
    );
    let paste = find(&p, Verb::Paste);
    assert!(!p.verbs[paste].row.enabled, "nothing copied yet");
    let r = p.verbs[paste].rect;
    assert_eq!(p.hit(r.x + 10.0, r.y + 10.0), None, "a dim row clicked");
    let r = p.verbs[find(&p, Verb::Copy)].rect;
    assert_eq!(p.hit(r.x + 10.0, r.y + 10.0), Some(Hit::Verb(0)));
    for v in &p.verbs {
        inside(c.panel, v.rect, "a verb row");
    }
    // Four rows are shorter than the rail, so the rail is the floor; the
    // rows still end inside it, and a taller table would size the panel.
    let last = p.verbs.last().unwrap().rect;
    assert!(last.y + last.h + page::PAD * scale <= c.panel.y + c.panel.h + 0.5);
    assert!((c.panel.h - body.max(rail_h(scale))).abs() < 0.5);
    let tall = page::rows_height(12, scale);
    assert!(tall > rail_h(scale));
    let c12 = build([1000.0, 600.0], scale, win(), tall);
    assert!((c12.panel.h - tall).abs() < 0.5, "a long table doesn't size the panel");
    // Copy, and Paste lights.
    e.copy_objects();
    let rows = home::rows(target, &e);
    let p = Page::home(c.panel, scale, "circle", &rows);
    assert!(p.verbs[find(&p, Verb::Paste)].row.enabled);
    // A verb keeps its row whatever the selection: the table is the
    // target's, the state only lights it.
    e.deselect();
    let rows = home::rows(target, &e);
    assert_eq!(rows.len(), 4);
    assert!(!rows[0].enabled, "Copy with nothing selected");
    assert!(rows[1].enabled, "Paste needs only the clipboard");
}

/// The knob's feel: a full DRAG_PX of upward travel turns it from empty
/// to full, Shift makes the same travel a tenth, the wheel steps, and the
/// value never leaves 0..1.
#[test]
fn a_knob_turns_by_the_book() {
    let s = 1.4;
    assert!((knob_drag(0.0, -DRAG_PX * s, s, false) - 1.0).abs() < 1e-5, "up is up");
    assert!((knob_drag(0.5, DRAG_PX * s * 0.25, s, false) - 0.25).abs() < 1e-5);
    assert!((knob_drag(0.0, -DRAG_PX * s, s, true) - 0.1).abs() < 1e-5, "fine");
    assert_eq!(knob_drag(0.9, -DRAG_PX * s, s, false), 1.0, "clamped");
    assert!((knob_step(0.5, 1.0, false) - 0.52).abs() < 1e-5);
    assert!((knob_step(0.5, -1.0, true) - 0.495).abs() < 1e-5);
    assert_eq!(knob_step(0.99, 5.0, false), 1.0);
}
