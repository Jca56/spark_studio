//! The Effects tab: every effect that can be added, one row each — the
//! kind's glyph and its name, nothing else — grouped under small gold
//! headers, a rule between rows, a wash under the cursor. Rows sit on the
//! panel itself: the first cut lifted them onto a card and Alva's verdict
//! was "darker not lighter" — depth here is recesses and rules, never a
//! lit region (Lantern Mix's ground-vs-object rule). No blurbs either:
//! "I know what the effect does." Glow is not listed: it lives in Style.
//!
//! Pure geometry: rects, words and hit tests from the panel and the
//! state; the drag itself is the studio's (`left`).

use spark_render::Viewport;
use spark_ui::{ICON_HSV, UiRect, surfaces, theme};

use super::Tab;
use crate::chrome::{Align, Label, MENU_TEXT, UI_TEXT};
use crate::fx::{EffectKind, KINDS};
use crate::inspector::spaced;

/// Inset from the panel's edges, logical px.
pub const PAD: f32 = 18.0;
/// A tab plate, and the strip's air under it.
const TAB_W: f32 = 150.0;
const TAB_H: f32 = 44.0;
const TAB_GAP: f32 = 8.0;
const STRIP_GAP: f32 = 18.0;
/// A group header's row, an effect's row, the glyph, and the air between
/// groups.
const GROUP_H: f32 = 36.0;
const GROUP_TEXT: f32 = 20.0;
const ROW_H: f32 = 56.0;
const GLYPH: f32 = 28.0;
const GROUP_GAP: f32 = 14.0;
const RULE: f32 = 2.0;

/// The kinds the browser offers, in order — everything but Glow.
pub fn offered() -> impl Iterator<Item = EffectKind> {
    KINDS
        .into_iter()
        .filter(|k| *k != EffectKind::Glow)
}

/// Which group a kind lists under.
pub fn group(kind: EffectKind) -> &'static str {
    match kind {
        EffectKind::Glow | EffectKind::Gradient => "Look",
        EffectKind::React => "Audio",
    }
}

/// A kind's glyph in `r` — stand-ins from the material set until effects
/// get their own: a hue square for a colour effect, a ring for a wave.
pub fn glyph(kind: EffectKind, r: Viewport, s: f32, color: [f32; 4]) -> UiRect {
    match kind {
        EffectKind::Glow | EffectKind::Gradient => UiRect::icon_sized(r, ICON_HSV, 2.0 * s, color, 0.4),
        EffectKind::React => UiRect::ring(r, 0.36, 2.5 * s, color),
    }
}

/// A widget on the tab.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hit {
    Tab(usize),
    Row(usize),
}

/// One effect's row, laid out.
#[derive(Clone, Debug, PartialEq)]
pub struct RowSlot {
    pub kind: EffectKind,
    pub rect: Viewport,
    /// A rule under it, unless it closes its group.
    pub rule: bool,
}

pub struct Page {
    pub scale: f32,
    pub tabs: Vec<(Tab, Viewport)>,
    pub tab: Tab,
    /// Group headers: the word and its row.
    pub groups: Vec<(String, Viewport)>,
    pub rows: Vec<RowSlot>,
}

impl Page {
    pub fn build(panel: Viewport, scale: f32, tab: Tab) -> Self {
        let s = scale;
        let pad = PAD * s;
        let x0 = panel.x + pad;
        let w = panel.w - pad * 2.0;
        let mut y = panel.y + pad;
        let tabs: Vec<(Tab, Viewport)> = Tab::ALL
            .iter()
            .enumerate()
            .map(|(i, t)| {
                (
                    *t,
                    Viewport {
                        x: x0 + (TAB_W + TAB_GAP) * s * i as f32,
                        y,
                        w: TAB_W * s,
                        h: TAB_H * s,
                    },
                )
            })
            .collect();
        y += (TAB_H + STRIP_GAP) * s;
        let mut page = Self {
            scale,
            tabs,
            tab,
            groups: Vec::new(),
            rows: Vec::new(),
        };
        if tab != Tab::Effects {
            return page;
        }
        let kinds: Vec<EffectKind> = offered().collect();
        let mut seen: Vec<&'static str> = Vec::new();
        for k in &kinds {
            let g = group(*k);
            if seen.contains(&g) {
                continue;
            }
            seen.push(g);
            page.groups.push((
                spaced(g),
                Viewport {
                    x: x0,
                    y,
                    w,
                    h: GROUP_H * s,
                },
            ));
            y += GROUP_H * s;
            let members: Vec<EffectKind> =
                kinds.iter().copied().filter(|m| group(*m) == g).collect();
            for (n, m) in members.iter().enumerate() {
                page.rows.push(RowSlot {
                    kind: *m,
                    rect: Viewport {
                        x: x0,
                        y,
                        w,
                        h: ROW_H * s,
                    },
                    rule: n + 1 < members.len(),
                });
                y += ROW_H * s;
            }
            y += GROUP_GAP * s;
        }
        page
    }

    pub fn hit(&self, x: f32, y: f32) -> Option<Hit> {
        if let Some(i) = self.tabs.iter().position(|(_, r)| r.contains(x, y)) {
            return Some(Hit::Tab(i));
        }
        if let Some(k) = self.rows.iter().position(|r| r.rect.contains(x, y)) {
            return Some(Hit::Row(k));
        }
        None
    }

    /// The tab's chrome: the strip, the rules, the wash under the cursor
    /// (an accent edge on the row being dragged), and the glyphs.
    pub fn rects(&self, over: Option<Hit>, held: Option<usize>) -> Vec<UiRect> {
        let t = theme();
        let m = surfaces();
        let s = self.scale;
        let mut out = Vec::new();
        for (i, (tab, r)) in self.tabs.iter().enumerate() {
            out.push(if *tab == self.tab {
                m.plate
                    .filled(t.accent_alt_bg)
                    .edge(2.0, t.accent)
                    .rect(*r, s)
            } else if over == Some(Hit::Tab(i)) {
                m.plate.filled(t.button_hover).rect(*r, s)
            } else {
                m.plate.rect(*r, s)
            });
        }
        for (k, row) in self.rows.iter().enumerate() {
            if held == Some(k) {
                out.push(m.hover.edged(row.rect, s, t.accent));
            } else if over == Some(Hit::Row(k)) {
                out.push(m.hover.rect(row.rect, s));
            }
            let g = GLYPH * s;
            let gr = Viewport {
                x: row.rect.x + 8.0 * s,
                y: row.rect.y + (row.rect.h - g) * 0.5,
                w: g,
                h: g,
            };
            out.push(glyph(row.kind, gr, s, t.icon));
            if row.rule {
                out.push(UiRect::region(
                    Viewport {
                        x: row.rect.x,
                        y: row.rect.y + row.rect.h - RULE * s,
                        w: row.rect.w,
                        h: (RULE * s).max(1.0),
                    },
                    t.card_border,
                ));
            }
        }
        out
    }

    pub fn labels(&self) -> Vec<Label> {
        let t = theme();
        let s = self.scale;
        let line = |sz: f32| spark_text::Text::line_height(sz);
        let mut out = Vec::new();
        let tsize = MENU_TEXT * s;
        for (tab, r) in &self.tabs {
            out.push(Label {
                text: tab.title().to_string(),
                size: tsize,
                pos: [r.x + r.w * 0.5, r.y + (r.h - line(tsize)) * 0.5],
                color: if *tab == self.tab { t.accent } else { t.text_dim },
                max_w: r.w,
                align: Align::Center,
            });
        }
        let gsize = GROUP_TEXT * s;
        for (word, r) in &self.groups {
            out.push(Label {
                text: word.clone(),
                size: gsize,
                pos: [r.x + 8.0 * s, r.y + (r.h - line(gsize)) * 0.5],
                color: t.accent,
                max_w: r.w,
                align: Align::Left,
            });
        }
        let size = UI_TEXT * s;
        for row in &self.rows {
            let x = row.rect.x + (8.0 + GLYPH + 14.0) * s;
            out.push(Label {
                text: row.kind.label().to_string(),
                size,
                pos: [x, row.rect.y + (row.rect.h - line(size)) * 0.5],
                color: t.text,
                max_w: row.rect.w - (x - row.rect.x) - 8.0 * s,
                align: Align::Left,
            });
        }
        out
    }
}
