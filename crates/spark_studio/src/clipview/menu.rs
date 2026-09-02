//! The clip view's right-click menu (Alva's spec, 2026-09-01): a key —
//! or the picked set, or a strip moment — gets its value in a box you
//! can type into, a Linear|Smooth switch, Copy, Cut, Paste and Delete;
//! a setting's row gets the four verbs for its whole curve; the graph's
//! air gets Paste, landing where you clicked. The menu itself is the
//! context menu's (`context`); this is what the view tells it and what
//! its verbs do here. Split from `input` so the gestures stay readable.

use super::words::typed_value;
use super::{Hit, Sel, beat_label, fmt_target, target_label};
use crate::Studio;
use crate::anim::{Ease, KEY_EPS, Target};
use crate::context::{self, Verb};
use crate::editor::KeySpan;

impl Studio {
    /// A right press while the view is open opens the menu on what it
    /// landed on: a diamond (picked first, unless it is already in the
    /// pick — the set is the subject then), a strip moment, a setting's
    /// row (shown first), or the graph's air at that local time. True
    /// when the press was on the panel.
    pub(crate) fn clip_view_right_press(&mut self, cx: f32, cy: f32) -> bool {
        if self.clip_view.is_none() {
            return false;
        }
        let Some(layout) = self.layout() else {
            return false;
        };
        if !layout.timeline.contains(cx, cy) {
            return false;
        }
        let scale = self.scale();
        let panel = crate::timeline::panel(layout.timeline, scale);
        let Some(page) = self.clip_view_page(&panel, scale) else {
            return true;
        };
        let t_at = {
            let cv = self.clip_view.as_ref().expect("open");
            self.snap_local(cv.view.t_at(cx, panel.axis)).max(0.0)
        };
        let target = match page.hit(cx, cy) {
            Some(Hit::Key(k)) => {
                let Some(d) = page.keys.get(k) else {
                    return true;
                };
                if let Some(cv) = self.clip_view.as_mut()
                    && !cv.sel.as_ref().is_some_and(|s| s.has(d.target, d.k))
                {
                    cv.sel = Some(Sel::Key {
                        target: d.target,
                        k: d.k,
                    });
                }
                context::Target::Keys { at: d.t }
            }
            Some(Hit::StripKey(k)) => {
                let Some(d) = page.strip_dots.get(k) else {
                    return true;
                };
                if let Some(cv) = self.clip_view.as_mut() {
                    cv.sel = Some(Sel::Time(d.t));
                }
                context::Target::Keys { at: d.t }
            }
            Some(Hit::Row(k)) => {
                let Some(row) = page.rows.get(k) else {
                    return true;
                };
                if let Some(cv) = self.clip_view.as_mut() {
                    cv.target = Some(row.target);
                    cv.sel = None;
                }
                context::Target::Row(row.target)
            }
            Some(Hit::Graph) | Some(Hit::Strip) => context::Target::Graph { at: t_at },
            Some(Hit::Back) | Some(Hit::LoopEnd) | None => return true,
        };
        self.context_open([cx, cy], target);
        true
    }

    /// The picked keys as a set, a strip moment expanded to every key
    /// at its time.
    fn pick_set(&self) -> Vec<(Target, usize)> {
        let Some(cv) = self.clip_view.as_ref() else {
            return Vec::new();
        };
        match &cv.sel {
            Some(Sel::Time(t)) => self
                .clip_view_clip()
                .map(|(_, clip)| {
                    clip.anim
                        .tracks
                        .iter()
                        .flat_map(|tr| {
                            tr.keys
                                .iter()
                                .enumerate()
                                .filter(|(_, k)| (k.t - t).abs() < KEY_EPS)
                                .map(|(k, _)| (tr.target, k))
                                .collect::<Vec<_>>()
                        })
                        .collect()
                })
                .unwrap_or_default(),
            Some(sel) => sel.set(),
            None => Vec::new(),
        }
    }

    /// The one key picked, if exactly one is.
    fn pick_one(&self) -> Option<(Target, usize)> {
        let set = self.pick_set();
        (set.len() == 1).then(|| set[0])
    }

    /// Every key of `target`'s curve, as a set.
    fn curve_set(&self, target: Target) -> Vec<(Target, usize)> {
        self.clip_view_clip()
            .and_then(|(_, clip)| clip.anim.track(target))
            .map(|tr| (0..tr.keys.len()).map(|k| (target, k)).collect())
            .unwrap_or_default()
    }

    /// What the menu's page shows for `target`: its title, the value box's
    /// number (one key only, in the inspector's units), and the ease
    /// switch's lit segment (`Some(None)` for a mixed pick; `None` for a
    /// page without the switch).
    pub(crate) fn clip_view_menu_facts(
        &self,
        target: context::Target,
    ) -> (String, Option<String>, Option<Option<usize>>) {
        let Some((i, clip)) = self.clip_view_clip() else {
            return (String::new(), None, None);
        };
        let shape = &self.editor.shapes()[i];
        let fx = self.editor.fx_of(i);
        let canvas = self.editor.canvas();
        let bpm = self.grid().bpm;
        let cv = self.clip_view.as_ref().expect("open");
        let segment = |e: Option<Ease>| {
            e.map(|e| match e {
                Ease::Linear => 0,
                Ease::Smooth => 1,
            })
        };
        match target {
            context::Target::Keys { .. } => {
                let set = self.pick_set();
                let ease = Some(segment(self.editor.keys_ease(i, cv.c, &set)));
                if let Some((t, k)) = self.pick_one()
                    && let Some(key) = clip.anim.track(t).and_then(|tr| tr.keys.get(k))
                {
                    let title = format!("{} · {}", target_label(t, shape, fx), beat_label(key.t, bpm));
                    let value = fmt_target(t, key.v, fx, canvas, shape.is_light());
                    return (title, Some(value), ease);
                }
                let title = match &cv.sel {
                    Some(Sel::Time(t)) => format!("{} keys · {}", set.len(), beat_label(*t, bpm)),
                    _ => format!("{} keys picked", set.len()),
                };
                (title, None, ease)
            }
            context::Target::Row(t) => (target_label(t, shape, fx), None, None),
            context::Target::Graph { at } => (beat_label(at, bpm), None, None),
            _ => (String::new(), None, None),
        }
    }

    /// A menu verb on a clip-view target. True when the document changed.
    pub(crate) fn clip_view_verb(&mut self, target: context::Target, verb: Verb) -> bool {
        let Some((i, _)) = self.clip_view_clip() else {
            return false;
        };
        let c = self.clip_view.as_ref().map(|cv| cv.c).unwrap_or(0);
        match target {
            context::Target::Keys { at } => match verb {
                Verb::Copy => {
                    self.clip_view_copy();
                    false
                }
                Verb::Cut => self.clip_view_cut(),
                Verb::Delete => self.clip_view_delete(),
                Verb::Paste => self.clip_view_paste_at(Some(at), None),
                Verb::Duplicate | Verb::ClearLoop | Verb::Relink => false,
            },
            context::Target::Row(t) => {
                let set = self.curve_set(t);
                match verb {
                    Verb::Copy => {
                        let n = self.editor.copy_keys(i, c, KeySpan::Set(set));
                        self.note_copied(n);
                        false
                    }
                    Verb::Cut => {
                        let n = self.editor.copy_keys(i, c, KeySpan::Set(set.clone()));
                        if n == 0 {
                            self.export_note = Some("nothing to cut — no keys here".to_string());
                            return false;
                        }
                        let gone = self.editor.delete_keys(i, c, &set);
                        self.export_note = Some(format!("cut {n} key{}", if n == 1 { "" } else { "s" }));
                        gone
                    }
                    Verb::Delete => {
                        let gone = self.editor.delete_keys(i, c, &set);
                        if let Some(cv) = self.clip_view.as_mut() {
                            cv.sel = None;
                        }
                        gone
                    }
                    Verb::Paste => self.clip_view_paste_at(None, Some(t)),
                    Verb::Duplicate | Verb::ClearLoop | Verb::Relink => false,
                }
            }
            context::Target::Graph { at } => match verb {
                Verb::Paste => self.clip_view_paste_at(Some(at), None),
                _ => false,
            },
            _ => false,
        }
    }

    fn note_copied(&mut self, n: usize) {
        self.export_note = Some(match n {
            0 => "nothing to copy — no keys here".to_string(),
            1 => "copied 1 key".to_string(),
            n => format!("copied {n} keys"),
        });
    }

    /// The menu's ease switch: every picked key takes `ease`.
    pub(crate) fn clip_view_set_ease(&mut self, target: context::Target, ease: Ease) -> bool {
        if !matches!(target, context::Target::Keys { .. }) {
            return false;
        }
        let Some((i, _)) = self.clip_view_clip() else {
            return false;
        };
        let c = self.clip_view.as_ref().map(|cv| cv.c).unwrap_or(0);
        let set = self.pick_set();
        self.editor.set_keys_ease(i, c, &set, ease)
    }

    /// The menu's value box, committed: the one picked key takes the
    /// number typed, read in the inspector's units.
    pub(crate) fn clip_view_set_value(&mut self, target: context::Target, typed: f32) -> bool {
        if !matches!(target, context::Target::Keys { .. }) {
            return false;
        }
        let Some((i, _)) = self.clip_view_clip() else {
            return false;
        };
        let c = self.clip_view.as_ref().map(|cv| cv.c).unwrap_or(0);
        let Some((t, k)) = self.pick_one() else {
            return false;
        };
        let v = typed_value(t, typed, self.editor.shapes()[i].is_light());
        self.editor.set_key_value(i, c, t, k, v)
    }
}
