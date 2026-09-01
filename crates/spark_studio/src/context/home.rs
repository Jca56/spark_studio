//! Home: what the menu offers for the thing under the cursor when it
//! opened (Alva's spec, 2026-08-31: "it should be smart and know what
//! options to show depending on what was under the cursor").
//!
//! The menu's subject is a [`Target`], captured at the right press and
//! kept for the menu's life — not re-read from the selection, so a
//! right-click on empty space with a selection still means empty space.
//! Every verb sits in one table per target, so scaling is mechanical: a
//! new right-clickable thing is a `Target` variant plus its table; a new
//! verb is a row in the tables that want it plus a dispatch arm. Whether
//! a row is *lit* is the editor's state (`enabled`), not the table's.
//!
//! Empty space offers nothing yet. An object offers Copy, Paste,
//! Duplicate and Delete — Delete in red. Folder, Merge, Hide, the style
//! pair and Convert to Path left the menu (they belong elsewhere; their
//! keys still work), and Make Comp left it for good.

use crate::Studio;
use crate::editor::Editor;

/// What was under the cursor when the menu opened.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Target {
    /// Empty canvas, or an empty panel.
    #[default]
    Empty,
    /// An object on the canvas, by id — the right-click selected it, so
    /// the verbs act on the selection it belongs to.
    Object(u32),
}

/// What a Home row does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verb {
    Copy,
    Paste,
    Duplicate,
    Delete,
}

/// How a row reads: plain, or the red of something you can't take back.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tone {
    Normal,
    Danger,
}

/// One row of a target's table.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Action {
    pub verb: Verb,
    pub label: &'static str,
    pub key: &'static str,
    pub tone: Tone,
}

const fn act(verb: Verb, label: &'static str, key: &'static str) -> Action {
    Action {
        verb,
        label,
        key,
        tone: Tone::Normal,
    }
}

const fn danger(verb: Verb, label: &'static str, key: &'static str) -> Action {
    Action {
        tone: Tone::Danger,
        ..act(verb, label, key)
    }
}

/// An object's table.
const OBJECT: [Action; 4] = [
    act(Verb::Copy, "Copy", "Ctrl+C"),
    act(Verb::Paste, "Paste", "Ctrl+V"),
    act(Verb::Duplicate, "Duplicate", "Ctrl+D"),
    danger(Verb::Delete, "Delete", "Del"),
];

/// Empty space's table — nothing, for now.
const EMPTY: [Action; 0] = [];

/// The rows a target's Home carries, in order.
pub fn actions(target: Target) -> &'static [Action] {
    match target {
        Target::Empty => &EMPTY,
        Target::Object(_) => &OBJECT,
    }
}

/// Whether a verb has anything to work on right now — the table says
/// where a verb belongs, this says whether it is lit.
pub fn enabled(verb: Verb, e: &Editor) -> bool {
    match verb {
        Verb::Paste => e.has_clipboard(),
        Verb::Copy | Verb::Duplicate | Verb::Delete => !e.selection().is_empty(),
    }
}

/// What Home is titled: the object (or how many are selected), nothing
/// for empty space.
pub fn title(target: Target, e: &Editor) -> String {
    match target {
        Target::Empty => String::new(),
        Target::Object(_) => {
            let names: Vec<String> = e.selection().iter().map(|&i| e.display_name(i)).collect();
            crate::status::selection(&names)
        }
    }
}

/// A target's rows as they stand for the editor's state.
pub fn rows(target: Target, e: &Editor) -> Vec<super::page::Row> {
    actions(target)
        .iter()
        .map(|a| super::page::Row {
            verb: a.verb,
            label: a.label,
            key: a.key,
            tone: a.tone,
            enabled: enabled(a.verb, e),
        })
        .collect()
}

impl Studio {
    /// Do what a Home row says. `at` is where the menu was opened, in
    /// canvas units — where a paste lands. True when the document changed.
    pub(crate) fn context_verb(&mut self, verb: Verb, at: [f32; 2]) -> bool {
        let changed = match verb {
            Verb::Copy => self.editor.copy_objects(),
            Verb::Paste => self.editor.paste_objects(at),
            Verb::Duplicate => self.editor.duplicate_selected(),
            Verb::Delete => self.editor.delete_selected(),
        };
        self.request_redraw();
        changed
    }
}
