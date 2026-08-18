//! What a click on the layer list landed on.
//!
//! Split from the layout half: `rows` decides where everything *is*, this
//! decides what the cursor is over. Both walk the same `Cards`, and keeping
//! them in one file was what pushed it past its size budget.

use spark_render::Viewport;

use super::Cards;
use crate::editor::Prop;

#[derive(Clone, Copy, PartialEq)]
pub enum CardHit {
    /// The identity strip (or dead card space): select / rename / reorder.
    Head(usize),
    /// The visibility eye.
    Eye(usize),
    Cog(usize),
    /// Start scrubbing this field.
    Scrub(usize, Prop),
    /// A detail slider: (shape, prop, normalized position).
    Slider(usize, Prop, f32),
    Outline(usize, bool),
    /// A star field's form: index into `STAR_FORMS`.
    Form(usize, usize),
    Blend(usize, bool),
    Gradient(usize, bool),
    /// The effects-tab button on a card head.
    FxTab(usize),
    /// An effect's eye: stop drawing it, keep its settings.
    FxToggle(usize, u32),
    /// An effect's remove button.
    FxRemove(usize, u32),
    /// An effect parameter slider: (shape, effect, parameter, position).
    FxSlider(usize, u32, u8, f32),
    /// Arm a gradient endpoint as the color home's target (true = B).
    Chip(usize, bool),
    /// A folder's `−`/`+` disclosure box.
    FolderDisclose(u32),
    /// A folder's eye.
    FolderEye(u32),
    /// A folder header: select its contents / rename / drop layers onto it.
    FolderHead(u32),
    /// Start scrubbing one of a folder transform's fields.
    FolderScrub(u32, Prop),
}

impl CardHit {
    /// Whether this hit is a button worth lighting on hover. Sliders and
    /// scrub fields are excluded: their payload changes as the cursor moves,
    /// so they would never compare equal twice and every mouse move would
    /// force a redraw.
    pub fn hoverable(self) -> bool {
        matches!(
            self,
            CardHit::Eye(_)
                | CardHit::Cog(_)
                | CardHit::FxTab(_)
                | CardHit::FxToggle(_, _)
                | CardHit::FxRemove(_, _)
                | CardHit::FolderEye(_)
                | CardHit::FolderDisclose(_)
        )
    }
}

/// Hits require the click inside the panel too — scrolled-out cards must
/// not swallow clicks meant for whatever's beneath them.
pub fn hit(cards: &Cards, panel: Viewport, px: f32, py: f32) -> Option<CardHit> {
    if !panel.contains(px, py) {
        return None;
    }
    for f in &cards.folders {
        if let Some(s) = f.scrubs.iter().find(|s| s.rect.contains(px, py)) {
            return Some(CardHit::FolderScrub(f.id, s.prop));
        }
        if !f.row.contains(px, py) {
            continue;
        }
        if f.disclose.contains(px, py) {
            return Some(CardHit::FolderDisclose(f.id));
        }
        if f.eye.contains(px, py) {
            return Some(CardHit::FolderEye(f.id));
        }
        return Some(CardHit::FolderHead(f.id));
    }
    let lr = cards.rows.iter().find(|r| r.row.contains(px, py))?;
    let i = lr.index;
    if lr.eye.contains(px, py) {
        return Some(CardHit::Eye(i));
    }
    if lr.cog.is_some_and(|c| c.contains(px, py)) {
        return Some(CardHit::Cog(i));
    }
    if lr.fx_tab.is_some_and(|c| c.contains(px, py)) {
        return Some(CardHit::FxTab(i));
    }
    if let Some(f) = lr.scrubs.iter().find(|f| f.rect.contains(px, py)) {
        return Some(CardHit::Scrub(i, f.prop));
    }
    if let Some(d) = &lr.detail {
        if let Some(row) = d.sliders.iter().find(|r| {
            px >= r.track.x
                && px <= r.track.x + r.track.w
                && (py - (r.track.y + r.track.h * 0.5)).abs() <= r.track.h * 2.2
        }) {
            let t = spark_ui::Slider::t_at(row.track, px);
            return Some(CardHit::Slider(i, row.prop, t));
        }
        if let Some(f) = &d.form
            && let Some(k) = f.seg.hit(px, py)
        {
            return Some(CardHit::Form(i, k));
        }
        if let Some(s) = &d.style
            && let Some(k) = s.seg.hit(px, py)
        {
            return Some(CardHit::Outline(i, k == 1));
        }
        if let Some(k) = d.blend.as_ref().and_then(|t| t.seg.hit(px, py)) {
            return Some(CardHit::Blend(i, k == 1));
        }
        if let Some(k) = d.grad.as_ref().and_then(|t| t.seg.hit(px, py)) {
            return Some(CardHit::Gradient(i, k == 1));
        }
        for row in &d.fx {
            if row.eye.contains(px, py) {
                return Some(CardHit::FxToggle(i, row.id));
            }
            if row.remove.contains(px, py) {
                return Some(CardHit::FxRemove(i, row.id));
            }
            if let Some(p) = row.params.iter().find(|p| {
                px >= p.track.x
                    && px <= p.track.x + p.track.w
                    && (py - (p.track.y + p.track.h * 0.5)).abs() <= p.track.h * 2.2
            }) {
                let t = spark_ui::Slider::t_at(p.track, px);
                return Some(CardHit::FxSlider(i, p.id, p.param, t));
            }
        }
        if let Some(chips) = &d.chips {
            for (k, c) in chips.iter().enumerate() {
                if c.contains(px, py) {
                    return Some(CardHit::Chip(i, k == 1));
                }
            }
        }
    }
    // The head, or dead card space: either way it's the card's click.
    Some(CardHit::Head(i))
}
