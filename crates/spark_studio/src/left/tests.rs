//! The left panel's geometry, held by tests.

use super::*;
use crate::fx::KINDS;

fn panel() -> Viewport {
    Viewport {
        x: 0.0,
        y: 62.0,
        w: 540.0,
        h: 1300.0,
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

/// The Effects tab lists every kind but Glow, each under its group, in
/// rows inside a card inside the panel, with a rule between rows of a
/// group and none after its last; the tab strip sits above the card and
/// every row and tab hits.
#[test]
fn the_effects_tab_lists_every_kind_but_glow() {
    for scale in [1.0f32, 1.4] {
        let p = Page::build(panel(), scale, Tab::Effects);
        let tag = format!("scale {scale}");
        assert_eq!(p.tabs.len(), Tab::ALL.len());
        inside(panel(), p.card, &format!("{tag}: the card"));
        assert!(p.tabs[0].1.y + p.tabs[0].1.h <= p.card.y, "{tag}: the strip overlaps the card");
        assert_eq!(p.hit(p.tabs[0].1.x + 5.0, p.tabs[0].1.y + 5.0), Some(Hit::Tab(0)));
        let kinds: Vec<EffectKind> = p.rows.iter().map(|r| r.kind).collect();
        assert!(!kinds.contains(&EffectKind::Glow), "{tag}: Glow is Style's");
        for k in KINDS {
            assert_eq!(kinds.contains(&k), k != EffectKind::Glow, "{tag}: {k:?}");
        }
        for (k, row) in p.rows.iter().enumerate() {
            inside(p.card, row.rect, &format!("{tag}: row {k}"));
            assert_eq!(
                p.hit(row.rect.x + row.rect.w * 0.5, row.rect.y + row.rect.h * 0.5),
                Some(Hit::Row(k))
            );
            // Rows never overlap each other or a group header.
            for other in &p.rows[..k] {
                assert!(row.rect.y >= other.rect.y + other.rect.h - 0.5, "{tag}: rows overlap");
            }
            for (_, g) in &p.groups {
                assert!(
                    row.rect.y >= g.y + g.h - 0.5 || row.rect.y + row.rect.h <= g.y + 0.5,
                    "{tag}: a row sits on a group header"
                );
            }
        }
        // Groups: each kind's header precedes it, and the last row of a
        // group draws no rule.
        for (word, g) in &p.groups {
            assert!(word.contains(' '), "{tag}: {word} isn't letter-spaced");
            inside(p.card, *g, &format!("{tag}: group {word}"));
        }
        assert!(p.rows.iter().any(|r| !r.rule), "{tag}: no group closes");
        let labels = p.labels();
        assert!(labels.iter().any(|l| l.text == "Effects"));
        assert!(labels.iter().any(|l| l.text == "React"));
        assert!(labels.iter().any(|l| l.text == EffectKind::React.blurb()));
        assert!(labels.iter().any(|l| l.text == "A U D I O"));
    }
}

/// The card's rects draw the glyph per row, the wash only under the
/// cursor, and the accent edge on the row being dragged.
#[test]
fn rows_light_under_the_cursor_and_the_held_one_wears_the_accent() {
    let p = Page::build(panel(), 1.0, Tab::Effects);
    let quiet = p.rects(None, None).len();
    let lit = p.rects(Some(Hit::Row(0)), None).len();
    let held = p.rects(None, Some(1)).len();
    assert_eq!(lit, quiet + 1, "hover adds one wash");
    assert_eq!(held, quiet + 1, "a held row adds one edge");
    let t = spark_ui::theme();
    assert!(
        p.rects(None, Some(1))
            .iter()
            .any(|r| r.edge_color == t.accent),
        "the held row isn't edged in the accent"
    );
}
