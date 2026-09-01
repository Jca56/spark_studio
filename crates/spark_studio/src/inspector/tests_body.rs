//! The inspector body — sections, effects — held by tests. Split from
//! `tests` for the file budget; the rig is shared.

use super::page;
use super::tests::{draw, page as build_page, panel};
use super::*;
use crate::editor::Editor;
use crate::fx::EffectKind;
use crate::props::Tool;
use spark_render::LightKind;

fn page(e: &Editor, scale: f32, scroll: f32) -> Page {
    build_page(e, scale, scroll)
}

/// The body is sections that fold: a circle has Transform and Style;
/// folding Transform empties its fields and pulls Style up; the header
/// still hits either way; every open section draws its rule and insets
/// its content under it; a light's second section is Light.
#[test]
fn the_body_is_sections_that_fold() {
    let mut e = Editor::empty();
    draw(&mut e, Tool::Circle, [300.0, 300.0], [360.0, 300.0]);
    let open = page(&e, 1.0, 0.0);
    let keys: Vec<SectionKey> = open.sections.iter().map(|s| s.key).collect();
    assert_eq!(keys, [SectionKey::Transform, SectionKey::Style]);
    assert_eq!(open.sections[0].title, "T R A N S F O R M");
    assert!(open.sections.iter().all(|s| s.open && s.rule.h > 0.0));
    let h = open.sections[0].header;
    assert_eq!(open.hit(h.x + 5.0, h.y + 5.0), Some(Hit::Section(0)));
    assert!(!open.fields.is_empty());
    assert!(open.fields[0].rect.x > h.x, "content isn't inset");
    assert!(open.sections[0].rule.x < open.fields[0].rect.x, "the rule isn't left of the content");
    let shut = Page::build(panel(), 1.0, &e, 0.0, None, None, true, &[SectionKey::Transform]);
    assert!(shut.fields.is_empty(), "a folded section laid out its fields");
    assert!(!shut.sections[0].open && shut.sections[0].rule.h == 0.0);
    assert!(
        shut.sections[1].header.y < open.sections[1].header.y,
        "Style didn't climb"
    );
    let h = shut.sections[0].header;
    assert_eq!(shut.hit(h.x + 5.0, h.y + 5.0), Some(Hit::Section(0)));
    assert!(shut.labels(None, None).iter().any(|l| l.text == "S T Y L E"));
    e.add_light(LightKind::Sun);
    let p = page(&e, 1.0, 0.0);
    assert_eq!(p.sections[1].title, "L I G H T");
}

/// An effect added to the object becomes a section of its own — its
/// name spaced out, an Enabled box, its settings, a red Remove — and
/// goes when removed. A gradient's colour is a chip, not three sliders.
#[test]
fn an_added_effect_gets_a_section() {
    let mut e = Editor::empty();
    e.set_time(0.0);
    e.sync_to_time();
    e.choose_tool(crate::props::Tool::Circle);
    e.set_cursor_canvas([300.0, 300.0]);
    e.mouse_down(false);
    e.set_cursor_canvas([380.0, 300.0]);
    e.mouse_up();
    assert!(e.add_effect(EffectKind::Gradient));
    let p = page(&e, 1.0, 0.0);
    let keys: Vec<SectionKey> = p.sections.iter().map(|s| s.key).collect();
    assert_eq!(
        keys,
        [
            SectionKey::Transform,
            SectionKey::Style,
            SectionKey::Effect(EffectKind::Gradient),
        ]
    );
    assert_eq!(p.sections[2].title, "G R A D I E N T");
    let gid = e.fx_of(0).find_kind(EffectKind::Gradient).unwrap().id;
    assert!(
        p.checks
            .iter()
            .any(|c| c.kind == page::CheckKind::EffectOn(gid) && c.on)
    );
    let removes: Vec<(page::ButtonKind, String)> =
        p.buttons.iter().map(|b| (b.kind, b.label.clone())).collect();
    assert!(removes.contains(&(page::ButtonKind::RemoveEffect(gid), "Remove Gradient".into())));
    let b = &p.buttons[0];
    assert_eq!(p.hit(b.rect.x + 5.0, b.rect.y + 5.0), Some(Hit::Button(0)));
    assert!(
        p.labels(None, None)
            .iter()
            .any(|l| l.text == "Remove Gradient" && l.color == spark_ui::theme().red)
    );
    // The gradient's colour is a chip, not three sliders.
    assert_eq!(p.chips.len(), 1);
    assert_eq!(p.chips[0].id, gid);
    assert!(
        !p.sliders
            .iter()
            .any(|s| matches!(s.target, SliderTarget::Effect { id, .. } if id == gid))
    );
    let c = p.chips[0].rect;
    assert_eq!(p.hit(c.x + 3.0, c.y + 3.0), Some(Hit::FxChip(0)));
    // Turned off, the box empties; removed, the section goes.
    assert!(e.toggle_effect(0, gid));
    let p = page(&e, 1.0, 0.0);
    assert!(
        p.checks
            .iter()
            .any(|c| c.kind == page::CheckKind::EffectOn(gid) && !c.on)
    );
    assert!(e.remove_effect(0, gid));
    let p = page(&e, 1.0, 0.0);
    assert_eq!(p.sections.len(), 2);
}

/// A setting with a reaction on it wears the dot — on its field, or on
/// its slider, Glow's included once the effect exists.
#[test]
fn a_reacting_setting_wears_the_dot() {
    use crate::anim::Target;
    use crate::fx::{Reaction, Source};
    let mut e = Editor::empty();
    e.set_time(0.0);
    e.sync_to_time();
    e.choose_tool(crate::props::Tool::Circle);
    e.set_cursor_canvas([300.0, 300.0]);
    e.mouse_down(false);
    e.set_cursor_canvas([380.0, 300.0]);
    e.mouse_up();
    let p = page(&e, 1.0, 0.0);
    assert!(p.fields.iter().all(|f| !f.reacts) && p.sliders.iter().all(|s| !s.reacts));
    assert!(e.set_reaction(
        0,
        Reaction {
            target: Target::Shape(Prop::X),
            source: Source::Bass,
            amount: 0.5,
        }
    ));
    assert!(e.set_reaction(
        0,
        Reaction {
            target: Target::Shape(Prop::Opacity),
            source: Source::Onset,
            amount: 0.5,
        }
    ));
    assert!(e.set_glow_selection(30.0));
    let gid = e.fx_of(0).find_kind(EffectKind::Glow).unwrap().id;
    assert!(e.set_reaction(
        0,
        Reaction {
            target: Target::Effect { id: gid, param: 0 },
            source: Source::Mid,
            amount: 1.0,
        }
    ));
    let p = page(&e, 1.0, 0.0);
    let x = p.fields.iter().find(|f| f.prop == Prop::X).unwrap();
    let y = p.fields.iter().find(|f| f.prop == Prop::Y).unwrap();
    assert!(x.reacts && !y.reacts);
    let dots = |label: &str| p.sliders.iter().find(|s| s.label == label).unwrap().reacts;
    assert!(dots("Opacity") && dots("Glow") && !dots("Brightness"));
    // Off again: the dot goes; undo brings it back.
    assert!(e.remove_reaction(0, Target::Shape(Prop::X)));
    assert!(!page(&e, 1.0, 0.0).fields.iter().find(|f| f.prop == Prop::X).unwrap().reacts);
    e.undo();
    assert!(page(&e, 1.0, 0.0).fields.iter().find(|f| f.prop == Prop::X).unwrap().reacts);
}
