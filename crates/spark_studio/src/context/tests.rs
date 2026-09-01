//! The context menu's geometry, held by tests: nobody who can run these
//! can look at the window, so the panel is asserted, not eyeballed.

use super::home::Verb;
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

/// The rail rides the panel's left flank: every button fully outside
/// the body, square, one column, and the stack spanning exactly the
/// panel's height — top button flush with the top edge, bottom
/// button flush with the bottom.
#[test]
fn the_rail_spans_exactly_the_panels_height() {
    for scale in [1.0f32, 1.4] {
        let c = build([1000.0, 600.0], scale, win());
        let first = c.rail[0].2;
        let last = c.rail[5].2;
        assert!(
            (first.y - c.panel.y).abs() < 0.5,
            "scale {scale}: the rail doesn't start at the panel's top"
        );
        assert!(
            (last.y + last.h - (c.panel.y + c.panel.h)).abs() < 0.5,
            "scale {scale}: the rail misses the panel's bottom by {}",
            (c.panel.y + c.panel.h) - (last.y + last.h)
        );
        for (_, _, b) in &c.rail {
            assert!(
                b.x + b.w < c.panel.x,
                "scale {scale}: a button leaks into the panel"
            );
            assert!((b.w - b.h).abs() < 0.5, "scale {scale}: not square");
            assert!((b.x - first.x).abs() < 0.5, "scale {scale}: not a column");
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
            let c = build(corner, scale, win());
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

/// Every tool's page fits its panel at both output scales: the switch,
/// every knob's grab box, the chips and the picker all inside, no two
/// knobs overlapping, the picker never squeezed below its floor — and the
/// knob count is the spec's.
#[test]
fn every_tool_page_fits_inside_the_panel() {
    for scale in [1.0f32, 1.4] {
        let c = build([1000.0, 600.0], scale, win());
        for tool in DRAW_TOOLS {
            let d = ToolDefaults::birth(tool);
            let page = Page::tool(
                c.panel,
                scale,
                tool,
                tool_title(tool).unwrap(),
                &d,
                spark_render::CANVAS,
                Some(0),
                [0.5, 1.0, 1.0],
            );
            let tag = format!("{tool:?} at {scale}");
            assert_eq!(
                page.knobs.len(),
                crate::defaults::knobs(tool).len(),
                "{tag}: knob count"
            );
            for (k, slot) in page.knobs.iter().enumerate() {
                inside(c.panel, slot.hit, &format!("{tag}: knob {k}"));
                for other in &page.knobs[..k] {
                    assert!(
                        !overlaps(slot.hit, other.hit),
                        "{tag}: knobs {:?} and {:?} overlap",
                        slot.spec.label,
                        other.spec.label
                    );
                }
                assert!(
                    slot.radius >= 40.0 * scale,
                    "{tag}: a knob too small to grab"
                );
            }
            if let Some((_, seg, _)) = &page.switch {
                for s in &seg.segments {
                    inside(c.panel, *s, &format!("{tag}: switch"));
                }
            }
            let (chips, _) = page.chips.as_ref().expect("chips on every tool page");
            for (i, chip) in chips.rects(&PALETTE, None).iter().enumerate() {
                let v = Viewport {
                    x: chip.pos[0],
                    y: chip.pos[1],
                    w: chip.size[0],
                    h: chip.size[1],
                };
                inside(c.panel, v, &format!("{tag}: chip {i}"));
                assert_eq!(
                    page.hit(v.x + v.w * 0.5, v.y + v.h * 0.5),
                    Some(Hit::Chip(i))
                );
            }
            let (p, _) = page.picker.as_ref().expect("a picker on every tool page");
            inside(c.panel, p.sv, &format!("{tag}: picker square"));
            inside(c.panel, p.hue, &format!("{tag}: hue bar"));
            assert!(
                p.sv.h >= page::PICKER_MIN * scale - 0.5,
                "{tag}: the picker was squeezed to {}",
                p.sv.h / scale
            );
            // The picker sits below every knob and chip: nothing overlaps
            // it.
            for slot in &page.knobs {
                assert!(
                    !overlaps(slot.hit, p.sv),
                    "{tag}: a knob sits on the picker"
                );
            }
            let chip0 = chips.rects(&PALETTE, None)[0];
            assert!(
                chip0.pos[1] + chip0.size[1] <= p.sv.y + 0.5,
                "{tag}: the chips run into the picker"
            );
        }
    }
}

/// What lights is what clicks: a knob's centre hits it, a fill's
/// thickness knob is dimmed and does not, the segments hit in order,
/// and the picker's two halves are told apart.
#[test]
fn hit_testing_matches_the_drawing() {
    let scale = 1.4;
    let c = build([1000.0, 600.0], scale, win());
    let mut d = ToolDefaults::birth(Tool::Polygon);
    let page = |d: &ToolDefaults| {
        Page::tool(
            c.panel,
            scale,
            Tool::Polygon,
            "Polygon",
            d,
            spark_render::CANVAS,
            None,
            [0.0, 0.0, 1.0],
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
        assert_eq!(
            p.hit(s.x + s.w * 0.5, s.y + s.h * 0.5),
            Some(Hit::Segment(i))
        );
    }
    let (pk, _) = p.picker.as_ref().unwrap();
    assert_eq!(p.hit(pk.sv.x + 5.0, pk.sv.y + 5.0), Some(Hit::Sv));
    assert_eq!(p.hit(pk.hue.x + 5.0, pk.hue.y + 5.0), Some(Hit::Hue));
    // The panel's air is nothing.
    assert_eq!(p.hit(c.panel.x + 2.0, c.panel.y + 2.0), None);
    // And the readout is the number the knob holds.
    assert_eq!(p.knobs[0].readout, "5");
}

/// Home lists its verbs inside the panel, every row hittable when its
/// verb applies and inert when it doesn't; the merge row flips to Unmerge
/// for a merged selection.
#[test]
fn home_lists_its_verbs_for_the_selection() {
    let scale = 1.0;
    let c = build([1000.0, 600.0], scale, win());
    let mut e = Editor::empty();
    let empty = home::state(&e);
    assert_eq!(empty.title, "no selection");
    let p = Page::home(c.panel, scale, &empty);
    assert_eq!(p.verbs.len(), empty.rows.len());
    for row in &p.verbs {
        inside(c.panel, row.row, "a verb row");
        assert!(!row.enabled, "{} lit with nothing selected", row.label);
        assert_eq!(p.hit(row.row.x + 10.0, row.row.y + 10.0), None);
    }
    // Two circles selected: most verbs light, Merge among them.
    for x in [300.0, 700.0] {
        e.choose_tool(Tool::Circle);
        e.set_cursor_canvas([x, 300.0]);
        e.mouse_down(false);
        e.set_cursor_canvas([x + 60.0, 300.0]);
        e.mouse_up();
    }
    e.choose_tool(Tool::Select);
    e.select(Some(0));
    e.toggle_select(1);
    let two = home::state(&e);
    assert_eq!(two.title, "2 layers selected");
    let p = Page::home(c.panel, scale, &two);
    let find = |v: Verb| p.verbs.iter().position(|r| r.verb == v).expect("row");
    let merge = find(Verb::Merge);
    assert!(p.verbs[merge].enabled);
    assert_eq!(
        p.hit(p.verbs[merge].row.x + 10.0, p.verbs[merge].row.y + 10.0),
        Some(Hit::Verb(merge))
    );
    assert!(
        !p.verbs[find(Verb::PasteStyle)].enabled,
        "nothing copied yet"
    );
    assert!(p.verbs[find(Verb::Duplicate)].enabled);
    // Merged: the row reads Unmerge.
    assert!(e.merge_selected());
    let merged = home::state(&e);
    assert!(merged.rows.iter().any(|(v, ..)| *v == Verb::Unmerge));
    assert!(!merged.rows.iter().any(|(v, ..)| *v == Verb::Merge));
    // Copy a style and Paste lights.
    e.copy_style();
    assert!(
        home::state(&e)
            .rows
            .iter()
            .any(|(v, _, _, on)| *v == Verb::PasteStyle && *on)
    );
}

/// The knob's feel: a full DRAG_PX of upward travel turns it from empty
/// to full, Shift makes the same travel a tenth, the wheel steps, and the
/// value never leaves 0..1.
#[test]
fn a_knob_turns_by_the_book() {
    let s = 1.4;
    assert!(
        (knob_drag(0.0, -DRAG_PX * s, s, false) - 1.0).abs() < 1e-5,
        "up is up"
    );
    assert!((knob_drag(0.5, DRAG_PX * s * 0.25, s, false) - 0.25).abs() < 1e-5);
    assert!(
        (knob_drag(0.0, -DRAG_PX * s, s, true) - 0.1).abs() < 1e-5,
        "fine"
    );
    assert_eq!(knob_drag(0.9, -DRAG_PX * s, s, false), 1.0, "clamped");
    assert!((knob_step(0.5, 1.0, false) - 0.52).abs() < 1e-5);
    assert!((knob_step(0.5, -1.0, true) - 0.495).abs() < 1e-5);
    assert_eq!(knob_step(0.99, 5.0, false), 1.0);
}

/// The picker round-trips every palette colour through HSV and back
/// exactly enough that the chip ring stays lit after a pick.
#[test]
fn the_picker_round_trips_the_palette() {
    for (i, &rgb) in PALETTE.iter().enumerate() {
        let back = rgb_of(hsv_of(rgb));
        for c in 0..3 {
            assert!(
                (back[c] - rgb[c]).abs() < 1e-3,
                "palette {i} came back as {back:?}, was {rgb:?}"
            );
        }
        assert!(same_colour(back, rgb) || (back[0] - rgb[0]).abs() < 1e-3);
    }
}
