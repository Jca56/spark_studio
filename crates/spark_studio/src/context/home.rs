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
//!
//! In the clip view (Alva's spec, 2026-09-01) a key — or the picked
//! set, or a strip moment — offers its value in a box you can type in,
//! a Linear|Smooth switch, and Copy, Cut, Paste, Delete; a setting's
//! row offers the same four for the whole curve; the graph's air
//! offers Paste, landing where you clicked. The page itself is the
//! clip view's (`Page::keys`); the tables live here with the rest.

use crate::Studio;
use crate::editor::Editor;

/// What was under the cursor when the menu opened.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub enum Target {
    /// Empty canvas, or an empty panel.
    #[default]
    Empty,
    /// An object on the canvas, by id — the right-click selected it, so
    /// the verbs act on the selection it belongs to.
    Object(u32),
    /// What is picked in the clip view — the key, the set, the moment —
    /// read live from the view, the way an object's verbs read the
    /// selection. `at` is the local time under the click.
    Keys { at: f32 },
    /// A setting's row in the clip view: its whole curve.
    Row(crate::anim::Target),
    /// The clip view's graph, at a local time.
    Graph { at: f32 },
    /// The arrangement's time axis: the grid, and the loop.
    Timeline,
}

impl Target {
    /// Whether the menu opened in the clip view.
    pub fn in_clip_view(self) -> bool {
        matches!(self, Target::Keys { .. } | Target::Row(_) | Target::Graph { .. })
    }
}

/// What a Home row does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verb {
    Copy,
    Cut,
    Paste,
    Duplicate,
    Delete,
    /// The timeline's: drop the loop region.
    ClearLoop,
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

/// The picked keys' table, and a setting row's: the same four verbs,
/// on the pick or on the whole curve.
const KEYS: [Action; 4] = [
    act(Verb::Copy, "Copy", "Ctrl+C"),
    act(Verb::Cut, "Cut", "Ctrl+X"),
    act(Verb::Paste, "Paste", "Ctrl+V"),
    danger(Verb::Delete, "Delete", "Del"),
];

/// The graph's air: a paste lands where you clicked.
const GRAPH: [Action; 1] = [act(Verb::Paste, "Paste here", "")];

/// The timeline's, under its grid switch.
const TIMELINE: [Action; 1] = [act(Verb::ClearLoop, "Clear loop", "")];

/// The rows a target's Home carries, in order.
pub fn actions(target: Target) -> &'static [Action] {
    match target {
        Target::Empty => &EMPTY,
        Target::Object(_) => &OBJECT,
        Target::Keys { .. } | Target::Row(_) => &KEYS,
        Target::Graph { .. } => &GRAPH,
        Target::Timeline => &TIMELINE,
    }
}

/// Whether a verb has anything to work on right now — the table says
/// where a verb belongs, this says whether it is lit. In the clip view
/// Paste wants copied keys, and the rest want the pick the menu opened
/// on, which is always there.
pub fn enabled(target: Target, verb: Verb, e: &Editor) -> bool {
    if target.in_clip_view() {
        return match verb {
            Verb::Paste => e.key_clip().is_some_and(|k| !k.is_empty()),
            _ => true,
        };
    }
    match verb {
        Verb::Paste => e.has_clipboard(),
        Verb::Cut => false,
        Verb::Copy | Verb::Duplicate | Verb::Delete => !e.selection().is_empty(),
        // The studio knows whether there is a loop; the page asks it.
        Verb::ClearLoop => true,
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
        // The clip view titles its own pages.
        Target::Keys { .. } | Target::Row(_) | Target::Graph { .. } => String::new(),
        Target::Timeline => "Timeline".to_string(),
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
            enabled: enabled(target, a.verb, e),
        })
        .collect()
}

impl Studio {
    /// Do what a Home row says. `at` is where the menu was opened, in
    /// canvas units — where a paste lands. True when the document changed.
    pub(crate) fn context_verb(&mut self, target: Target, verb: Verb, at: [f32; 2]) -> bool {
        if target.in_clip_view() {
            let changed = self.clip_view_verb(target, verb);
            self.request_redraw();
            return changed;
        }
        let changed = match verb {
            Verb::Copy => self.editor.copy_objects(),
            Verb::Paste => self.editor.paste_objects(at),
            Verb::Duplicate => self.editor.duplicate_selected(),
            Verb::Delete => self.editor.delete_selected(),
            Verb::ClearLoop => self.clear_loop(),
            Verb::Cut => false,
        };
        self.request_redraw();
        changed
    }
}
