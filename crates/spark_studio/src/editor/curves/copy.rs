//! The key clipboard: `Ctrl+C` in the clip view takes keys — the picked
//! one, every key at the picked moment, or, with nothing picked, the
//! whole clip's — and `Ctrl+V` puts them back with the first of them
//! on the playhead, in this clip or any other object's, as many times
//! as you like (Alva, 2026-09-01: "that will help immensely").
//!
//! A copy names its **settings**, not its tracks: a shape property is
//! itself, an effect parameter is its effect's *kind* and the
//! parameter — effect ids are per object, and a Glow curve copied off
//! one laser has to land on the next laser's Glow. Times are kept
//! relative to the copy's first key, so a paste lands that key on the
//! playhead and the rest keep their spacing after it. A setting the
//! destination can't key (a circle's X on a line, a Glow on an object
//! without the effect) is skipped; a key landing on one already there
//! replaces it, ease and all. One undo step per paste.

use super::Editor;
use crate::anim::{Ease, KEY_EPS, Key, Target, Track};
use crate::fx::EffectKind;
use crate::props::Prop;

/// What names a setting across objects.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Setting {
    Shape(Prop),
    Effect(EffectKind, u8),
}

/// One copied key: its setting, its time from the copy's first key,
/// its value and how it runs to the next.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CopiedKey {
    pub setting: Setting,
    pub t: f32,
    pub v: f32,
    pub ease: Ease,
}

/// What `Ctrl+C` holds.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct KeyClip {
    pub keys: Vec<CopiedKey>,
}

impl KeyClip {
    pub fn is_empty(&self) -> bool {
        self.keys.is_empty()
    }
}

/// What a copy takes from a clip.
#[derive(Clone, Debug, PartialEq)]
pub enum KeySpan {
    /// One key of one setting, by index on its track.
    Key(Target, usize),
    /// Every key at a local time, across every track.
    Moment(f32),
    /// A picked set of keys, by track and index.
    Set(Vec<(Target, usize)>),
    /// Every key the clip has.
    Clip,
}

impl Editor {
    /// The setting a track on object `i` drives, named so it can land
    /// elsewhere. `None` for an effect the object no longer has.
    fn setting_of(&self, i: usize, target: Target) -> Option<Setting> {
        match target {
            Target::Shape(p) => Some(Setting::Shape(p)),
            Target::Effect { id, param } => self
                .base_fx
                .get(i)?
                .find(id)
                .map(|e| Setting::Effect(e.kind, param)),
        }
    }

    /// The track a setting lands on for object `i`: a property it can
    /// key, or the parameter of its effect of that kind. `None` where
    /// the object has no such thing.
    fn target_for(&self, i: usize, setting: Setting) -> Option<Target> {
        match setting {
            Setting::Shape(p) => {
                crate::anim::keyable(self.base.get(i)?, p).then_some(Target::Shape(p))
            }
            Setting::Effect(kind, param) => {
                let e = self.base_fx.get(i)?.find_kind(kind)?;
                ((param as usize) < kind.params().len()).then_some(Target::Effect { id: e.id, param })
            }
        }
    }

    /// Copy `span`'s keys off object `i`'s clip `c`. Changes nothing on
    /// screen. How many keys were taken — none leaves the clipboard as
    /// it was, so a stray Ctrl+C on an empty clip costs nothing.
    pub fn copy_keys(&mut self, i: usize, c: usize, span: KeySpan) -> usize {
        let Some(anim) = self.clip_anim(i, c) else {
            return 0;
        };
        let mut keys: Vec<CopiedKey> = Vec::new();
        for tr in &anim.tracks {
            let Some(setting) = self.setting_of(i, tr.target) else {
                continue;
            };
            for (k, key) in tr.keys.iter().enumerate() {
                let take = match &span {
                    KeySpan::Key(target, at) => tr.target == *target && k == *at,
                    KeySpan::Moment(t) => (key.t - t).abs() < KEY_EPS,
                    KeySpan::Set(set) => set.contains(&(tr.target, k)),
                    KeySpan::Clip => true,
                };
                if take {
                    keys.push(CopiedKey {
                        setting,
                        t: key.t,
                        v: key.v,
                        ease: key.ease,
                    });
                }
            }
        }
        if keys.is_empty() {
            return 0;
        }
        let t0 = keys.iter().map(|k| k.t).fold(f32::MAX, f32::min);
        for k in &mut keys {
            k.t -= t0;
        }
        let n = keys.len();
        self.key_clip = Some(KeyClip { keys });
        n
    }

    /// What `Ctrl+C` holds, if anything.
    pub fn key_clip(&self) -> Option<&KeyClip> {
        self.key_clip.as_ref()
    }

    /// Paste the copied keys onto object `i`'s clip `c`, the first of
    /// them at local time `at`. Settings the object lacks are skipped;
    /// a key landing on one already there replaces it. How many keys
    /// landed — none means no undo step either.
    pub fn paste_keys(&mut self, i: usize, c: usize, at: f32) -> usize {
        self.paste_keys_as(i, c, at, None)
    }

    /// Paste onto one setting: a copy of a single setting's keys lands
    /// on `target` whatever setting it came from — Y2's curve pasted
    /// onto Y1. A copy of several settings pastes by setting, as ever.
    pub fn paste_keys_onto(&mut self, i: usize, c: usize, at: f32, target: Target) -> usize {
        let one = self.key_clip.as_ref().is_some_and(|clip| {
            let mut settings = clip.keys.iter().map(|k| k.setting);
            settings.next().is_some_and(|first| settings.all(|s| s == first))
        });
        self.paste_keys_as(i, c, at, one.then_some(target))
    }

    fn paste_keys_as(&mut self, i: usize, c: usize, at: f32, onto: Option<Target>) -> usize {
        let Some(clip) = self.key_clip.clone() else {
            return 0;
        };
        if self.clip_anim(i, c).is_none() {
            return 0;
        }
        let at = at.max(0.0);
        let landing: Vec<(Target, Key)> = clip
            .keys
            .iter()
            .filter_map(|k| {
                let target = match onto {
                    Some(t) => t,
                    None => self.target_for(i, k.setting)?,
                };
                let v = self.fit_value(i, target, k.v);
                Some((
                    target,
                    Key {
                        t: at + k.t,
                        v,
                        ease: k.ease,
                    },
                ))
            })
            .collect();
        if landing.is_empty() {
            return 0;
        }
        self.push_keys();
        let anim = &mut self.clips[i][c].anim;
        for (target, key) in &landing {
            match anim.track_mut(*target) {
                Some(tr) => tr.put(*key),
                None => anim.tracks.push(Track {
                    target: *target,
                    keys: vec![*key],
                }),
            }
        }
        self.unpose(i);
        landing.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fx::EffectKind;
    use crate::props::Tool;

    fn draw(e: &mut Editor, tool: Tool, a: [f32; 2], b: [f32; 2]) -> usize {
        e.set_time(0.0);
        e.sync_to_time();
        e.choose_tool(tool);
        e.set_cursor_canvas(a);
        e.mouse_down(false);
        e.set_cursor_canvas(b);
        e.mouse_up();
        e.choose_tool(Tool::Select);
        e.primary().expect("drawn")
    }

    fn keys_of(e: &Editor, i: usize, target: Target) -> Vec<(f32, f32, Ease)> {
        e.clip_anim(i, 0)
            .and_then(|a| a.track(target))
            .map(|tr| tr.keys.iter().map(|k| (k.t, k.v, k.ease)).collect())
            .unwrap_or_default()
    }

    /// Give object `i`'s first clip a track of `keys` on `target`.
    fn lay(e: &mut Editor, i: usize, target: Target, keys: &[(f32, f32, Ease)]) {
        e.clip_anim_mut(i, 0).tracks.push(Track {
            target,
            keys: keys
                .iter()
                .map(|&(t, v, ease)| Key { t, v, ease })
                .collect(),
        });
    }

    /// Add a Glow to object `i` and say which id it got.
    fn glow_on(e: &mut Editor, i: usize) -> u32 {
        assert!(e.add_effect_to(i, EffectKind::Glow));
        e.fx_of(i).find_kind(EffectKind::Glow).expect("added").id
    }

    /// A circle with X keyed at 0, 0.5 and 1 (300 → 600 → 900), Y keyed
    /// at 0.5 alone, and a Glow keyed at 0 and 1.
    fn keyed() -> (Editor, usize) {
        let mut e = Editor::empty();
        let i = draw(&mut e, Tool::Circle, [300.0, 300.0], [380.0, 300.0]);
        let (l, s) = (Ease::Linear, Ease::Smooth);
        lay(&mut e, i, Target::Shape(Prop::X), &[(0.0, 300.0, l), (0.5, 600.0, l), (1.0, 900.0, l)]);
        lay(&mut e, i, Target::Shape(Prop::Y), &[(0.5, 100.0, s)]);
        let gid = glow_on(&mut e, i);
        lay(&mut e, i, Target::Effect { id: gid, param: 0 }, &[(0.0, 10.0, l), (1.0, 80.0, l)]);
        (e, i)
    }

    /// One picked key pastes at the playhead with its value and ease;
    /// a picked moment brings every setting keyed there; a paste onto
    /// a time already keyed replaces that key; and the paste is one
    /// undo step.
    #[test]
    fn a_key_and_a_moment_paste_where_asked() {
        let (mut e, i) = keyed();
        let x = Target::Shape(Prop::X);
        let y = Target::Shape(Prop::Y);
        assert_eq!(e.copy_keys(i, 0, KeySpan::Key(x, 1)), 1);
        assert_eq!(e.paste_keys(i, 0, 1.5), 1);
        assert_eq!(keys_of(&e, i, x)[3], (1.5, 600.0, Ease::Linear));
        // The moment at 0.5 is an X and a Y; both land at 0.25.
        assert_eq!(e.copy_keys(i, 0, KeySpan::Moment(0.5)), 2);
        assert_eq!(e.paste_keys(i, 0, 0.25), 2);
        assert_eq!(keys_of(&e, i, x)[1], (0.25, 600.0, Ease::Linear));
        assert_eq!(keys_of(&e, i, y), vec![(0.25, 100.0, Ease::Smooth), (0.5, 100.0, Ease::Smooth)]);
        // Onto the key at 1.0: replaced, not doubled.
        assert_eq!(e.paste_keys(i, 0, 1.0), 2);
        let xs = keys_of(&e, i, x);
        assert_eq!(xs.iter().filter(|k| (k.0 - 1.0).abs() < 1e-4).count(), 1);
        assert_eq!(xs.iter().find(|k| (k.0 - 1.0).abs() < 1e-4).unwrap().1, 600.0);
        assert!(e.undo(), "the paste is an undo step");
        assert_eq!(keys_of(&e, i, x).iter().find(|k| (k.0 - 1.0).abs() < 1e-4).unwrap().1, 900.0);
        // Nothing picked: the whole clip — every key on every track as
        // they stand now (X 5, Y 2, Glow 2).
        let total: usize = e.clip_anim(i, 0).unwrap().tracks.iter().map(|t| t.keys.len()).sum();
        assert_eq!(total, 9);
        assert_eq!(e.copy_keys(i, 0, KeySpan::Clip), total);
    }

    /// The whole clip pastes onto another object's clip: times keep
    /// their spacing from the playhead, the Glow curve finds the other
    /// object's own Glow by kind, and a setting the other object can't
    /// key is skipped rather than planted.
    #[test]
    fn a_clip_pastes_onto_another_object_by_setting() {
        let (mut e, i) = keyed();
        assert_eq!(e.copy_keys(i, 0, KeySpan::Clip), 6);
        // A line: it keys its ends, never X and Y — those are skipped.
        let j = draw(&mut e, Tool::Line, [100.0, 100.0], [500.0, 100.0]);
        assert_eq!(e.paste_keys(j, 0, 0.25), 0, "no Glow, no X: nothing to land on");
        let gj = glow_on(&mut e, j);
        assert_eq!(e.paste_keys(j, 0, 0.25), 2, "the Glow curve alone");
        let glow = keys_of(&e, j, Target::Effect { id: gj, param: 0 });
        assert_eq!(glow, vec![(0.25, 10.0, Ease::Linear), (1.25, 80.0, Ease::Linear)]);
        assert!(keys_of(&e, j, Target::Shape(Prop::X)).is_empty());
        // Another circle takes everything, with the spacing kept. Its
        // Glow sits behind a Gradient, so its id differs from the
        // source's: the copy has to find it by kind, not by number.
        let k = draw(&mut e, Tool::Circle, [800.0, 300.0], [860.0, 300.0]);
        assert!(e.add_effect_to(k, EffectKind::Gradient));
        let gk = glow_on(&mut e, k);
        let gi = e.fx_of(i).find_kind(EffectKind::Glow).unwrap().id;
        assert_ne!(gk, gi, "the rig: a different id on the destination");
        assert_eq!(e.paste_keys(k, 0, 2.0), 6);
        let xs = keys_of(&e, k, Target::Shape(Prop::X));
        assert_eq!(xs.iter().map(|k| k.0).collect::<Vec<_>>(), vec![2.0, 2.5, 3.0]);
        assert_eq!(keys_of(&e, k, Target::Shape(Prop::Y)), vec![(2.5, 100.0, Ease::Smooth)]);
        assert_eq!(keys_of(&e, k, Target::Effect { id: gk, param: 0 }).len(), 2);
        assert!(keys_of(&e, k, Target::Effect { id: gi, param: 0 }).is_empty(), "landed by id, not kind");
        // An empty span copies nothing and keeps what was held.
        assert_eq!(e.copy_keys(k, 0, KeySpan::Moment(9.0)), 0);
        assert_eq!(e.key_clip().map(|c| c.keys.len()), Some(6));
    }
}
