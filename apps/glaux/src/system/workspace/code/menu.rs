//! What the Code view offers the menu bar.
// Only the macOS menu bar asks.
#![cfg_attr(not(target_os = "macos"), allow(dead_code))]

use super::CodeEditor;
use super::edit::has_selection;
use crate::system::code_session::CodeSession;
use crate::system::menu_state::MenuState;
use crate::system::workspace::menu::in_field;

impl CodeEditor {
    /// The menu commands the view acts on, given what it shows now.
    ///
    /// An overlay takes every event. Only the prompts for a new file, a
    /// rename and a line number hold a text field, so only they take the
    /// editing commands. With the find bar's text focused the same goes
    /// for its field, while the commands that act on the document still
    /// reach it.
    pub fn menu_state(&self, session: Option<&CodeSession>) -> MenuState {
        if self.has_overlay() {
            return self.overlay_field().map(in_field).unwrap_or_default();
        }
        let Some(session) = session else {
            return MenuState::default();
        };
        let mut state = MenuState {
            new_file: true,
            open_file: true,
            ..MenuState::default()
        };
        let Some(document) = session.active_document() else {
            return state;
        };
        let id = document.id();
        state.close_document = true;
        state.save = document.is_modified();
        state.find = true;
        state.replace = true;
        state.go_to_line = true;
        match self.find.as_ref().filter(|bar| bar.focus().is_some()) {
            Some(bar) => {
                let editing = bar.focused_field().map(in_field).unwrap_or_default();
                MenuState {
                    select_all: editing.select_all,
                    cut: editing.cut,
                    copy: editing.copy,
                    paste: editing.paste,
                    ..state
                }
            }
            None => MenuState {
                undo: session.can_undo(id),
                redo: session.can_redo(id),
                cut: has_selection(session),
                copy: has_selection(session),
                paste: true,
                select_all: true,
                ..state
            },
        }
    }

    /// The text field of the overlay that has the keys, if it has one.
    /// The overlays are tried in the order [`CodeEditor::input`] does.
    fn overlay_field(&self) -> Option<&crate::system::workspace::text_field::TextField> {
        if self.settings_dialog.is_some()
            || self.picker.is_some()
            || self.menu.is_some()
            || self.context_menu.is_some()
        {
            return None;
        }
        let prompt = if let Some(new_file) = &self.new_file {
            new_file.prompt()
        } else if let Some(rename) = &self.rename {
            rename.prompt()
        } else {
            self.go_to.as_ref()?.prompt()
        };
        Some(prompt.field())
    }
}
