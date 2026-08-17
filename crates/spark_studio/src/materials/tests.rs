//! Playground tests. The panel is geometry nobody who can run the tests can
//! look at, so the layout asserts itself: nothing overlapping, nothing
//! escaping the panel, and every control reachable.

use super::*;
use spark_ui::{default_theme, from_hex, hex_of};

/// A bottom panel roughly the shape the real one takes.
fn area() -> Viewport {
    Viewport {
        x: 0.0,
        y: 700.0,
        w: 1900.0,
        h: 320.0,
    }
}

fn state(tab: Tab) -> State {
    State {
        tab,
        pick: 0,
        editing: None,
    }
}

fn inside(area: Viewport, v: Viewport) -> bool {
    v.x >= area.x - 0.5
        && v.y >= area.y - 0.5
        && v.x + v.w <= area.x + area.w + 0.5
        && v.y + v.h <= area.y + area.h + 0.5
}

/// Every colour has to be reachable, and every cell has to land inside the
/// panel. The first version ran its controls clean off the edge.
#[test]
fn the_colour_grid_fits_inside_the_panel() {
    let a = area();
    let p = build(a, 1.0, &state(Tab::Colors));
    assert_eq!(p.cells.len(), SLOTS.len(), "every colour is shown");
    for c in &p.cells {
        assert!(
            inside(a, c.rect),
            "{} escapes the panel",
            SLOTS[c.slot].label
        );
        assert!(inside(a, c.swatch), "swatch escapes");
        assert!(c.swatch.x + c.swatch.w <= c.hex_pos[0], "swatch over hex");
    }
    for v in [p.print, p.reset, p.tabs[0], p.tabs[1]] {
        assert!(inside(a, v), "a control escapes the panel");
    }
}

/// Cells must not stack on each other, or clicking one would edit another.
#[test]
fn colour_cells_never_overlap() {
    let p = build(area(), 1.0, &state(Tab::Colors));
    for (i, a) in p.cells.iter().enumerate() {
        for (j, b) in p.cells.iter().enumerate().skip(i + 1) {
            let apart = a.rect.x + a.rect.w <= b.rect.x + 0.5
                || b.rect.x + b.rect.w <= a.rect.x + 0.5
                || a.rect.y + a.rect.h <= b.rect.y + 0.5
                || b.rect.y + b.rect.h <= a.rect.y + 0.5;
            assert!(apart, "{i} and {j} overlap");
        }
    }
}

/// The tab strip and the buttons share the top row; they must not collide.
#[test]
fn the_top_strip_does_not_collide() {
    let p = build(area(), 1.0, &state(Tab::Colors));
    assert!(p.tabs[0].x + p.tabs[0].w <= p.tabs[1].x, "tabs overlap");
    assert!(
        p.tabs[1].x + p.tabs[1].w <= p.print.x,
        "tabs hit the buttons"
    );
    assert!(p.print.x + p.print.w <= p.reset.x, "buttons overlap");
}

/// A narrow window must still fit everything — the grid trades column width
/// for column count rather than running off the end.
#[test]
fn a_narrow_panel_still_fits_every_colour() {
    let a = Viewport {
        x: 0.0,
        y: 0.0,
        w: 900.0,
        h: 260.0,
    };
    let p = build(a, 1.0, &state(Tab::Colors));
    assert_eq!(p.cells.len(), SLOTS.len());
    for c in &p.cells {
        assert!(
            inside(a, c.rect),
            "{} escapes a narrow panel",
            SLOTS[c.slot].label
        );
    }
}

#[test]
fn the_depth_tab_shows_every_material_and_knob() {
    let a = area();
    let p = build(a, 1.0, &state(Tab::Depth));
    assert_eq!(p.picks.len(), MATERIALS.len());
    assert_eq!(p.rows.len(), KNOBS.len());
    for v in &p.picks {
        assert!(inside(a, *v), "a material row escapes the panel");
    }
    for r in &p.rows {
        assert!(inside(a, r.track), "{} escapes the panel", r.label);
    }
}

/// Knob tracks must not sit on top of each other, or a drag would grab the
/// wrong one.
#[test]
fn knob_tracks_stay_apart() {
    let p = build(area(), 1.0, &state(Tab::Depth));
    for (i, a) in p.rows.iter().enumerate() {
        for b in p.rows.iter().skip(i + 1) {
            let apart = a.track.x + a.track.w <= b.track.x + 0.5
                || b.track.x + b.track.w <= a.track.x + 0.5
                || a.track.y + a.track.h <= b.track.y - 8.0
                || b.track.y + b.track.h <= a.track.y - 8.0;
            assert!(apart, "{} and {} overlap", a.label, b.label);
        }
    }
}

/// Every slot has to read and write the field it names, or the grid would
/// quietly edit the wrong colour.
#[test]
fn every_slot_reads_back_what_it_writes() {
    let mark = spark_ui::srgb(0x123456);
    for s in SLOTS {
        let mut t = default_theme();
        (s.set)(&mut t, mark);
        assert_eq!((s.get)(&t), mark, "{} does not round trip", s.label);
    }
}

/// Two slots pointing at the same field would make one of them a decoy.
#[test]
fn no_two_slots_share_a_field() {
    let mark = spark_ui::srgb(0x123456);
    for (i, a) in SLOTS.iter().enumerate() {
        let mut t = default_theme();
        (a.set)(&mut t, mark);
        for (j, b) in SLOTS.iter().enumerate() {
            if i != j {
                assert_ne!(
                    (b.get)(&t),
                    mark,
                    "{} and {} write the same field",
                    a.label,
                    b.label
                );
            }
        }
    }
}

/// The cell shows the code you would type to reproduce it.
#[test]
fn cells_show_a_code_that_parses_back() {
    let p = build(area(), 1.0, &state(Tab::Colors));
    for c in &p.cells {
        assert_eq!(from_hex(&c.hex), Some(c.color), "{} is unreadable", c.hex);
    }
}

/// While a cell is being typed into it shows the buffer, not the colour —
/// otherwise a half-typed code would be overwritten every frame.
#[test]
fn an_edited_cell_shows_what_is_being_typed() {
    let st = State {
        tab: Tab::Colors,
        pick: 0,
        editing: Some((3, "1A2".into())),
    };
    let p = build(area(), 1.0, &st);
    let cell = p.cells.iter().find(|c| c.slot == 3).unwrap();
    assert_eq!(cell.hex, "1A2");
    assert!(cell.editing);
    assert!(
        p.cells.iter().filter(|c| c.editing).count() == 1,
        "one at a time"
    );
}

#[test]
fn every_knob_round_trips() {
    for (knob, _, label, max) in KNOBS {
        let mut s = Surface::flat([0.5, 0.5, 0.5, 1.0], 12.0);
        let want = max * 0.5;
        set(&mut s, knob, want);
        let got = get(&s, knob);
        assert!((got - want).abs() < max * 0.01, "{label}: {want} -> {got}");
    }
}

#[test]
fn zero_shade_turns_the_gradient_off() {
    let mut s = Surface::flat([0.5, 0.5, 0.5, 1.0], 12.0);
    set(&mut s, Knob::Shade, 0.7);
    assert!(s.fill_to[3] > 0.0, "shading arms the gradient");
    set(&mut s, Knob::Shade, 0.0);
    assert_eq!(s.fill_to, [0.0; 4], "and zero disarms it");
}

/// The recipe is pasted into source, so it has to name palette fields for
/// materials and print real codes for colours.
#[test]
fn the_recipe_carries_both_halves() {
    let t = default_theme();
    let mut m = Surfaces::from_theme(&t);
    m.card.shadow = [2.0, 10.0, 0.5];
    let out = recipe(&t, &m);
    assert!(
        out.contains(&format!("0x{}", hex_of(t.panel))),
        "colours:\n{out}"
    );
    assert!(out.contains("Side panels"), "colour names:\n{out}");
    assert!(out.contains("card: Surface::flat(t.card, 12.0)"), "{out}");
    assert!(out.contains(".raised(2.0, 10.0, 0.50)"), "{out}");
}

/// Material names in the panel are what you can point at on screen; the
/// code names only appear in a printed recipe.
#[test]
fn materials_are_named_for_what_you_see() {
    for (shown, field, ..) in MATERIALS {
        assert!(
            shown.chars().next().is_some_and(|c| c.is_uppercase()),
            "{shown} should read as a name"
        );
        assert_ne!(shown, field, "{field} still shows its code name");
    }
}

/// Editing a colour rederives every material from the palette, which is how
/// a recolour reaches the borders. Depth dialled in on the other tab has to
/// survive that, or the two tabs would quietly undo each other.
///
/// The only test here that touches the live skin, so it owns it and puts it
/// back.
#[test]
fn a_recolour_keeps_the_depth() {
    let start = spark_ui::surfaces();

    let mut tuned = Surfaces::from_theme(&default_theme());
    tuned.card.shadow = [3.0, 12.0, 0.4];
    tuned.card.bevel = [0.25, 0.1, 4.0];
    tuned.well.inner = [2.0, 6.0, 0.5];
    spark_ui::set_surfaces(tuned);

    // Recolour the way `apply_hex` does.
    let mut t = default_theme();
    t.card = spark_ui::srgb(0x445566);
    let depth = spark_ui::surfaces();
    spark_ui::set_theme(t);
    super::input::carry_depth(&depth);

    let after = spark_ui::surfaces();
    assert_eq!(after.card.fill, spark_ui::srgb(0x445566), "colour landed");
    assert_eq!(after.card.shadow, [3.0, 12.0, 0.4], "shadow survived");
    assert_eq!(after.card.bevel, [0.25, 0.1, 4.0], "bevel survived");
    assert_eq!(after.well.inner, [2.0, 6.0, 0.5], "inner shadow survived");

    spark_ui::set_theme(default_theme());
    spark_ui::set_surfaces(start);
}
