//! The inspector's geometry, held by tests: nobody who can run these
//! can look at the window, so the panel is asserted, not eyeballed.

use super::popup::PopHit;
use super::*;
use crate::editor::Editor;
use crate::fx::EffectKind;
use crate::props::{SWATCH_COLS, SWATCH_ROWS, Tool};
use spark_render::LightKind;

fn panel() -> Viewport {
    // The 4K panel at 1.4: what the layout hands the inspector there.
    Viewport {
        x: 3000.0,
        y: 62.0,
        w: 620.0,
        h: 1460.0,
    }
}

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
        "{what} escapes: {inner:?} vs {outer:?}"
    );
}

fn draw(e: &mut Editor, tool: Tool, from: [f32; 2], to: [f32; 2]) -> usize {
    e.choose_tool(tool);
    e.set_cursor_canvas(from);
    e.mouse_down(false);
    e.set_cursor_canvas(to);
    e.mouse_up();
    e.choose_tool(Tool::Select);
    e.primary().expect("drawn")
}

fn page(e: &Editor, scale: f32, scroll: f32) -> Page {
    Page::build(panel(), scale, e, scroll, None, None)
}

/// Nothing selected: the colour section and nothing else — the pair,
/// the 8×4 grid, the rule under them, all inside the panel, the body
/// empty and starting under the rule.
#[test]
fn with_nothing_selected_the_panel_is_the_colour_section() {
    let e = Editor::empty();
    for scale in [1.0f32, 1.4] {
        let p = page(&e, scale, 0.0);
        assert!(p.title.is_none());
        assert!(p.fields.is_empty() && p.sliders.is_empty());
        assert!(p.switches.is_empty() && p.checks.is_empty());
        inside(panel(), p.fg, "foreground swatch");
        inside(panel(), p.bg, "background swatch");
        assert!(p.bg.x > p.fg.x && p.bg.y > p.fg.y, "the background sits under, down-right");
        assert_eq!(p.grid.len(), SWATCH_COLS * SWATCH_ROWS);
        for (i, c) in p.grid.iter().enumerate() {
            inside(panel(), *c, &format!("chip {i}"));
            assert!(c.x > p.bg.x + p.bg.w, "chip {i} runs into the pair");
            assert_eq!(p.hit(c.x + c.w * 0.5, c.y + c.h * 0.5), Some(Hit::Chip(i)));
        }
        // Eight across, four down: chips 0 and 7 share a row, 0 and 8 a column.
        assert!((p.grid[0].y - p.grid[7].y).abs() < 0.5);
        assert!((p.grid[0].x - p.grid[8].x).abs() < 0.5);
        inside(panel(), p.divider, "the rule");
        assert!(p.divider.y > p.grid[31].y + p.grid[31].h, "the rule is under the grid");
        assert!(p.body.y > p.divider.y, "the body starts under the rule");
        assert_eq!(p.max_scroll(), 0.0);
        assert!(p.labels(None, None).is_empty());
        assert_eq!(p.hit(p.fg.x + 3.0, p.fg.y + 3.0), Some(Hit::Fg));
        // The background's visible lip, past the foreground's edge.
        assert_eq!(p.hit(p.bg.x + p.bg.w - 3.0, p.bg.y + p.bg.h - 3.0), Some(Hit::Bg));
        assert_eq!(p.fg_rgb, e.color());
        assert_eq!(p.bg_rgb, e.color_b());
    }
}

/// Each kind gets its own controls, sliders in Alva's order — Sides,
/// Opacity, Brightness, Thickness, Glow: a circle has place, aim and size
/// fields (width and height too — its box is what an ellipse is),
/// Fill|Outline and Additive; a polygon leads with Sides; a light drops
/// spin, opacity and Additive for a kind switch, Intensity and a cone; a
/// star field its sky after.
#[test]
fn each_kind_gets_its_own_controls() {
    let mut e = Editor::empty();
    let props = |p: &Page| -> Vec<Prop> { p.fields.iter().map(|f| f.prop).collect() };
    let sliders = |p: &Page| -> Vec<Prop> { p.sliders.iter().map(|s| s.prop).collect() };

    draw(&mut e, Tool::Circle, [300.0, 300.0], [360.0, 300.0]);
    let p = page(&e, 1.0, 0.0);
    assert!(p.title.as_ref().is_some_and(|(t, ..)| t.starts_with("circle")));
    assert_eq!(
        props(&p),
        [
            Prop::X,
            Prop::Y,
            Prop::Z,
            Prop::Tilt,
            Prop::Turn,
            Prop::Rotation,
            Prop::Scale,
            Prop::Width,
            Prop::Height
        ]
    );
    assert_eq!(
        sliders(&p),
        [Prop::Opacity, Prop::Brightness, Prop::Thickness, Prop::Glow]
    );
    assert_eq!(p.switches.len(), 1);
    assert_eq!(p.switches[0].kind, page::SwitchKind::FillOutline);
    assert_eq!(p.switches[0].active, 1, "born an outline");
    assert_eq!(p.checks.len(), 1);
    assert!(!p.checks[0].on);

    draw(&mut e, Tool::Polygon, [600.0, 300.0], [700.0, 300.0]);
    let p = page(&e, 1.0, 0.0);
    assert_eq!(
        sliders(&p),
        [Prop::Sides, Prop::Opacity, Prop::Brightness, Prop::Thickness, Prop::Glow]
    );

    draw(&mut e, Tool::Box, [900.0, 300.0], [1000.0, 360.0]);
    let p = page(&e, 1.0, 0.0);
    assert!(!props(&p).contains(&Prop::Depth), "a box has no depth");

    draw(&mut e, Tool::Stars, [100.0, 600.0], [400.0, 800.0]);
    let p = page(&e, 1.0, 0.0);
    assert_eq!(p.switches[0].kind, page::SwitchKind::StarForm);
    assert!(sliders(&p).ends_with(&[Prop::Density, Prop::Twinkle, Prop::TwinkleRate]));
    assert!(p.sliders.iter().any(|s| s.prop == Prop::Thickness && s.label == "Size"));

    e.add_light(LightKind::Spot);
    let p = page(&e, 1.0, 0.0);
    assert!(!props(&p).contains(&Prop::Rotation), "a light is aimed, not spun");
    assert!(props(&p).contains(&Prop::Tilt) && props(&p).contains(&Prop::Turn));
    assert_eq!(p.switches[0].kind, page::SwitchKind::LightKind);
    assert_eq!(p.switches[0].active, 2);
    assert_eq!(sliders(&p), [Prop::Brightness, Prop::Cone]);
    assert_eq!(p.sliders[0].label, "Intensity");
    assert!(p.checks.is_empty(), "a light is already pure light");
    e.add_light(LightKind::Ambient);
    let p = page(&e, 1.0, 0.0);
    assert_eq!(sliders(&p), [Prop::Brightness, Prop::Rim]);
}

/// Every row of the transform strip reads red, green, blue left to
/// right, in the gizmo's own colours — X·Y·Z, Tilt·Turn·Rot (the rings
/// about X, Y, Z, so the colour is the ring's too), S·W·H. A light has
/// no spin, so its aim row is Tilt, Turn: red, green, and nothing blue.
#[test]
fn captions_run_red_green_blue_across_each_row() {
    use crate::gizmo::Axis;
    let mut e = Editor::empty();
    draw(&mut e, Tool::Box, [300.0, 300.0], [400.0, 360.0]);
    let rgb = |a: Axis| {
        let c = a.color();
        [c[0], c[1], c[2], 1.0]
    };
    let cols = [rgb(Axis::X), rgb(Axis::Y), rgb(Axis::Z)];
    // Every label by its text — no caption shares a text with a value.
    let colours = |p: &Page| -> Vec<(String, [f32; 4])> {
        p.labels(None, None)
            .iter()
            .map(|l| (l.text.clone(), l.color))
            .collect()
    };
    let p = page(&e, 1.0, 0.0);
    let got = colours(&p);
    let colour_of = |cap: &str| {
        got.iter()
            .find(|(t, _)| t == cap)
            .unwrap_or_else(|| panic!("no {cap} caption"))
            .1
    };
    for row in [["X", "Y", "Z"], ["Tilt", "Turn", "Rot"], ["S", "W", "H"]] {
        for (k, cap) in row.iter().enumerate() {
            assert_eq!(colour_of(cap), cols[k], "{cap} is not column {k}'s colour");
        }
    }
    // The strip's order is the row's, so the colours land where the eye
    // expects them.
    let order: Vec<Prop> = p.fields.iter().map(|f| f.prop).collect();
    assert_eq!(
        &order[3..6],
        &[Prop::Tilt, Prop::Turn, Prop::Rotation],
        "the aim row is Tilt, Turn, Rot"
    );
    // A light: Tilt, Turn only — red, green.
    e.add_light(LightKind::Spot);
    let p = page(&e, 1.0, 0.0);
    let got = colours(&p);
    let colour_of = |cap: &str| got.iter().find(|(t, _)| t == cap).map(|(_, c)| *c);
    assert_eq!(colour_of("Tilt"), Some(cols[0]));
    assert_eq!(colour_of("Turn"), Some(cols[1]));
    assert_eq!(colour_of("Rot"), None);
}

/// The transform strip rows up three across and every widget sits in
/// the body's window at both scales; a field's box hits it, a slider's
/// band hits it, a switch's segments hit in order, the checkbox row
/// hits, and the grid still hits above.
#[test]
fn the_body_lays_out_inside_the_panel_and_hits_what_it_draws() {
    let mut e = Editor::empty();
    draw(&mut e, Tool::Polygon, [600.0, 300.0], [700.0, 300.0]);
    for scale in [1.0f32, 1.4] {
        let p = page(&e, scale, 0.0);
        let tag = format!("scale {scale}");
        let row: Vec<&page::FieldSlot> = p.fields.iter().take(3).collect();
        assert!((row[0].rect.y - row[2].rect.y).abs() < 0.5, "{tag}: not a row");
        assert!(row[0].rect.x < row[1].rect.x && row[1].rect.x < row[2].rect.x);
        assert!(row[2].rect.x + row[2].rect.w <= panel().x + panel().w - page::PAD * scale + 0.5);
        for (k, f) in p.fields.iter().enumerate() {
            inside(p.body, f.rect, &format!("{tag}: field {k}"));
            assert_eq!(
                p.hit(f.rect.x + f.rect.w * 0.5, f.rect.y + f.rect.h * 0.5),
                Some(Hit::Field(k))
            );
        }
        for (k, sl) in p.sliders.iter().enumerate() {
            inside(p.body, sl.hit, &format!("{tag}: slider {k}"));
            assert!(sl.hit.h >= 30.0 * scale, "{tag}: a band too thin to grab");
            assert_eq!(p.hit(sl.hit.x + 3.0, sl.hit.y + 3.0), Some(Hit::Slider(k)));
            let thumb = Slider::rects(sl.track, sl.v)[2];
            let back = Slider::t_at(sl.track, thumb.pos[0] + thumb.size[0] * 0.5);
            assert!((back - sl.v).abs() < 1e-3, "{tag}: thumb off its value");
        }
        for (i, seg) in p.switches[0].seg.segments.iter().enumerate() {
            assert_eq!(
                p.hit(seg.x + seg.w * 0.5, seg.y + seg.h * 0.5),
                Some(Hit::Switch(0, i))
            );
        }
        let c = &p.checks[0].check;
        assert_eq!(p.hit(c.row.x + c.row.w - 10.0, c.row.y + c.row.h * 0.5), Some(Hit::Check(0)));
        let chip = p.grid[2];
        assert_eq!(
            p.hit(chip.x + chip.w * 0.5, chip.y + chip.h * 0.5),
            Some(Hit::Chip(2))
        );
        // Nothing overlaps: every body widget's band is below the last.
        let mut bottoms: Vec<(f32, f32)> = p
            .fields
            .iter()
            .map(|f| (f.rect.y, f.rect.y + f.rect.h))
            .chain(p.sliders.iter().map(|s| (s.hit.y, s.hit.y + s.hit.h)))
            .chain(p.switches.iter().map(|s| {
                let r = s.seg.segments[0];
                (r.y, r.y + r.h)
            }))
            .chain(p.checks.iter().map(|c| (c.check.row.y, c.check.row.y + c.check.row.h)))
            .collect();
        bottoms.sort_by(|a, b| a.0.total_cmp(&b.0));
        for w in bottoms.windows(2) {
            assert!(
                (w[0].0 - w[1].0).abs() < 0.5 || w[0].1 <= w[1].0 + 0.5,
                "{tag}: rows overlap at {:?}",
                w
            );
        }
    }
}

/// Scrolling moves the body up by exactly the scroll, a widget scrolled
/// out of the window neither hits nor labels, the scroll stops at the
/// content's end, and the pinned colour section never moves.
#[test]
fn scrolling_moves_the_body_and_hides_what_leaves_the_window() {
    let mut e = Editor::empty();
    draw(&mut e, Tool::Stars, [100.0, 600.0], [400.0, 800.0]);
    let p0 = page(&e, 1.4, 0.0);
    let short = Viewport {
        h: p0.body.y - panel().y + 300.0,
        ..panel()
    };
    let at = |scroll: f32| Page::build(short, 1.4, &e, scroll, None, None);
    let a = at(0.0);
    assert!(a.max_scroll() > 0.0, "a star field's page needs to scroll here");
    let b = at(100.0);
    assert!((a.fields[0].rect.y - b.fields[0].rect.y - 100.0).abs() < 0.5);
    assert!((a.sliders[0].hit.y - b.sliders[0].hit.y - 100.0).abs() < 0.5);
    let far = at(a.max_scroll());
    let f = far.fields[0].rect;
    assert!(f.y + f.h < far.body.y, "the first row should have scrolled out");
    assert_eq!(far.hit(f.x + 5.0, f.y + 5.0), None);
    assert!(
        !far.labels(None, None).iter().any(|l| l.text == "X"),
        "a scrolled-out caption was emitted"
    );
    let last = far.checks.last().map(|c| c.check.row).unwrap();
    assert!(last.y + last.h <= far.body.y + far.body.h + 0.5);
    assert_eq!(a.fg, far.fg);
    assert_eq!(a.grid[0], far.grid[0]);
}

/// A field being typed into shows the buffer, left-aligned from where
/// the caret table starts, instead of its centred number.
#[test]
fn an_edited_field_shows_its_buffer() {
    let mut e = Editor::empty();
    draw(&mut e, Tool::Circle, [300.0, 300.0], [360.0, 300.0]);
    let tb = TextBox::selecting_all("123");
    let p = Page::build(
        panel(),
        1.0,
        &e,
        0.0,
        Some(&(EditKey::Prop(Prop::Y), tb)),
        None,
    );
    let (slot, _) = p.edit.as_ref().expect("the edit found its field");
    assert_eq!(p.fields[*slot].prop, Prop::Y);
    let labels = p.labels(None, None);
    let buf = labels.iter().find(|l| l.text == "123").expect("the buffer is shown");
    assert_eq!(buf.align, crate::chrome::Align::Left);
}

/// The popup opens beside its swatch, inside the window, with the ×,
/// the picker, the hex field and three channel fields all hittable; its
/// fields print the swatch's colour the way Lantern Studio's do, and a
/// typed channel replaces exactly that channel.
#[test]
fn the_popup_sits_beside_its_swatch_and_prints_the_colour() {
    let e = Editor::empty();
    for scale in [1.0f32, 1.4] {
        let p = page(&e, scale, 0.0);
        let gold = {
            let c = spark_ui::srgb(0xFFC800);
            [c[0], c[1], c[2]]
        };
        let pop = popup::build(p.fg, win(), scale, Slot::Fg, gold, hsv_of(gold), None);
        inside(win(), pop.panel, "the popup");
        assert!(
            pop.panel.x + pop.panel.w <= p.fg.x,
            "scale {scale}: the popup covers the swatch it opened on"
        );
        assert_eq!(pop.hit(pop.close.x + 3.0, pop.close.y + 3.0), Some(PopHit::Close));
        assert_eq!(
            pop.hit(pop.picker.sv.x + 5.0, pop.picker.sv.y + 5.0),
            Some(PopHit::Sv)
        );
        assert_eq!(
            pop.hit(pop.picker.hue.x + 5.0, pop.picker.hue.y + 5.0),
            Some(PopHit::Hue)
        );
        assert_eq!(pop.hit(pop.hex.x + 5.0, pop.hex.y + 5.0), Some(PopHit::Hex));
        for k in 0..3 {
            let c = pop.chans[k];
            inside(pop.panel, c, "a channel field");
            assert_eq!(pop.hit(c.x + 5.0, c.y + 5.0), Some(PopHit::Chan(k)));
            assert!(c.w >= 50.0 * scale, "a channel field too narrow for 255");
        }
        let labels = pop.labels();
        assert!(labels.iter().any(|l| l.text == "Foreground"));
        assert!(labels.iter().any(|l| l.text == "ffc800"));
        assert!(labels.iter().any(|l| l.text == "255"));
        assert!(labels.iter().any(|l| l.text == "200"));
        assert!(labels.iter().any(|l| l.text == "0"));
        assert_eq!(popup::channels(gold), [255, 200, 0]);
        let red = with_channel(gold, 2, 255);
        assert_eq!(popup::channels(red), [255, 200, 255]);
        assert_eq!(popup::hex(red), "ffc8ff");
        // Background's popup opens on the background swatch, titled so.
        let pb = popup::build(p.bg, win(), scale, Slot::Bg, gold, hsv_of(gold), None);
        assert!(pb.labels().iter().any(|l| l.text == "Background"));
    }
}

/// The pair swaps without painting; the background paints a selected
/// shape's gradient end when it has one and nothing when it doesn't.
#[test]
fn the_pair_swaps_and_the_background_paints_a_gradient_end() {
    let mut e = Editor::empty();
    let i = draw(&mut e, Tool::Circle, [300.0, 300.0], [360.0, 300.0]);
    let (a, b) = (e.color(), e.color_b());
    assert_ne!(a, b);
    let before = e.shapes()[i].rgb();
    assert!(e.swap_colors());
    assert_eq!((e.color(), e.color_b()), (b, a));
    assert_eq!(e.shapes()[i].rgb(), before, "a swap painted the selection");
    // No gradient: the background paints nothing.
    assert!(!e.set_color_b([0.1, 0.2, 0.3]));
    assert_eq!(e.color_b(), [0.1, 0.2, 0.3]);
    assert_eq!(e.shapes()[i].rgb(), before);
    // With one: the effect's end colour takes it.
    assert!(e.add_effect(EffectKind::Gradient));
    assert!(e.set_color_b([0.4, 0.5, 0.6]));
    let (id, c) = e.colour_effect(i).expect("a gradient");
    let g = e.fx_of(i).find(id).unwrap();
    assert!((g.get(c as usize) - 0.4).abs() < 1e-5);
    assert!((g.get(c as usize + 2) - 0.6).abs() < 1e-5);
}

/// The colour round-trips through the popup's HSV closely enough that
/// a grid pick keeps its chip ring.
#[test]
fn the_picker_round_trips_the_grid() {
    for (i, &rgb) in swatch_grid().iter().enumerate() {
        let back = rgb_of(hsv_of(rgb));
        for c in 0..3 {
            assert!(
                (back[c] - rgb[c]).abs() < 2e-3,
                "chip {i} came back as {back:?}, was {rgb:?}"
            );
        }
    }
}
