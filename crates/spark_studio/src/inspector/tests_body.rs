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

/// An added effect gets its own section — Enabled, its settings, and a
/// red Remove — Glow excepted, which stays in Style; a gradient's colour
/// is a chip, React's amounts are sliders that move the effect's params;
/// removed, the section goes.
#[test]
fn an_added_effect_gets_a_section() {
    let mut e = Editor::empty();
    draw(&mut e, Tool::Circle, [300.0, 300.0], [360.0, 300.0]);
    e.set_glow_selection(20.0);
    let p = page(&e, 1.0, 0.0);
    assert_eq!(p.sections.len(), 2, "Glow is not a section");
    assert!(e.add_effect(EffectKind::Gradient));
    assert!(e.add_effect(EffectKind::React));
    let p = page(&e, 1.0, 0.0);
    let keys: Vec<SectionKey> = p.sections.iter().map(|s| s.key).collect();
    assert_eq!(
        keys,
        [
            SectionKey::Transform,
            SectionKey::Style,
            SectionKey::Effect(EffectKind::Gradient),
            SectionKey::Effect(EffectKind::React)
        ]
    );
    assert_eq!(p.sections[3].title, "R E A C T");
    let gid = e.fx_of(0).find_kind(EffectKind::Gradient).unwrap().id;
    let rid = e.fx_of(0).find_kind(EffectKind::React).unwrap().id;
    assert!(
        p.checks
            .iter()
            .any(|c| c.kind == page::CheckKind::EffectOn(gid) && c.on)
    );
    assert!(p.checks.iter().any(|c| c.kind == page::CheckKind::EffectOn(rid)));
    let removes: Vec<(page::ButtonKind, String)> =
        p.buttons.iter().map(|b| (b.kind, b.label.clone())).collect();
    assert!(removes.contains(&(page::ButtonKind::RemoveEffect(gid), "Remove Gradient".into())));
    assert!(removes.contains(&(page::ButtonKind::RemoveEffect(rid), "Remove React".into())));
    let b = &p.buttons[0];
    assert_eq!(p.hit(b.rect.x + 5.0, b.rect.y + 5.0), Some(Hit::Button(0)));
    assert!(
        p.labels(None, None)
            .iter()
            .any(|l| l.text == "Remove React" && l.color == spark_ui::theme().red)
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
    // React's three amounts are effect sliders at their defaults, 1 of
    // 20 — the ceiling is absurd on purpose, so the default sits low.
    let react: Vec<&page::SliderSlot> = p
        .sliders
        .iter()
        .filter(|s| matches!(s.target, SliderTarget::Effect { id, .. } if id == rid))
        .collect();
    assert_eq!(react.len(), 3);
    assert!((react[0].v - 0.05).abs() < 1e-5);
    assert_eq!(react[0].readout, "1.00");
    assert_eq!(react[0].range, (0.0, 20.0));
    // Turned off, the box empties; a set parameter reads back.
    assert!(e.toggle_effect(0, rid));
    assert!(e.set_effect_param(0, rid, 1, 0.25));
    let p = page(&e, 1.0, 0.0);
    assert!(
        p.checks
            .iter()
            .any(|c| c.kind == page::CheckKind::EffectOn(rid) && !c.on)
    );
    let amt = p
        .sliders
        .iter()
        .find(|s| s.target == SliderTarget::Effect { id: rid, param: 1 })
        .unwrap();
    assert_eq!(amt.readout, "0.25");
    assert_eq!(amt.label, "Glow");
    assert!(e.remove_effect(0, rid));
    let p = page(&e, 1.0, 0.0);
    assert_eq!(p.sections.len(), 3);
}
