//! Home: the panel with Move armed — the verbs for what is selected
//! (Alva's call over colour or an Add list, 2026-08-31). Each row is a
//! thing the keyboard already does, with its shortcut down the right, so
//! the menu doubles as the honest list of what a selection can have done
//! to it. A verb acts and the menu closes, the way every context menu
//! does.

use spark_render::ShapeKind;

use crate::Studio;
use crate::editor::Editor;

/// What a Home row does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verb {
    Duplicate,
    Delete,
    /// Flip the primary's visibility (the eye on its track row).
    Hide,
    CopyStyle,
    PasteStyle,
    /// Wrap the selection in a folder.
    Folder,
    Merge,
    Unmerge,
    /// Convert the primary shape to an editable path.
    ToPath,
    /// Make Comp from Selection.
    MakeComp,
}

/// Home's contents for a selection: the title, and each row as
/// `(verb, label, shortcut, enabled)`.
pub struct HomeState {
    pub title: String,
    pub rows: Vec<(Verb, String, &'static str, bool)>,
}

/// What Home shows for the editor's selection right now.
pub fn state(e: &Editor) -> HomeState {
    let sel = e.selection();
    let any = !sel.is_empty();
    let primary = e.primary();
    let hidden = primary.is_some_and(|i| e.is_hidden(i));
    let merged = primary.is_some_and(|i| e.groups().get(i).copied().unwrap_or(0) != 0);
    let can_path = primary.is_some_and(|i| {
        e.shapes().get(i).is_some_and(|s| {
            matches!(
                s.kind(),
                ShapeKind::Circle | ShapeKind::Box | ShapeKind::Ngon | ShapeKind::Line
            )
        })
    });
    let names: Vec<String> = sel.iter().map(|&i| e.display_name(i)).collect();
    let row = |v: Verb, label: &str, key: &'static str, on: bool| (v, label.to_string(), key, on);
    HomeState {
        title: crate::status::selection(&names),
        rows: vec![
            row(Verb::Duplicate, "Duplicate", "Ctrl+D", any),
            row(Verb::Delete, "Delete", "Del", any),
            row(Verb::Hide, if hidden { "Show" } else { "Hide" }, "", any),
            row(Verb::CopyStyle, "Copy Style", "Ctrl+C", any),
            row(
                Verb::PasteStyle,
                "Paste Style",
                "Ctrl+V",
                any && e.has_style_clip(),
            ),
            row(Verb::Folder, "Folder", "Ctrl+Shift+N", any),
            if merged {
                row(Verb::Unmerge, "Unmerge", "Ctrl+Shift+G", true)
            } else {
                row(Verb::Merge, "Merge", "Ctrl+G", sel.len() >= 2)
            },
            row(Verb::ToPath, "Convert to Path", "P", can_path),
            row(Verb::MakeComp, "Make Comp", "Ctrl+Shift+C", any),
        ],
    }
}

impl Studio {
    /// Do what a Home row says. True when the document changed.
    pub(crate) fn context_verb(&mut self, verb: Verb) -> bool {
        let changed = match verb {
            Verb::Duplicate => self.editor.duplicate_selected(),
            Verb::Delete => self.editor.delete_selected(),
            Verb::Hide => match self.editor.primary() {
                Some(i) => self.editor.toggle_hidden(i),
                None => false,
            },
            Verb::CopyStyle => self.editor.copy_style(),
            Verb::PasteStyle => self.editor.paste_style(),
            Verb::Folder => self.editor.new_folder_from_selection(),
            Verb::Merge => self.editor.merge_selected(),
            Verb::Unmerge => self.editor.unmerge_selected(),
            Verb::ToPath => self.editor.convert_to_path(),
            Verb::MakeComp => self.make_comp_from_selection(),
        };
        self.request_redraw();
        changed
    }
}
