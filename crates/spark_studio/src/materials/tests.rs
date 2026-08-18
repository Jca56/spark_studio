//! Playground tests. The panel is geometry nobody who can run the tests can
//! look at, so the layout asserts itself: nothing overlapping, nothing
//! escaping the panel, and every control reachable.

use super::*;
use spark_ui::{default_theme, from_hex, hex_of};

/// The live skin is a global. Any test that swaps it has to hold this, or
/// two running at once restore each other's saved copy and both pass while
/// proving nothing. Poisoning is ignored on purpose: one test panicking
/// should fail that test, not every test after it.
pub static SKIN: std::sync::Mutex<()> = std::sync::Mutex::new(());

pub fn own_the_skin() -> std::sync::MutexGuard<'static, ()> {
    SKIN.lock().unwrap_or_else(|e| e.into_inner())
}

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
        assert!(inside(a, c.rect), "{} escapes the panel", c.name);
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
        assert!(inside(a, c.rect), "{} escapes a narrow panel", c.name);
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

/// [`MATERIALS`] and the two `nth` functions are kept in step by hand, so
/// every index has to address its own distinct surface. A duplicated match
/// arm would silently point two rows at one material — you would tune
/// "Timeline" and watch the status strip change.
#[test]
fn every_material_index_addresses_its_own_surface() {
    let base = Surfaces::from_theme(&default_theme());
    for (i, (name, ..)) in MATERIALS.iter().enumerate() {
        let mut m = base;
        // A radius no default carries, so the write is unmistakable.
        super::nth_mut(&mut m, i).radius = 99.0;
        assert_eq!(super::nth(&m, i).radius, 99.0, "{name} is unreadable");
        for (j, (other, ..)) in MATERIALS.iter().enumerate() {
            if i != j {
                assert_ne!(
                    super::nth(&m, j).radius,
                    99.0,
                    "{name} and {other} are the same surface"
                );
            }
        }
    }
}

/// The material list wraps into columns rather than running off the bottom
/// of a short panel. Rows must not land on top of each other whatever shape
/// the panel is dragged into.
#[test]
fn material_rows_wrap_without_overlapping() {
    for h in [200.0f32, 340.0, 700.0] {
        for scale in [1.0f32, 1.4] {
            let a = Viewport { h, ..area() };
            let p = build(a, scale, &state(Tab::Depth));
            assert_eq!(p.picks.len(), MATERIALS.len());
            for (i, x) in p.picks.iter().enumerate() {
                assert!(inside(a, *x), "{} escapes at h={h}", MATERIALS[i].0);
                for y in p.picks.iter().skip(i + 1) {
                    let apart = x.x + x.w <= y.x + 0.5
                        || y.x + y.w <= x.x + 0.5
                        || x.y + x.h <= y.y + 0.5
                        || y.y + y.h <= x.y + 0.5;
                    assert!(apart, "{} overlaps at h={h} scale {scale}", MATERIALS[i].0);
                }
            }
        }
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
        editing: Some((Edit::Slot(3), "1A2".into())),
    };
    let p = build(area(), 1.0, &st);
    let cell = p.cells.iter().find(|c| c.edit == Edit::Slot(3)).unwrap();
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
        if knob.is_switch() {
            continue;
        }
        let mut s = Surface::flat([0.5, 0.5, 0.5, 1.0], 12.0);
        let want = max * 0.5;
        set(&mut s, knob, want);
        let got = get(&s, knob);
        assert!((got - want).abs() < max * 0.01, "{label}: {want} -> {got}");
    }
}

/// A switch only ever reads back off or on, and the slider's whole lower
/// half has to mean off — a control that flips at 1% is a control nobody
/// can aim.
#[test]
fn a_switch_snaps_to_off_or_on() {
    for (knob, _, label, max) in KNOBS {
        if !knob.is_switch() {
            continue;
        }
        let mut s = Surface::flat([0.5, 0.5, 0.5, 1.0], 12.0);
        for (drive, want) in [(0.0, 0.0), (0.49, 0.0), (0.51, 1.0), (1.0, 1.0)] {
            set(&mut s, knob, drive * max);
            assert_eq!(get(&s, knob), want, "{label} at {drive}");
        }
    }
}

#[test]
fn an_invisible_end_colour_is_no_gradient() {
    let vp = || Viewport {
        x: 0.0,
        y: 0.0,
        w: 100.0,
        h: 40.0,
    };
    let base = Surface::flat([0.5, 0.5, 0.5, 1.0], 12.0);
    let armed = base.shade([0.1, 0.1, 0.1, 1.0]).rect(vp(), 1.0);
    assert_eq!(armed.grad[0], 1.0, "an end colour arms the gradient");
    // Zero means off here as everywhere: an end colour you cannot see is
    // not a gradient, which is what lets one control own the whole feature.
    let off = base.shade([0.1, 0.1, 0.1, 0.0]).rect(vp(), 1.0);
    assert_eq!(off.grad[0], 0.0, "alpha zero disarms it");
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

/// A gradient has to print back as the thing that made it: an auto-shade
/// as the *expression* that follows a later recolour, a hand-picked end
/// colour as the literal it can only be.
#[test]
fn the_recipe_prints_a_gradient_it_can_read_back() {
    let t = default_theme();

    let mut plain = Surfaces::from_theme(&t);
    plain.card.fill_to = spark_ui::srgb(0x101010);
    let out = recipe(&t, &plain);
    assert!(out.contains(".shade(srgb(0x101010))"), "opaque end:\n{out}");
    assert!(
        !out.contains(".toward(") && !out.contains(".radial("),
        "a plain top-to-bottom gradient needs neither:\n{out}"
    );

    let mut picked = Surfaces::from_theme(&t);
    picked.card.fill_to = spark_ui::srgba(0x4020FF80);
    set(&mut picked.card, Knob::GradAngle, 0.0);
    set(&mut picked.card, Knob::GradRadial, 1.0);
    let out = recipe(&t, &picked);
    assert!(out.contains(".shade(srgba(0x4020FF80))"), "picked:\n{out}");
    assert!(out.contains(".toward(0.000)"), "direction:\n{out}");
    assert!(out.contains(".radial(true)"), "radial:\n{out}");
}

/// A translucent palette entry has to print with the constructor that can
/// actually read its code back — eight digits through `srgb` would be a
/// recipe that doesn't compile.
#[test]
fn transparent_colours_print_their_constructor() {
    let mut t = default_theme();
    t.panel = spark_ui::srgba(0x151515C0);
    let out = recipe(&t, &Surfaces::from_theme(&t));
    assert!(out.contains("srgba(0x151515C0)"), "transparent:\n{out}");
    assert!(
        out.contains(&format!("srgb(0x{})", hex_of(t.card))),
        "opaque:\n{out}"
    );
}

/// The gradient's far end is a colour you type, so the Depth tab has to
/// offer a field for it — and that field has to sit clear of the knobs it
/// shares a column with.
#[test]
fn the_depth_tab_offers_an_end_colour() {
    for scale in [1.0f32, 1.4] {
        let st = State {
            tab: Tab::Depth,
            pick: 0,
            editing: None,
        };
        let p = build(area(), scale, &st);
        let cell = p
            .cells
            .iter()
            .find(|c| c.edit == Edit::GradEnd)
            .unwrap_or_else(|| panic!("scale {scale}: no end colour field"));
        assert!(cell.field.w > 0.0 && cell.swatch.w > 0.0);
        for row in &p.rows {
            let clear = cell.rect.y + cell.rect.h <= row.label_pos[1] + 0.5
                || row.track.y + row.track.h <= cell.rect.y + 0.5
                || cell.rect.x + cell.rect.w <= row.track.x + 0.5
                || row.track.x + row.track.w <= cell.rect.x + 0.5;
            assert!(clear, "scale {scale}: end colour overlaps {}", row.label);
        }
    }
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
/// Touches the live skin, so it takes the lock and puts everything back.
#[test]
fn a_recolour_keeps_the_depth() {
    let _skin = own_the_skin();
    let start = spark_ui::surfaces();

    let mut tuned = Surfaces::from_theme(&default_theme());
    tuned.card.shadow = [3.0, 12.0, 0.4];
    tuned.card.bevel = [0.25, 0.1, 4.0];
    tuned.well.inner = [2.0, 6.0, 0.5];
    let picked = spark_ui::srgba(0x4020FF80);
    tuned.plate.fill_to = picked;
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
    // A gradient's far end is a colour somebody chose, so recolouring the
    // *fill* leaves it exactly where it was put.
    assert_eq!(after.plate.fill_to, picked, "the end colour survived");

    spark_ui::set_theme(default_theme());
    spark_ui::set_surfaces(start);
}
