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
/// rows on the panel — no card under them — with a rule between rows of
/// a group and none after its last; the tab strip sits above; every row
/// and tab hits; and the words are the tabs, the groups and the names,
/// nothing more.
#[test]
fn the_effects_tab_lists_every_kind_but_glow() {
    for scale in [1.0f32, 1.4] {
        let p = Page::build(panel(), scale, Tab::Effects);
        let tag = format!("scale {scale}");
        assert_eq!(p.tabs.len(), Tab::ALL.len());
        assert_eq!(p.hit(p.tabs[0].1.x + 5.0, p.tabs[0].1.y + 5.0), Some(Hit::Tab(0)));
        let kinds: Vec<EffectKind> = p.rows.iter().map(|r| r.kind).collect();
        assert!(!kinds.contains(&EffectKind::Glow), "{tag}: Glow is Style's");
        for k in KINDS {
            assert_eq!(kinds.contains(&k), k != EffectKind::Glow, "{tag}: {k:?}");
        }
        let strip_bottom = p.tabs[0].1.y + p.tabs[0].1.h;
        for (k, row) in p.rows.iter().enumerate() {
            inside(panel(), row.rect, &format!("{tag}: row {k}"));
            assert!(row.rect.y >= strip_bottom, "{tag}: a row overlaps the strip");
            assert_eq!(
                p.hit(row.rect.x + row.rect.w * 0.5, row.rect.y + row.rect.h * 0.5),
                Some(Hit::Row(k))
            );
            for other in &p.rows[..k] {
                assert!(
                    row.rect.y >= other.rect.y + other.rect.h - 0.5,
                    "{tag}: rows overlap"
                );
            }
            for (_, g) in &p.groups {
                assert!(
                    row.rect.y >= g.y + g.h - 0.5 || row.rect.y + row.rect.h <= g.y + 0.5,
                    "{tag}: a row sits on a group header"
                );
            }
        }
        for (word, g) in &p.groups {
            assert!(word.contains(' '), "{tag}: {word} isn't letter-spaced");
            inside(panel(), *g, &format!("{tag}: group {word}"));
        }
        assert!(p.rows.iter().any(|r| !r.rule), "{tag}: no group closes");
        // Words: tabs, groups, names — and no clutter under a name.
        let labels = p.labels();
        assert!(labels.iter().any(|l| l.text == "Effects"));
        assert!(labels.iter().any(|l| l.text == "Gradient"));
        assert!(labels.iter().any(|l| l.text == "L O O K"));
        let allowed = |t: &str| {
            Tab::ALL.iter().any(|tb| tb.title() == t)
                || p.groups.iter().any(|(w, _)| w == t)
                || KINDS.iter().any(|k| k.label() == t)
        };
        for l in &labels {
            assert!(allowed(&l.text), "{tag}: clutter text {:?}", l.text);
        }
        // Nothing but the tab plates is a lifted surface: the rows' chrome
        // is rules and glyphs, with a wash only under the cursor.
        let quiet = p.rects(None, None);
        let lifted = quiet
            .iter()
            .filter(|r| r.size[1] > 40.0 * scale && r.size[0] > 200.0 * scale)
            .count();
        assert_eq!(lifted, 0, "{tag}: a big lit region snuck in");
    }
}

/// The row chrome draws the glyph per row, the wash only under the
/// cursor, and the accent edge on the row being dragged.
#[test]
fn rows_light_under_the_cursor_and_the_held_one_wears_the_accent() {
    let p = Page::build(panel(), 1.0, Tab::Effects);
    let quiet = p.rects(None, None).len();
    let lit = p.rects(Some(Hit::Row(0)), None).len();
    let held = p.rects(None, Some(0)).len();
    assert_eq!(lit, quiet + 1, "hover adds one wash");
    assert_eq!(held, quiet + 1, "a held row adds one edge");
    let t = spark_ui::theme();
    assert!(
        p.rects(None, Some(0))
            .iter()
            .any(|r| r.edge_color == t.accent),
        "the held row isn't edged in the accent"
    );
}
