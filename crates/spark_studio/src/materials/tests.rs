//! Playground tests. The panel is geometry Claude cannot look at, so
//! the layout asserts itself: nothing overlapping, nothing escaping the
//! panel, and every knob surviving a round trip through its surface.

use super::*;

fn base() -> Surface {
    Surface::flat([0.5, 0.5, 0.5, 1.0], 12.0)
}

/// A left panel roughly the shape the real one takes at 1.0 scale.
fn area() -> Viewport {
    Viewport {
        x: 0.0,
        y: 108.0,
        w: 380.0,
        h: 900.0,
    }
}

/// Nothing here is visible to Claude, so the geometry has to assert
/// itself: every control inside its panel, nothing stacked on anything.
#[test]
fn the_panel_lays_out_without_overlaps() {
    let a = area();
    let p = build(a, 1.0, 0, 0.0);
    assert_eq!(p.chips.len(), MATERIALS.len());
    assert_eq!(p.rows.len(), KNOBS.len());
    for (i, c) in p.chips.iter().enumerate() {
        assert!(
            c.x >= a.x && c.x + c.w <= a.x + a.w,
            "chip {i} escapes wide"
        );
    }
    // Chips are laid two to a row; neighbours must not touch.
    for (i, c) in p.chips.iter().enumerate() {
        for (j, d) in p.chips.iter().enumerate().skip(i + 1) {
            let apart = c.x + c.w <= d.x || d.x + d.w <= c.x || c.y + c.h <= d.y;
            assert!(apart, "chips {i} and {j} overlap");
        }
    }
    let lowest_chip = p.chips.iter().fold(0.0f32, |m, c| m.max(c.y + c.h));
    assert!(p.rows[0].label_pos[1] >= lowest_chip, "rows run into chips");
    for pair in p.rows.windows(2) {
        let (a, b) = (&pair[0], &pair[1]);
        assert!(
            a.track.y + a.track.h < b.label_pos[1],
            "{} runs into {}",
            a.label,
            b.label
        );
        assert!(
            !on_track(a.track, b.track.x, b.track.y, 1.0),
            "grab boxes touch"
        );
    }
    let last = p.rows.last().unwrap();
    assert!(
        last.track.y + last.track.h < p.print.y,
        "rows hit the buttons"
    );
    assert!(p.print.x + p.print.w <= p.reset.x, "buttons overlap");
    assert!(
        p.reset.x + p.reset.w <= a.x + a.w,
        "reset button escapes the panel"
    );
}

/// `content_h` drives scroll clamping, so it has to actually cover the
/// last thing laid out — otherwise the buttons become unreachable.
#[test]
fn content_height_covers_the_buttons() {
    let a = area();
    let p = build(a, 1.0, 0, 0.0);
    assert!(
        p.content_h >= p.reset.y + p.reset.h - a.y,
        "content_h {} misses the buttons at {}",
        p.content_h,
        p.reset.y + p.reset.h - a.y
    );
}

/// Scrolling moves everything by exactly the scroll, and changes nothing
/// else — the hit tests rebuild the layout at the current scroll, so a
/// drift here would mis-aim every click.
#[test]
fn scrolling_shifts_the_whole_panel() {
    let a = area();
    let (top, down) = (build(a, 1.0, 0, 0.0), build(a, 1.0, 0, 200.0));
    assert_eq!(down.chips[0].y, top.chips[0].y - 200.0);
    assert_eq!(down.rows[5].track.y, top.rows[5].track.y - 200.0);
    assert_eq!(down.print.y, top.print.y - 200.0);
    assert_eq!(down.chips[0].x, top.chips[0].x, "no sideways drift");
    assert_eq!(down.content_h, top.content_h, "height is scroll-invariant");
}

/// The picker has to reach every material, and each index has to map to
/// the one it names.
#[test]
fn every_material_is_reachable_by_its_index() {
    let m = Surfaces::from_theme(&spark_ui::default_theme());
    let by_index: Vec<Surface> = (0..MATERIALS.len()).map(|i| nth(&m, i)).collect();
    for (want, got) in [m.card, m.header, m.plate, m.well, m.float, m.field, m.hover]
        .into_iter()
        .zip(&by_index)
    {
        assert_eq!(&want, got);
    }
    // ...and the mutable path must land on the same one.
    let mut m2 = m;
    for i in 0..MATERIALS.len() {
        nth_mut(&mut m2, i).radius = 99.0;
        assert_eq!(nth(&m2, i).radius, 99.0, "index {i} writes elsewhere");
    }
}

/// Every knob has to survive a round trip, or dragging a slider would
/// snap it somewhere else the moment the panel relaid out.
#[test]
fn every_knob_round_trips() {
    for (knob, label, max) in KNOBS {
        let mut s = base();
        let want = max * 0.5;
        set(&mut s, knob, want);
        let got = get(&s, knob);
        assert!(
            (got - want).abs() < max * 0.01,
            "{label}: set {want}, read back {got}"
        );
    }
}

/// Shade is the one knob with no field of its own — it lives in the
/// distance between two colors, so zero has to mean "no gradient".
#[test]
fn zero_shade_turns_the_gradient_off() {
    let mut s = base();
    set(&mut s, Knob::Shade, 0.7);
    assert!(s.fill_to[3] > 0.0, "shading arms the gradient");
    set(&mut s, Knob::Shade, 0.0);
    assert_eq!(s.fill_to, [0.0; 4], "and zero disarms it");
    assert_eq!(get(&s, Knob::Shade), 0.0);
}

#[test]
fn shade_darkens_rather_than_lightens() {
    let mut s = base();
    set(&mut s, Knob::Shade, 1.0);
    assert!(s.fill_to[0] < s.fill[0], "the far end is darker");
    assert!(s.fill_to[0] > 0.0, "but never all the way to black");
}

/// The recipe is pasted straight into `surface.rs`, so it has to name
/// palette fields rather than bake literals — otherwise a printed theme
/// would stop following a recolor.
#[test]
fn the_recipe_emits_palette_expressions() {
    let mut m = Surfaces::from_theme(&spark_ui::default_theme());
    nth_mut(&mut m, 0).shadow = [2.0, 10.0, 0.5];
    let out = recipe(&m);
    assert!(out.contains("card: Surface::flat(t.card, 12.0)"), "{out}");
    assert!(out.contains(".edge(2.5, t.card_border)"), "{out}");
    assert!(out.contains(".raised(2.0, 10.0, 0.50)"), "{out}");
    assert!(!out.contains('['), "no colour literals leaked in:\n{out}");
}

/// A knob left at zero must not print — the recipe should read as the
/// short list of things that were actually changed.
#[test]
fn untouched_knobs_stay_out_of_the_recipe() {
    let m = Surfaces::from_theme(&spark_ui::default_theme());
    let out = recipe(&m);
    for absent in [".lit(", ".raised(", ".recessed(", ".textured(", ".shade("] {
        assert!(
            !out.contains(absent),
            "{absent} in a default recipe:\n{out}"
        );
    }
}

/// Every material must be able to grow a border, or the Border knob is
/// a dead slider on the two that ship without one.
#[test]
fn a_borderless_material_can_still_grow_a_border() {
    let t = spark_ui::default_theme();
    let mut m = Surfaces::from_theme(&t);
    assert_eq!(m.well.border, 0.0, "ships borderless");
    assert_ne!(m.well.border_color[3], 0.0, "but has ink ready");
    m.well.border = 4.0;
    assert_eq!(
        m.well
            .rect(
                Viewport {
                    x: 0.0,
                    y: 0.0,
                    w: 40.0,
                    h: 20.0
                },
                1.0
            )
            .edge[0],
        4.0,
        "and it reaches the renderer"
    );
    assert!(
        recipe(&m).contains(".edge(4.0, t.card_border)"),
        "and prints"
    );
}
