//! The React popup: **right-click a setting** in the inspector — a
//! transform field or a Style slider — and a floating panel opens beside
//! it (Alva's spec, 2026-09-01: "toggle react on → pick what triggers
//! the reaction → set the intensity slider → close the menu, and the
//! setting can have a little dot to display React is on"). React is per
//! setting now: any number the inspector edits can ride any curve the
//! analysis bakes, by an intensity of its own. The dot rides the field
//! or the slider while a reaction is on it.

use spark_render::Viewport;
use spark_ui::{ICON_X, Slider, UiRect, surfaces, theme};

use super::Drag;
use crate::Studio;
use crate::anim::Target;
use crate::chrome::{Align, Label, MENU_TEXT, UI_TEXT};
use crate::fx::{AMOUNT_DEFAULT, AMOUNT_MAX, Reaction, Source};

/// The popup's size, logical px.
pub const W: f32 = 420.0;
const PAD: f32 = 18.0;
const TITLE_H: f32 = 48.0;
const CLOSE: f32 = 36.0;
/// The React row (its check and its word), a source plate, the
/// intensity slider's label row and track.
const ROW_H: f32 = 56.0;
const CHECK: f32 = 30.0;
const PLATE_H: f32 = 48.0;
const PLATE_GAP: f32 = 8.0;
const LABEL_H: f32 = 40.0;
const TRACK_H: f32 = 15.0;
const GAP: f32 = 12.0;
/// Air the popup keeps from the control it opened on and the window.
const MARGIN: f32 = 16.0;

/// The popup, while it is up: which setting, and the control it opened
/// beside.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Open {
    pub target: Target,
    pub anchor: Viewport,
}

/// A widget on the popup.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReactHit {
    Close,
    /// The React check: on or off.
    Toggle,
    /// A trigger plate, by index into [`Source::ALL`].
    Source(usize),
    /// The intensity slider's band.
    Amount,
}

pub struct ReactPage {
    pub panel: Viewport,
    pub close: Viewport,
    pub check: Viewport,
    pub toggle_row: Viewport,
    pub sources: Vec<(Source, Viewport)>,
    pub label_y: f32,
    pub track: Viewport,
    pub amount_hit: Viewport,
    pub reaction: Option<Reaction>,
    pub title: String,
    pub scale: f32,
}

impl ReactPage {
    /// Lay the popup out beside `anchor`, pulled back inside `win` — to
    /// the control's left, the way the colour popup sits, since the
    /// inspector is at the window's right edge.
    pub fn build(
        anchor: Viewport,
        win: Viewport,
        scale: f32,
        title: String,
        reaction: Option<Reaction>,
    ) -> Self {
        let s = scale;
        let pad = PAD * s;
        let w = W * s;
        let plate_rows = Source::ALL.len().div_ceil(3);
        let h = pad * 2.0
            + TITLE_H * s
            + ROW_H * s
            + plate_rows as f32 * (PLATE_H + PLATE_GAP) * s
            + GAP * s
            + LABEL_H * s
            + TRACK_H * s
            + GAP * s;
        let margin = MARGIN * s;
        let x = (anchor.x - w - margin).min(win.x + win.w - w).max(win.x);
        let y = (anchor.y - pad).min(win.y + win.h - h).max(win.y);
        let panel = Viewport { x, y, w, h };
        let x0 = panel.x + pad;
        let inner = w - pad * 2.0;
        let close_side = CLOSE * s;
        let close = Viewport {
            x: panel.x + w - pad - close_side,
            y: panel.y + pad + (TITLE_H * s - close_side) * 0.5 - 6.0 * s,
            w: close_side,
            h: close_side,
        };
        let mut yy = panel.y + pad + TITLE_H * s;
        let toggle_row = Viewport {
            x: x0,
            y: yy,
            w: inner,
            h: ROW_H * s,
        };
        let check = Viewport {
            x: x0,
            y: yy + (ROW_H * s - CHECK * s) * 0.5,
            w: CHECK * s,
            h: CHECK * s,
        };
        yy += ROW_H * s;
        let mut sources = Vec::new();
        let plate_w = (inner - PLATE_GAP * s * 2.0) / 3.0;
        for (k, src) in Source::ALL.into_iter().enumerate() {
            let col = (k % 3) as f32;
            let row = (k / 3) as f32;
            sources.push((
                src,
                Viewport {
                    x: x0 + col * (plate_w + PLATE_GAP * s),
                    y: yy + row * (PLATE_H + PLATE_GAP) * s,
                    w: plate_w,
                    h: PLATE_H * s,
                },
            ));
        }
        yy += plate_rows as f32 * (PLATE_H + PLATE_GAP) * s + GAP * s;
        let label_y = yy;
        yy += LABEL_H * s;
        let track = Viewport {
            x: x0,
            y: yy,
            w: inner,
            h: TRACK_H * s,
        };
        let thumb = Slider::thumb_side(track);
        let amount_hit = Viewport {
            x: x0,
            y: yy - (thumb - track.h) * 0.5,
            w: inner,
            h: thumb,
        };
        Self {
            panel,
            close,
            check,
            toggle_row,
            sources,
            label_y,
            track,
            amount_hit,
            reaction,
            title,
            scale,
        }
    }

    pub fn on(&self) -> bool {
        self.reaction.is_some()
    }

    pub fn hit(&self, x: f32, y: f32) -> Option<ReactHit> {
        if self.close.contains(x, y) {
            return Some(ReactHit::Close);
        }
        if self.toggle_row.contains(x, y) {
            return Some(ReactHit::Toggle);
        }
        if let Some(k) = self.sources.iter().position(|(_, r)| r.contains(x, y)) {
            return Some(ReactHit::Source(k));
        }
        if self.amount_hit.contains(x, y) {
            return Some(ReactHit::Amount);
        }
        None
    }

    /// The slider's position for the reaction's amount.
    fn amount_t(&self) -> f32 {
        self.reaction
            .map(|r| (r.amount / AMOUNT_MAX).clamp(0.0, 1.0))
            .unwrap_or(0.0)
    }

    pub fn rects(&self, over: Option<ReactHit>) -> Vec<UiRect> {
        let t = theme();
        let m = surfaces();
        let s = self.scale;
        let on = self.on();
        let mut out = vec![m.float.rect(self.panel, s)];
        out.push(UiRect::icon_sized(
            self.close,
            ICON_X,
            2.5 * s,
            if over == Some(ReactHit::Close) {
                t.icon_hover
            } else {
                t.icon
            },
            0.3,
        ));
        // The check: a well, filled gold while React is on.
        out.push(if on {
            m.well.at_radius(6.0).filled(t.accent).rect(self.check, s)
        } else if over == Some(ReactHit::Toggle) {
            m.well.edged(self.check, s, t.accent_alt)
        } else {
            m.well.rect(self.check, s)
        });
        // The triggers: plates, the chosen one purple under a gold edge.
        let chosen = self.reaction.map(|r| r.source);
        for (k, (src, r)) in self.sources.iter().enumerate() {
            let plate = m.plate.at_radius(10.0);
            out.push(if chosen == Some(*src) {
                plate
                    .filled(t.accent_alt_bg)
                    .edge(2.0, t.accent)
                    .rect(*r, s)
            } else if over == Some(ReactHit::Source(k)) {
                plate.filled(t.button_hover).rect(*r, s)
            } else {
                plate.rect(*r, s)
            });
        }
        out.extend(Slider::rects(self.track, self.amount_t()));
        out
    }

    pub fn labels(&self) -> Vec<Label> {
        let t = theme();
        let s = self.scale;
        let line = spark_text::Text::line_height;
        let on = self.on();
        let tsize = MENU_TEXT * s;
        let size = UI_TEXT * s;
        let mut out = vec![Label {
            text: format!("React · {}", self.title),
            size: tsize,
            pos: [
                self.panel.x + PAD * s,
                self.panel.y + PAD * s + (TITLE_H * s - line(tsize)) * 0.5 - 6.0 * s,
            ],
            color: t.text,
            max_w: self.panel.w - PAD * s * 2.0 - CLOSE * s,
            align: Align::Left,
        }];
        out.push(Label {
            text: if on {
                "On".to_string()
            } else {
                "Off".to_string()
            },
            size,
            pos: [
                self.check.x + self.check.w + 14.0 * s,
                self.toggle_row.y + (self.toggle_row.h - line(size)) * 0.5,
            ],
            color: if on { t.text } else { t.text_dim },
            max_w: self.toggle_row.w,
            align: Align::Left,
        });
        let chosen = self.reaction.map(|r| r.source);
        for (src, r) in &self.sources {
            out.push(Label {
                text: src.label().to_string(),
                size,
                pos: [r.x + r.w * 0.5, r.y + (r.h - line(size)) * 0.5],
                color: if chosen == Some(*src) {
                    t.accent
                } else if on {
                    t.text
                } else {
                    t.text_dim
                },
                max_w: r.w,
                align: Align::Center,
            });
        }
        let ly = self.label_y + (LABEL_H * s - line(size)) * 0.5;
        out.push(Label {
            text: "Intensity".to_string(),
            size,
            pos: [self.track.x, ly],
            color: if on { t.text } else { t.text_dim },
            max_w: self.track.w * 0.6,
            align: Align::Left,
        });
        out.push(Label {
            text: format!("{:.2}", self.reaction.map(|r| r.amount).unwrap_or(0.0)),
            size,
            pos: [self.track.x + self.track.w, ly],
            color: if on { t.accent } else { t.text_off },
            max_w: self.track.w * 0.4,
            align: Align::Right,
        });
        out
    }
}

impl Studio {
    /// The popup laid out for this frame, while it is up.
    pub(super) fn react_page(&self) -> Option<ReactPage> {
        let open = self.inspector.react?;
        let i = self.editor.primary()?;
        let (w, h) = self.gpu.as_ref()?.size();
        let shape = &self.editor.shapes()[i];
        let fx = self.editor.fx_of(i);
        Some(ReactPage::build(
            open.anchor,
            Viewport {
                x: 0.0,
                y: 0.0,
                w: w as f32,
                h: h as f32,
            },
            self.scale(),
            crate::clipview::target_label(open.target, shape, fx),
            self.editor.reaction(i, open.target),
        ))
    }

    /// Open the popup on a setting — a right-click on its control. The
    /// colour popup gives way; a field being typed into commits.
    pub(crate) fn open_react(&mut self, target: Target, anchor: Viewport) {
        self.inspector_commit();
        self.inspector.popup = None;
        self.inspector.react = Some(Open { target, anchor });
        self.inspector.react_over = None;
    }

    /// Rewrite the popup's reaction on the primary: `f` takes what is
    /// there and says what should be.
    fn react_set(&mut self, f: impl FnOnce(Option<Reaction>, Target) -> Option<Reaction>) -> bool {
        let (Some(open), Some(i)) = (self.inspector.react, self.editor.primary()) else {
            return false;
        };
        let was = self.editor.reaction(i, open.target);
        match f(was, open.target) {
            Some(r) => self.editor.set_reaction(i, r),
            None => self.editor.remove_reaction(i, open.target),
        }
    }

    /// A fresh reaction for the popup's setting: bass, at the default
    /// intensity — the classic.
    fn fresh(target: Target) -> Reaction {
        Reaction {
            target,
            source: Source::Bass,
            amount: AMOUNT_DEFAULT,
        }
    }

    /// The intensity slider dragged to `mx`: the amount follows — and
    /// turns the reaction on if it was off, since a slider you're
    /// dragging is a thing you want.
    pub(super) fn react_amount_at(&mut self, mx: f32) -> bool {
        let Some(p) = self.react_page() else {
            return false;
        };
        let amount = Slider::t_at(p.track, mx) * AMOUNT_MAX;
        self.react_set(|was, target| {
            let mut r = was.unwrap_or_else(|| Self::fresh(target));
            r.amount = amount;
            Some(r)
        })
    }

    /// A left press while the popup is up: inside, its widgets act —
    /// the check toggles the reaction, a plate picks the trigger (and
    /// turns it on), the slider jumps and follows; outside, it closes
    /// and the click goes on to whatever it hit, except that a click
    /// elsewhere in the inspector is left to the inspector. `Some(true)`
    /// when the popup took the click.
    pub(crate) fn react_press(&mut self, cx: f32, cy: f32) -> Option<bool> {
        self.inspector.react?;
        let Some(p) = self.react_page() else {
            self.inspector.react = None;
            return Some(false);
        };
        if !p.panel.contains(cx, cy) {
            let in_right = self.layout().is_some_and(|l| l.right.contains(cx, cy));
            self.inspector.react = None;
            return if in_right { None } else { Some(false) };
        }
        match p.hit(cx, cy) {
            Some(ReactHit::Close) => self.inspector.react = None,
            Some(ReactHit::Toggle) => {
                self.react_set(|was, target| match was {
                    Some(_) => None,
                    None => Some(Self::fresh(target)),
                });
            }
            Some(ReactHit::Source(k)) => {
                if let Some(src) = Source::ALL.get(k).copied() {
                    self.react_set(|was, target| {
                        let mut r = was.unwrap_or_else(|| Self::fresh(target));
                        r.source = src;
                        Some(r)
                    });
                }
            }
            Some(ReactHit::Amount) => {
                self.react_amount_at(cx);
                self.inspector.drag = Some(Drag::ReactAmount);
            }
            None => {}
        }
        Some(true)
    }

    /// Whether the popup covers a point — the inspector's hover looks
    /// away from what is under it.
    pub(super) fn react_contains(&self, x: f32, y: f32) -> bool {
        self.react_page().is_some_and(|p| p.panel.contains(x, y))
    }
}
