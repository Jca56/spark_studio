//! Card-layout tests. The expanded card decides what it shows by asking the
//! shape what it has, so these pin down that a star field gets star controls
//! and nothing else does — and that nothing in the stack lands on top of
//! anything else, which nobody who can run these can check by looking.

use spark_render::Shape;

use super::*;

const SCALE: f32 = 1.5;
const X: f32 = 100.0;
const W: f32 = 400.0;

/// Build one card's detail block and hand back the block and the y it ended
/// at, the way `rows` uses it.
fn build(shape: &Shape) -> (CardDetail, f32) {
    let mut cy = 0.0;
    let d = detail(
        shape,
        &crate::fx::Stack::default(),
        &|_, _| false,
        CardTab::Settings,
        X,
        W,
        SCALE,
        0,
        &mut cy,
    );
    (d, cy)
}

fn labels(d: &CardDetail) -> Vec<&'static str> {
    d.sliders.iter().map(|s| s.label).collect()
}

fn field() -> Shape {
    Shape::stars([500.0, 400.0], [300.0, 200.0], 12.0)
}

/// The folder's disclosure sits in the corner a layer card puts its
/// cogwheel in — that corner is where "open this thing" lives — with the
/// eye to its left, and the name clear of both.
#[test]
fn the_folder_disclosure_takes_the_cogwheel_corner() {
    let mut e = crate::editor::Editor::empty();
    for k in 0..2 {
        e.push_shape(Shape::circle([k as f32 * 10.0, 0.0], 10.0));
    }
    e.select(Some(0));
    e.toggle_select(1);
    e.new_folder_from_selection();
    let panel = Viewport {
        x: 0.0,
        y: 0.0,
        w: 460.0,
        h: 2000.0,
    };
    let cards = rows(panel, SCALE, &e, None, CardTab::Settings, 0.0);
    let f = cards.folders.first().expect("the folder has a header");
    assert!(
        f.eye.x + f.eye.w <= f.disclose.x + 0.5,
        "the eye is not left of the disclosure"
    );
    assert!(
        f.disclose.x + f.disclose.w <= f.head.x + f.head.w + 0.5,
        "the disclosure escapes the header"
    );
    assert!(
        f.label_pos[0] + 20.0 <= f.eye.x,
        "the name runs into the buttons"
    );
    // And a layer card's cog sits in the same corner, which is the point.
    let cog = cards.rows[0].cog.expect("a loose card has a cog");
    assert!(
        (cog.x + cog.w - (f.disclose.x + f.disclose.w)).abs() < 1.0,
        "the two 'open' buttons don't share a column"
    );
}

/// A folder header and a layer card carry the same four fields, and now
/// behave the same way. They used to diverge: the folder's version drew its
/// label inside the box and never showed the buffer you were typing into.
#[test]
fn folder_fields_are_laid_out_like_a_card_s() {
    let mut e = crate::editor::Editor::empty();
    for k in 0..2 {
        e.push_shape(Shape::circle([k as f32 * 10.0, 0.0], 10.0));
    }
    e.select(Some(0));
    e.toggle_select(1);
    e.new_folder_from_selection();
    let panel = Viewport {
        x: 0.0,
        y: 0.0,
        w: 460.0,
        h: 2000.0,
    };
    let cards = rows(panel, SCALE, &e, None, CardTab::Settings, 0.0);
    let f = cards.folders.first().expect("the folder has a header");
    assert_eq!(f.scrubs.len(), 4, "X/Y/R/S");
    for sf in &f.scrubs {
        // The label sits left of the box, not inside it.
        assert!(
            sf.label_pos[0] + SCRUB_LABEL_W * SCALE <= sf.rect.x + 0.5,
            "{}: the label overlaps its box",
            sf.label
        );
        assert!(sf.rect.w > 0.0, "{}: no box left", sf.label);
        // And the focused-field lookup finds it, which is what makes the
        // caret and click-to-place work at all.
        let key = EditField::Folder(f.id, sf.prop);
        assert!(
            cards.focused_field(key).is_some(),
            "{}: the folder field can't be focused",
            sf.label
        );
    }
}

/// Every slider leaves room to its right for its readout, and the number
/// never overlaps the track it belongs to.
#[test]
fn sliders_reserve_a_column_for_their_readout() {
    let mut cy = 0.0;
    let shape = Shape::rect([0.0, 0.0], [50.0, 30.0]);
    let d = detail(
        &shape,
        &crate::fx::Stack::default(),
        &|_, _| false,
        CardTab::Settings,
        X,
        W,
        SCALE,
        0,
        &mut cy,
    );
    assert!(!d.sliders.is_empty(), "a box has sliders to check");
    for row in &d.sliders {
        assert!(
            row.value_right > row.track.x + row.track.w,
            "{}: the readout sits on top of the track",
            row.label
        );
        assert!(
            row.value_right <= X + W + 0.5,
            "{}: the readout runs off the card",
            row.label
        );
        assert!(row.track.w > 0.0, "{}: no track left", row.label);
    }
}

#[test]
fn a_star_field_card_carries_the_star_controls() {
    let (d, _) = build(&field());
    let got = labels(&d);
    for want in [
        "Width",
        "Height",
        "Density",
        "Star size",
        "Twinkle",
        "Twinkle speed",
        "Seed",
    ] {
        assert!(got.contains(&want), "no {want} slider on a field: {got:?}");
    }
    let form = d.form.expect("a field has no star-form picker");
    assert_eq!(form.options.len(), 3, "dot, sparkle, cross");
    assert_eq!(form.seg.segments.len(), 3, "a segment each");
    assert_eq!(form.active, 0, "a fresh field scatters dots");
}

/// Fill/Outline is meaningless on a field — and worse than meaningless,
/// since the number it would flip is the star size. It must not be offered.
#[test]
fn a_star_field_card_offers_no_fill_toggle() {
    let (d, _) = build(&field());
    assert!(d.style.is_none(), "a field was offered Fill/Outline");
    // The controls that do still apply are all there.
    assert!(d.form.is_some() && d.chips.is_none());
}

/// The other direction: a shape that isn't a field must not sprout star
/// knobs, or every card in the list would grow four dead sliders.
#[test]
fn other_kinds_get_no_star_controls() {
    for shape in [
        Shape::circle([100.0, 100.0], 40.0),
        Shape::rect([100.0, 100.0], [40.0, 20.0]),
        Shape::ngon([100.0, 100.0], 40.0, 6),
        Shape::line([0.0, 0.0], [50.0, 50.0], 3.0),
    ] {
        let (d, _) = build(&shape);
        let got = labels(&d);
        for star_only in ["Density", "Star size", "Twinkle", "Twinkle speed", "Seed"] {
            assert!(!got.contains(&star_only), "{star_only} leaked onto {got:?}");
        }
        assert!(d.form.is_none(), "star-form picker on a non-field");
    }
}

/// Every control in an expanded card stacks below the one before it and
/// finishes inside the height the card reserved. A field's card is the
/// tallest there is — nine sliders and three toggles — so if anything is
/// going to overlap or overrun, it does it here.
#[test]
fn the_expanded_card_stacks_without_overlapping() {
    let (d, end) = build(&field());
    let mut floor = 0.0f32;
    let mut check = |name: &str, top: f32, bottom: f32| {
        assert!(
            top >= floor - 0.01,
            "{name} starts at {top}, above the previous control's {floor}"
        );
        assert!(bottom <= end + 0.01, "{name} runs past the card's {end}");
        floor = bottom;
    };
    for row in &d.sliders {
        // The label sits at `label_pos`, the track below it.
        check(row.label, row.label_pos[1], row.track.y + row.track.h);
    }
    if let Some(f) = &d.form {
        let last = f.seg.segments.last().expect("segments");
        check("Star", f.label_pos[1], last.y + last.h);
    }
    for (name, t) in [("Blend", &d.blend), ("Gradient", &d.grad)]
        .into_iter()
        .filter_map(|(n, t)| t.as_ref().map(|t| (n, t)))
    {
        let last = t.seg.segments.last().expect("segments");
        check(name, t.label_pos[1], last.y + last.h);
    }
    assert!(end > 0.0, "the card reserved no height at all");
}

/// Slider positions are the value's place in its range — a fresh field's
/// twinkle sits partway along, not pinned at an end, which is what tells
/// you the range and the value agree about what the units are.
#[test]
fn star_sliders_sit_where_their_values_do() {
    let (d, _) = build(&field());
    for row in &d.sliders {
        assert!(
            (0.0..=1.0).contains(&row.t),
            "{} slider is at {}, outside its own track",
            row.label,
            row.t
        );
    }
    let twinkle = d
        .sliders
        .iter()
        .find(|r| r.label == "Twinkle")
        .expect("twinkle row");
    assert!(
        twinkle.t > 0.05 && twinkle.t < 0.95,
        "a fresh field's twinkle slider is pinned at {}",
        twinkle.t
    );
}

#[test]
fn a_field_gets_its_own_glyph_and_name() {
    let (icon, name) = kind_parts(spark_render::ShapeKind::Stars);
    assert_eq!(icon, spark_ui::ICON_STARS);
    assert_eq!(name, "stars");
    // ...and it isn't sharing one with another kind.
    for other in [
        spark_render::ShapeKind::Circle,
        spark_render::ShapeKind::Box,
        spark_render::ShapeKind::Ngon,
        spark_render::ShapeKind::Line,
        spark_render::ShapeKind::Path,
    ] {
        assert_ne!(kind_parts(other).0, icon, "{other:?} wears the star glyph");
    }
}
