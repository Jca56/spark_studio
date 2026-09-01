//! Adding, removing and tuning the effects on a layer.
//!
//! Every one of these is an undoable document edit, so effects sit in the
//! same history as everything else — adding a glow and pressing Ctrl+Z
//! takes the glow away, not the last shape move.

use super::Editor;
use crate::anim::Target;
use crate::fx::EffectKind;
use crate::history::Tag;

impl Editor {
    /// Put an effect on every selected layer. The effects browser's click
    /// route, which lands next — `add_effect_to` is the card's own `+`.
    #[allow(dead_code)]
    /// Put an effect on every selected layer. Kinds are unique per layer, so
    /// adding one a layer already carries turns it back on instead.
    pub fn add_effect(&mut self, kind: EffectKind) -> bool {
        if self.selection.is_empty() {
            println!("select a layer to add an effect to");
            return false;
        }
        let s = self.snap();
        self.history.push(s);
        for &i in &self.selection.clone() {
            let stack = &mut self.fx[i];
            let id = stack.next_id();
            stack.add(kind, id);
            self.seed_colour(i, id);
            // Structural: the new effect reaches the document truth
            // directly, leaving other effects' curve scratch alone.
            if let Some(e) = self.fx[i].find(id).cloned() {
                self.base_fx[i].effects.push(e);
            }
        }
        let cur = self.snap();
        self.history.drop_noop(&cur);
        println!(
            "added {} to {} layer(s)",
            kind.label(),
            self.selection.len()
        );
        true
    }

    /// Add to one layer specifically — what a drop onto a card means,
    /// regardless of what happens to be selected. Wired up when the effects
    /// browser lands.
    #[allow(dead_code)]
    pub fn add_effect_to(&mut self, i: usize, kind: EffectKind) -> bool {
        if i >= self.fx.len() {
            return false;
        }
        let s = self.snap();
        self.history.push(s);
        let stack = &mut self.fx[i];
        let id = stack.next_id();
        stack.add(kind, id);
        self.seed_colour(i, id);
        // A structural edit reaches the document truth directly — only
        // the new effect crosses, so other effects' curve-driven params
        // stay scratch.
        if let Some(e) = self.fx[i].find(id).cloned() {
            self.base_fx[i].effects.push(e);
        }
        let cur = self.snap();
        self.history.drop_noop(&cur);
        println!("added {} to {}", kind.label(), self.display_name(i));
        true
    }

    /// Give a freshly added colour-owning effect a colour worth looking at.
    ///
    /// An unset gradient end is `[0, 0, 0]`, so adding the effect used to
    /// produce a fade to black — a look, but never the one anybody asked
    /// for by clicking "Gradient". A deep-dimmed copy of the shape's own
    /// colour makes the wash read the moment it is added, and it is only
    /// ever a starting point: the chips repaint it. Untouched if the effect
    /// already carries a colour, which is what re-adding a kind you already
    /// have means.
    fn seed_colour(&mut self, i: usize, id: u32) {
        let Some(c) = self
            .fx
            .get(i)
            .and_then(|s| s.find(id))
            .and_then(|e| e.kind.colour_param())
        else {
            return;
        };
        let rgb = self.shapes[i].rgb();
        let Some(e) = self.fx[i].find_mut(id) else {
            return;
        };
        if (0..3).all(|k| e.get(c as usize + k) == 0.0) {
            for (k, channel) in rgb.iter().enumerate() {
                e.set(c as usize + k, channel * 0.15);
            }
        }
    }

    /// The eye on an effect row: stop drawing it, keep its settings.
    #[allow(dead_code)] // kept for the redesign; the old panels were the only caller
    pub fn toggle_effect(&mut self, i: usize, id: u32) -> bool {
        if self.fx.get(i).and_then(|s| s.find(id)).is_none() {
            return false;
        }
        let s = self.snap();
        self.history.push(s);
        if let Some(e) = self.fx[i].find_mut(id) {
            e.on = !e.on;
        }
        if let Some(e) = self.base_fx[i].find_mut(id) {
            e.on = !e.on;
        }
        true
    }

    /// Take an effect off a layer, and its curves with it — a track driving
    /// a parameter of something that no longer exists is a curve nothing
    /// can ever read, and leaving it would resurrect on an id collision.
    #[allow(dead_code)] // kept for the redesign; the old panels were the only caller
    pub fn remove_effect(&mut self, i: usize, id: u32) -> bool {
        if self.fx.get(i).and_then(|s| s.find(id)).is_none() {
            return false;
        }
        let s = self.snap();
        self.history.push(s);
        self.fx[i].remove(id);
        self.base_fx[i].remove(id);
        // The effect's curves go with it, in every clip.
        for c in self.clips.get_mut(i).map(Vec::as_mut_slice).unwrap_or(&mut []) {
            c.anim
                .tracks
                .retain(|t| !matches!(t.target, Target::Effect { id: e, .. } if e == id));
        }
        true
    }

    /// A parameter slider. Coalesces into one undo step per drag, like every
    /// other slider on the card.
    #[allow(dead_code)] // kept for the redesign; the old panels were the only caller
    pub fn set_effect_param(&mut self, i: usize, id: u32, param: u8, v: f32) -> bool {
        if self.fx.get(i).and_then(|s| s.find(id)).is_none() {
            return false;
        }
        self.record(Tag::Effect(id, param));
        if let Some(e) = self.fx[i].find_mut(id) {
            e.set(param as usize, v);
        }
        self.mark_posed(&[i]);
        true
    }

    /// Whether a curve drives this parameter — the card's gold readout.
    #[allow(dead_code)] // kept for the redesign; the old panels were the only caller
    pub fn fx_keyed(&self, i: usize, id: u32, param: u8) -> bool {
        self.clip_at(i, self.time)
            .and_then(|ci| self.clips.get(i)?.get(ci))
            .is_some_and(|c| {
                c.anim
                    .tracks
                    .iter()
                    .any(|t| t.target == Target::Effect { id, param } && !t.keys.is_empty())
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use spark_render::Shape;

    fn one() -> Editor {
        let mut e = Editor::empty();
        e.push_shape(Shape::circle([0.0, 0.0], 10.0));
        e.selection = vec![0];
        e
    }

    /// Removing an effect takes its curves with it. A track pointing at a
    /// parameter of something that no longer exists can never be read, and
    /// would come back to life if the id were ever handed out again.
    #[test]
    fn removing_an_effect_removes_its_curves() {
        let mut e = one();
        e.add_effect(EffectKind::Glow);
        let id = e.fx_of(0).find_kind(EffectKind::Glow).unwrap().id;
        // The shape needs a pose before a change can be a change — the
        // first stamp is always the pose (see `Editor::pick_props`).
        e.set_time(0.0);
        e.sync_to_time();
        e.stamp_key();
        e.set_time(1.0);
        e.sync_to_time();
        e.set_effect_param(0, id, 0, 90.0);
        e.sync_to_time();
        e.stamp_key();
        assert!(e.fx_keyed(0, id, 0), "the parameter never got a curve");
        assert!(e.remove_effect(0, id));
        assert!(!e.fx_keyed(0, id, 0), "the curve outlived its effect");
        assert!(e.fx_of(0).find(id).is_none());
    }

    /// Off keeps the settings and the curves — it's an audition switch.
    #[test]
    fn toggling_off_keeps_everything() {
        let mut e = one();
        e.add_effect(EffectKind::Glow);
        let id = e.fx_of(0).find_kind(EffectKind::Glow).unwrap().id;
        e.set_effect_param(0, id, 0, 90.0);
        assert!(e.toggle_effect(0, id));
        assert!(!e.fx_of(0).find(id).unwrap().on);
        assert_eq!(e.fx_of(0).find(id).unwrap().get(0), 90.0);
    }

    /// Adding an effect is an ordinary undoable edit, not something that
    /// happens outside the document's history.
    #[test]
    fn adding_an_effect_undoes() {
        let mut e = one();
        e.add_effect(EffectKind::Glow);
        assert!(e.fx_of(0).find_kind(EffectKind::Glow).is_some());
        e.undo();
        assert!(e.fx_of(0).find_kind(EffectKind::Glow).is_none());
    }
}
