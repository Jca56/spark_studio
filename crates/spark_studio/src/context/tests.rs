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

/// One shape, always: the panel is its fixed tall rectangle whatever the
/// scale, and the rail is a column of fixed small squares — the
/// transport's size, not a sixth of the panel — top-aligned with it,
/// fully outside it, and never hanging below it.
#[test]
fn the_panel_is_one_tall_rectangle_with_a_small_rail() {
    for scale in [1.0f32, 1.4] {
        let c = build([1000.0, 600.0], scale, win());
        assert!((c.panel.w - PANEL_W * scale).abs() < 0.5);
        assert!((c.panel.h - PANEL_H * scale).abs() < 0.5);
        assert!(c.panel.h > c.panel.w * 1.4, "not a tall rectangle");
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
        assert!(last.y + last.h <= c.panel.y + c.panel.h + 0.5, "the rail hangs below");
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

/// Every tool's page fits the one panel at both output scales: the
/// switch and every slider's band inside, no two bands overlapping, the
/// track inside its band — and the slider count is the spec's.
#[test]
fn every_tool_page_fits_the_panel() {
    for scale in [1.0f32, 1.4] {
        let c = build([1000.0, 600.0], scale, win());
        for tool in DRAW_TOOLS {
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
                p.sliders.len(),
                crate::defaults::sliders(tool).len(),
                "{tag}: slider count"
            );
            for (k, slot) in p.sliders.iter().enumerate() {
                inside(c.panel, slot.hit, &format!("{tag}: slider {k}"));
                inside(slot.hit, slot.track, &format!("{tag}: slider {k}'s track"));
                assert!(slot.hit.h >= 30.0 * scale, "{tag}: a band too thin to grab");
                for other in &p.sliders[..k] {
                    assert!(
                        !overlaps(slot.hit, other.hit),
                        "{tag}: sliders {:?} and {:?} overlap",
                        slot.spec.label,
                        other.spec.label
                    );
                }
                // Its label line sits above its band, inside the panel.
                assert!(slot.label_y >= c.panel.y && slot.label_y < slot.hit.y);
            }
            if let Some((_, seg, _)) = &p.switch {
                for s in &seg.segments {
                    inside(c.panel, *s, &format!("{tag}: switch"));
                }
            }
        }
    }
}

/// What lights is what clicks: a slider's band hits it anywhere across,
/// a fill's thickness slider is dimmed and does not, the segments hit in
/// order, the air is nothing — and the thumb sits where the value says.
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
        )
    };
    let p = page(&d);
    for (k, slot) in p.sliders.iter().enumerate() {
        let (h, b) = (slot.hit, slot.hit);
        assert_eq!(p.hit(h.x + 3.0, h.y + h.h * 0.5), Some(Hit::Slider(k)), "left end");
        assert_eq!(p.hit(b.x + b.w - 3.0, b.y + 3.0), Some(Hit::Slider(k)), "right end");
        // The thumb the widget draws is under the value the slot holds.
        let thumb = Slider::rects(slot.track, slot.v)[2];
        let back = Slider::t_at(slot.track, thumb.pos[0] + thumb.size[0] * 0.5);
        assert!((back - slot.v).abs() < 1e-3, "slider {k}: thumb at {back}, value {}", slot.v);
    }
    // Alva's order: Sides, Opacity, Brightness, Thickness, Glow.
    assert_eq!(p.slider_prop(0), Some(Prop::Sides));
    assert_eq!(p.slider_prop(1), Some(Prop::Opacity));
    assert_eq!(p.slider_prop(3), Some(Prop::Thickness));
    // Fill: the thickness slider is still drawn but no longer grabs.
    d.outline = false;
    let p = page(&d);
    let thick = &p.sliders[3];
    assert!(!thick.live);
    assert_eq!(p.hit(thick.hit.x + 10.0, thick.hit.y + 10.0), None);
    assert_eq!(
        p.hit(p.sliders[0].hit.x + 10.0, p.sliders[0].hit.y + 10.0),
        Some(Hit::Slider(0))
    );
    let (_, seg, active) = p.switch.as_ref().unwrap();
    assert_eq!(*active, 0, "fill is lit");
    for (i, s) in seg.segments.iter().enumerate() {
        assert_eq!(p.hit(s.x + s.w * 0.5, s.y + s.h * 0.5), Some(Hit::Segment(i)));
    }
    assert_eq!(p.hit(c.panel.x + 2.0, c.panel.y + 2.0), None);
    assert_eq!(p.sliders[0].readout, "5");
    // Every slider has a label and a readout, and an engaged one goes gold.
    let labels = p.labels(Some(Hit::Slider(0)), None);
    assert!(labels.iter().any(|l| l.text == "Sides"));
    assert!(labels.iter().any(|l| l.text == "5" && l.color == theme().accent));
}

/// Home knows its subject: empty space offers nothing (and no title);
/// an object offers Copy, Paste, Duplicate and Delete — Delete in red,
/// Paste lit only once something is copied — each row hittable exactly
/// when lit.
#[test]
fn home_offers_what_the_target_has() {
    let scale = 1.0;
    let c = build([1000.0, 600.0], scale, win());
    let mut e = Editor::empty();
    // Empty space.
    assert!(home::actions(Target::Empty).is_empty());
    assert_eq!(home::title(Target::Empty, &e), "");
    let rows = home::rows(Target::Empty, &e);
    let p = Page::home(c.panel, scale, "", &rows);
    assert!(p.verbs.is_empty());
    assert!(p.labels(None, None).is_empty(), "an empty page says nothing");

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
    let p = Page::home(c.panel, scale, "circle", &rows);
    let find = |p: &Page, v: Verb| p.verbs.iter().position(|r| r.row.verb == v).expect("row");
    let del = find(&p, Verb::Delete);
    assert_eq!(p.verbs[del].row.tone, Tone::Danger);
    assert!(p.verbs[del].row.enabled);
    let red = theme().red;
    assert!(
        p.labels(None, None).iter().any(|l| l.text == "Delete" && l.color == red),
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

/// The wheel steps a slider a fiftieth a notch, a two-hundredth with
/// Shift, and never past its ends.
#[test]
fn a_slider_steps_by_the_book() {
    assert!((slider_step(0.5, 1.0, false) - 0.52).abs() < 1e-5);
    assert!((slider_step(0.5, -1.0, true) - 0.495).abs() < 1e-5);
    assert_eq!(slider_step(0.99, 5.0, false), 1.0);
    assert_eq!(slider_step(0.01, -5.0, false), 0.0);
}

/// The clip view's pages: a key's page carries a value box, the
/// Linear|Smooth switch and Copy · Cut · Paste · Delete — Paste lit
/// only once keys are copied; a set's page has the switch and no box; a
/// row's has the verbs alone; the graph's air has Paste here. Every
/// widget answers where it is drawn and sits inside the panel.
#[test]
fn the_clip_views_pages_carry_a_box_a_switch_and_the_verbs() {
    use super::page::{EASES, FieldSlot};
    let scale = 1.4;
    let c = build([1000.0, 600.0], scale, win());
    let e = Editor::empty();
    let keys = Target::Keys { at: 0.5 };
    assert_eq!(
        home::actions(keys).iter().map(|a| a.verb).collect::<Vec<_>>(),
        [Verb::Copy, Verb::Cut, Verb::Paste, Verb::Delete]
    );
    assert_eq!(home::actions(Target::Row(crate::anim::Target::Shape(Prop::X))).len(), 4);
    assert_eq!(home::actions(Target::Graph { at: 1.0 }).len(), 1);
    assert!(home::title(keys, &e).is_empty(), "the view titles its own pages");
    let rows = home::rows(keys, &e);
    assert!(!rows[2].enabled, "Paste with nothing copied");
    assert!(rows[0].enabled && rows[1].enabled && rows[3].enabled);
    assert_eq!(rows[3].tone, Tone::Danger);
    // One key: the box, the switch on Linear, the rows under both.
    let p = Page::keys(c.panel, scale, "X · Bar 1.2", Some(("600".to_string(), None)), Some(Some(0)), None, &rows);
    let f = p.field.clone().expect("a value box");
    assert_eq!(f, FieldSlot { rect: f.rect, text: "600".to_string(), editing: false });
    inside(c.panel, f.rect, "the value box");
    assert_eq!(p.hit(f.rect.x + 5.0, f.rect.y + 5.0), Some(Hit::Field));
    let (seg, active) = p.ease.as_ref().expect("the switch");
    assert_eq!(*active, Some(0));
    assert_eq!(seg.segments.len(), EASES.len());
    assert!(seg.segments[0].y > f.rect.y + f.rect.h, "the switch sits under the box");
    let r = seg.segments[1];
    assert_eq!(p.hit(r.x + 5.0, r.y + 5.0), Some(Hit::Ease(1)));
    assert!(p.verbs[0].rect.y > r.y + r.h, "the rows sit under the switch");
    assert_eq!(p.hit(p.verbs[0].rect.x + 5.0, p.verbs[0].rect.y + 5.0), Some(Hit::Verb(0)));
    assert_eq!(p.hit(p.verbs[2].rect.x + 5.0, p.verbs[2].rect.y + 5.0), None, "Paste is dim");
    for v in &p.verbs {
        inside(c.panel, v.rect, "a verb row");
    }
    let words: Vec<String> = p.labels(None, None).iter().map(|l| l.text.clone()).collect();
    for w in ["X · Bar 1.2", "600", "Linear", "Smooth", "Copy", "Cut", "Paste", "Delete"] {
        assert!(words.iter().any(|x| x == w), "missing {w}: {words:?}");
    }
    assert!(p.edit_box().is_none());
    // Typing: the box shows the buffer, left-aligned, and hands the frame
    // its caret box.
    let p = Page::keys(c.panel, scale, "X · Bar 1.2", Some(("600".to_string(), Some("61".to_string()))), Some(Some(0)), None, &rows);
    assert!(p.field.as_ref().is_some_and(|f| f.editing && f.text == "61"));
    let (rect, x0, _) = p.edit_box().expect("a caret box");
    assert_eq!(rect, p.field.as_ref().unwrap().rect);
    assert!(x0 > rect.x && x0 < rect.x + 30.0);
    // A set: no box, a mixed switch lights nothing, the rows climb.
    let p_set = Page::keys(c.panel, scale, "3 keys picked", None, Some(None), None, &rows);
    assert!(p_set.field.is_none());
    assert_eq!(p_set.ease.as_ref().unwrap().1, None);
    assert!(p_set.verbs[0].rect.y < p.verbs[0].rect.y);
    // A row: the verbs alone; the graph: Paste here.
    let p_row = Page::keys(c.panel, scale, "Y2", None, None, None, &rows);
    assert!(p_row.field.is_none() && p_row.ease.is_none());
    assert_eq!(p_row.verbs.len(), 4);
    let air = home::rows(Target::Graph { at: 1.0 }, &e);
    assert_eq!(air.len(), 1);
    assert_eq!(air[0].label, "Paste here");
    assert!(!air[0].enabled);
    // The timeline's page: the grid switch, one segment per grid, lit on
    // the one in force, and Clear loop under it; the graph's air carries
    // the switch too.
    use crate::timeline::Grid;
    let tl = home::rows(Target::Timeline, &e);
    assert_eq!(tl.len(), 1);
    assert_eq!(tl[0].verb, Verb::ClearLoop);
    assert_eq!(home::title(Target::Timeline, &e), "Timeline");
    let p_tl = Page::keys(c.panel, scale, "Timeline", None, None, Some(Grid::Quarter.index()), &tl);
    let (seg, active) = p_tl.grid.as_ref().expect("the grid switch");
    assert_eq!(seg.segments.len(), Grid::ALL.len());
    assert_eq!(*active, 2);
    let r = seg.segments[0];
    assert_eq!(p_tl.hit(r.x + 5.0, r.y + 5.0), Some(Hit::Grid(0)));
    assert!(p_tl.verbs[0].rect.y > r.y + r.h, "Clear loop sits under the switch");
    let words: Vec<String> = p_tl.labels(None, None).iter().map(|l| l.text.clone()).collect();
    for w in Grid::LABELS {
        assert!(words.iter().any(|x| x == w), "missing {w}");
    }
    let p_air = Page::keys(c.panel, scale, "Bar 2.1", None, None, Some(0), &air);
    assert!(p_air.grid.is_some() && p_air.ease.is_none());
    // Home's object page is the same layout with nothing above the rows.
    let obj = Page::home(c.panel, scale, "circle", &home::rows(Target::Empty, &e));
    assert!(obj.field.is_none() && obj.ease.is_none() && obj.verbs.is_empty());
}

/// Every drawing tool has a rail button — the rail is sized by the tool
/// table, so a new tool can't have a key and a page and no button
/// (lightning did, 2026-09-02).
#[test]
fn every_tool_has_a_rail_button() {
    let c = build([1000.0, 600.0], 1.0, win());
    for tool in [
        Tool::Circle,
        Tool::Box,
        Tool::Polygon,
        Tool::Line,
        Tool::Stars,
        Tool::Bolt,
        Tool::Vortex,
    ] {
        assert!(
            c.rail.iter().any(|(t, _, _)| *t == tool),
            "{tool:?} has no rail button"
        );
    }
    assert_eq!(c.rail.len(), RAIL.len());
}

