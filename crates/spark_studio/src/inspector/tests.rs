//! The inspector's geometry, held by tests: nobody who can run these
//! can look at the window, so the panel is asserted, not eyeballed.

use super::*;
use crate::editor::Editor;
use crate::props::Tool;
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
    Page::build(panel(), scale, e, [0.5, 1.0, 1.0], scroll, None)
}

/// Nothing selected: the colour home and nothing else, inside the
/// panel, with the body empty.
#[test]
fn with_nothing_selected_the_panel_is_the_colour_home() {
    let e = Editor::empty();
    for scale in [1.0f32, 1.4] {
        let p = page(&e, scale, 0.0);
        assert!(p.title.is_none());
        assert!(p.fields.is_empty() && p.sliders.is_empty());
        assert!(p.switches.is_empty() && p.checks.is_empty());
        inside(panel(), p.picker.sv, "picker");
        inside(panel(), p.picker.hue, "hue bar");
        let chip = p.chips.rects(&PALETTE, None)[0];
        assert!(chip.pos[1] >= panel().y, "chips above the panel");
        assert!(p.body.y > p.picker.sv.y + p.picker.sv.h, "the body starts under the picker");
        assert_eq!(p.max_scroll(), 0.0);
        assert!(p.labels(None, None).is_empty());
        assert_eq!(p.hit(p.picker.sv.x + 5.0, p.picker.sv.y + 5.0), Some(Hit::Sv));
        assert_eq!(p.hit(p.picker.hue.x + 5.0, p.picker.hue.y + 5.0), Some(Hit::Hue));
    }
}

/// Each kind gets its own controls: a circle has place, aim and size
/// fields (width and height too — its box is what an ellipse is),
/// Fill|Outline, thickness, glow, brightness, opacity and Additive; a
/// polygon adds Sides; a light drops spin, opacity and Additive for a
/// kind switch, Intensity and a cone; a star field its sky.
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
            Prop::Rotation,
            Prop::Tilt,
            Prop::Turn,
            Prop::Scale,
            Prop::Width,
            Prop::Height
        ]
    );
    assert_eq!(
        sliders(&p),
        [Prop::Thickness, Prop::Glow, Prop::Brightness, Prop::Opacity]
    );
    assert_eq!(p.switches.len(), 1);
    assert_eq!(p.switches[0].kind, page::SwitchKind::FillOutline);
    assert_eq!(p.switches[0].active, 1, "born an outline");
    assert_eq!(p.checks.len(), 1);
    assert!(!p.checks[0].on);

    draw(&mut e, Tool::Polygon, [600.0, 300.0], [700.0, 300.0]);
    let p = page(&e, 1.0, 0.0);
    assert_eq!(sliders(&p)[0], Prop::Sides);

    draw(&mut e, Tool::Box, [900.0, 300.0], [1000.0, 360.0]);
    let p = page(&e, 1.0, 0.0);
    assert!(props(&p).contains(&Prop::Width) && props(&p).contains(&Prop::Height));
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

/// The transform strip rows up three across and every widget sits in
/// the body's window at both scales; a field's box hits it, a slider's
/// band hits it, a switch's segments hit in order, the checkbox row
/// hits, and the chips still hit above.
#[test]
fn the_body_lays_out_inside_the_panel_and_hits_what_it_draws() {
    let mut e = Editor::empty();
    draw(&mut e, Tool::Polygon, [600.0, 300.0], [700.0, 300.0]);
    for scale in [1.0f32, 1.4] {
        let p = page(&e, scale, 0.0);
        let tag = format!("scale {scale}");
        // Three across on the first row, at one height.
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
        let chip = p.chips.rects(&PALETTE, None)[2];
        assert_eq!(
            p.hit(chip.pos[0] + chip.size[0] * 0.5, chip.pos[1] + chip.size[1] * 0.5),
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
            // Fields in one row share a top; anything else stacks.
            assert!(
                (w[0].0 - w[1].0).abs() < 0.5 || w[0].1 <= w[1].0 + 0.5,
                "{tag}: rows overlap at {:?}",
                w
            );
        }
    }
}

/// Scrolling moves the body up by exactly the scroll, a widget scrolled
/// out of the window neither hits nor labels, and the scroll stops at
/// the content's end.
#[test]
fn scrolling_moves_the_body_and_hides_what_leaves_the_window() {
    let mut e = Editor::empty();
    draw(&mut e, Tool::Stars, [100.0, 600.0], [400.0, 800.0]);
    let p0 = page(&e, 1.4, 0.0);
    let short = Viewport {
        h: p0.body.y - panel().y + 300.0,
        ..panel()
    };
    let at = |scroll: f32| Page::build(short, 1.4, &e, [0.0; 3], scroll, None);
    let a = at(0.0);
    assert!(a.max_scroll() > 0.0, "a star field's page needs to scroll here");
    let b = at(100.0);
    assert!((a.fields[0].rect.y - b.fields[0].rect.y - 100.0).abs() < 0.5);
    assert!((a.sliders[0].hit.y - b.sliders[0].hit.y - 100.0).abs() < 0.5);
    // Scrolled far enough, the first field row leaves the window.
    let far = at(a.max_scroll());
    let f = far.fields[0].rect;
    assert!(f.y + f.h < far.body.y, "the first row should have scrolled out");
    assert_eq!(far.hit(f.x + 5.0, f.y + 5.0), None);
    assert!(
        !far.labels(None, None).iter().any(|l| l.text == "X"),
        "a scrolled-out caption was emitted"
    );
    // The last widget's bottom lands inside the window at max scroll.
    let last = far.checks.last().map(|c| c.check.row).unwrap();
    assert!(last.y + last.h <= far.body.y + far.body.h + 0.5);
    // The pinned colour home never moves.
    assert_eq!(a.picker.sv, far.picker.sv);
}

/// A field being typed into shows the buffer, left-aligned from where
/// the caret table starts, instead of its centred number.
#[test]
fn an_edited_field_shows_its_buffer() {
    let mut e = Editor::empty();
    draw(&mut e, Tool::Circle, [300.0, 300.0], [360.0, 300.0]);
    let tb = TextBox::selecting_all("123");
    let p = Page::build(panel(), 1.0, &e, [0.0; 3], 0.0, Some(&(Prop::Y, tb)));
    let (slot, _) = p.edit.as_ref().expect("the edit found its field");
    assert_eq!(p.fields[*slot].prop, Prop::Y);
    let labels = p.labels(None, None);
    let buf = labels.iter().find(|l| l.text == "123").expect("the buffer is shown");
    assert_eq!(buf.align, crate::chrome::Align::Left);
    assert!(!labels.iter().any(|l| l.text == "300" && l.pos[0] > buf.pos[0] && l.pos[1] == buf.pos[1]));
}

/// The colour round-trips through the picker's HSV closely enough that
/// a palette pick keeps its chip ring.
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
    }
}
