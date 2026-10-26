//! What the workspace offers the menu bar.
// Only the macOS menu bar asks.
#![cfg_attr(not(target_os = "macos"), allow(dead_code))]

use super::text_field::TextField;
use super::{Route, Workspace};
use crate::system::editing::session::CodeSession;
use crate::system::menu_state::MenuState;
use crate::system::sprite_document::SpriteDocument;

/// What a focused text field takes. It keeps no history, so Undo and Redo
/// do nothing, and Copy and Cut need something selected.
pub(super) fn in_field(field: &TextField) -> MenuState {
    MenuState {
        select_all: !field.is_empty(),
        copy: field.has_selection(),
        cut: field.has_selection(),
        paste: true,
        ..MenuState::default()
    }
}

impl Workspace {
    /// The menu commands the view that would receive them acts on.
    ///
    /// Only that view counts. Documents the editors keep open behind a
    /// dialog, an overlay or the documentation browser are out of reach of
    /// the menu, as they are of the keyboard.
    pub fn menu_state(
        &self,
        sprites: Option<&SpriteDocument>,
        code: Option<&CodeSession>,
    ) -> MenuState {
        let documentation = MenuState {
            documentation: true,
            ..MenuState::default()
        };
        // Running comes from the Code and Sprites views, where F5 does the
        // same. The log panel opens wherever no dialog is up.
        let running = MenuState {
            run: true,
            toggle_logs: true,
            ..documentation
        };
        match self.route() {
            Route::Modal | Route::SpriteDialog => MenuState::default(),
            Route::CodeOverlay => self.code.menu_state(code),
            Route::Docs => self.docs.menu_state(),
            Route::Code => MenuState {
                documentation: true,
                run: true,
                toggle_logs: true,
                ..self.code.menu_state(code)
            },
            Route::Sprites => sprites.map_or(running, |document| MenuState {
                save: document.is_modified(),
                undo: document.can_undo(),
                redo: document.can_redo(),
                ..running
            }),
            Route::Inert => MenuState {
                toggle_logs: true,
                ..documentation
            },
        }
    }

    /// The same for the documentation browser shown apart from the
    /// workspace, where no editor is behind it.
    pub fn docs_apart_menu_state(&self) -> MenuState {
        self.docs.menu_state()
    }

    /// Shows the browser on its front page, as the Docs button of the tab
    /// bar does, if the view in front is not busy with a dialog or an
    /// overlay and the browser is not open already. Returns whether it
    /// opened.
    ///
    /// Unlike F1 in the Code view, it does not look at the caret.
    pub fn show_docs(&mut self, sprites: Option<&mut SpriteDocument>) -> bool {
        if !matches!(self.route(), Route::Code | Route::Sprites | Route::Inert) {
            return false;
        }
        self.toggle_docs(sprites);
        true
    }
}
