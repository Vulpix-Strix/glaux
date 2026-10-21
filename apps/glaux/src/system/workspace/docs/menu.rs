//! What the documentation browser offers the menu bar.
// Only the macOS menu bar asks.
#![cfg_attr(not(target_os = "macos"), allow(dead_code))]

use super::DocsBrowser;
use crate::system::menu_state::MenuState;
use crate::system::workspace::menu::in_field;

impl DocsBrowser {
    /// The menu commands the browser acts on. SEARCH DOCS takes every
    /// event and edits its query. The list of tabs takes every event and
    /// uses none of them.
    pub fn menu_state(&self) -> MenuState {
        if let Some(search) = &self.search {
            return in_field(search.field());
        }
        if self.menu.is_some() {
            return MenuState::default();
        }
        MenuState {
            close_document: !self.tabs.is_empty(),
            find: true,
            navigate_back: self.can_go(true),
            navigate_forward: self.can_go(false),
            toggle_sidebar: true,
            ..MenuState::default()
        }
    }
}
