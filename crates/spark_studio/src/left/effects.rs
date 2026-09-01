//! The Effects tab: a card on the left panel listing every effect that
//! can be added, grouped under small gold headers, one row each — the
//! kind's glyph, its name, one line on what it does — with a rule between
//! rows and a wash under the cursor. Glow is not here: it lives in Style
//! (Alva's call — the one effect so fundamental it is a setting).
//!
//! Pure geometry: rects, words and hit tests from the panel and the
//! state; the drag itself is the studio's (`left`).

use spark_render::Viewport;
use spark_ui::{ICON_ARC, ICON_HSV, UiRect, surfaces, theme};

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
const STRIP_GAP: f32 = 14.0;
/// The card's own inset, a group header's row, an effect's row, and the
/// air between groups.
const CARD_PAD: f32 = 14.0;
const GROUP_H: f32 = 36.0;
const GROUP_TEXT: f32 = 20.0;
const ROW_H: f32 = 78.0;
const GLYPH: f32 = 32.0;
const BLURB_TEXT: f32 = 19.0;
const GROUP_GAP: f32 = 10.0;
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

/// A kind's glyph — stand-ins from the icon set until effects get their
/// own: a hue bar for a colour effect, an arc for a wave.
pub fn glyph(kind: EffectKind) -> f32 {
    match kind {
        EffectKind::Glow | EffectKind::Gradient => ICON_HSV,
        EffectKind::React => ICON_ARC,
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
    pub card: Viewport,
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
        let card = Viewport {
            x: x0,
            y,
            w,
            h: (panel.y + panel.h - pad - y).max(1.0),
        };
        let mut page = Self {
            scale,
            tabs,
            tab,
            card,
            groups: Vec::new(),
            rows: Vec::new(),
        };
        if tab != Tab::Effects {
            return page;
        }
        let cp = CARD_PAD * s;
        let (rx, rw) = (card.x + cp, card.w - cp * 2.0);
        let mut y = card.y + cp;
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
                    x: rx,
                    y,
                    w: rw,
                    h: GROUP_H * s,
                },
            ));
            y += GROUP_H * s;
            let members: Vec<EffectKind> = kinds.iter().copied().filter(|m| group(*m) == g).collect();
            for (n, m) in members.iter().enumerate() {
                page.rows.push(RowSlot {
                    kind: *m,
                    rect: Viewport {
                        x: rx,
                        y,
                        w: rw,
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

    /// The tab's chrome: the strip, the card, the rules, the wash under
    /// the cursor, and the glyphs. `held` is the row being dragged.
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
        out.push(m.card.rect(self.card, s));
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
            out.push(UiRect::icon_sized(gr, glyph(row.kind), 2.0 * s, t.icon, 0.4));
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
        let name_size = UI_TEXT * s;
        let blurb_size = BLURB_TEXT * s;
        for row in &self.rows {
            let x = row.rect.x + (8.0 + GLYPH + 14.0) * s;
            let stack = line(name_size) + line(blurb_size) + 2.0 * s;
            let top = row.rect.y + (row.rect.h - stack) * 0.5;
            out.push(Label {
                text: row.kind.label().to_string(),
                size: name_size,
                pos: [x, top],
                color: t.text,
                max_w: row.rect.w - (x - row.rect.x) - 8.0 * s,
                align: Align::Left,
            });
            out.push(Label {
                text: row.kind.blurb().to_string(),
                size: blurb_size,
                pos: [x, top + line(name_size) + 2.0 * s],
                color: t.text_dim,
                max_w: row.rect.w - (x - row.rect.x) - 8.0 * s,
                align: Align::Left,
            });
        }
        out
    }
}
