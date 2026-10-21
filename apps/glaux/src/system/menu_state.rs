//! Which menu commands the screen in front of the user would act on.
//!
//! The native menu bar enables an item only when the surface that would
//! receive its event does something with it. The surfaces work this out
//! themselves, next to the code that handles the events, so the answers
//! follow their behavior.
// Only the macOS menu bar asks.
#![cfg_attr(not(target_os = "macos"), allow(dead_code))]

use super::{System, SystemState};

/// The commands of the menu bar the system would act on if they arrived
/// now. Each command is the [`glaux_input::InputEvent`] its keyboard
/// shortcut sends, except `documentation`, which opens the browser on its
/// front page and is not F1.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct MenuState {
    pub new_file: bool,
    pub open_file: bool,
    pub close_document: bool,
    pub save: bool,
    pub undo: bool,
    pub redo: bool,
    pub cut: bool,
    pub copy: bool,
    pub paste: bool,
    pub select_all: bool,
    pub find: bool,
    pub replace: bool,
    pub go_to_line: bool,
    pub toggle_sidebar: bool,
    pub documentation: bool,
}

impl System {
    /// The menu commands that would do something right now.
    ///
    /// Nothing is available while the question about unsaved edits is open
    /// or quitting is under way, since neither takes anything else. The
    /// shell only offers the documentation, and a running or paused
    /// cartridge offers nothing, so its keys reach the game.
    pub fn menu_state(&self) -> MenuState {
        if self.unsaved.is_some() || self.exit_requested || self.exiting {
            return MenuState::default();
        }
        match &self.state {
            SystemState::Shell { .. } => MenuState {
                documentation: true,
                ..MenuState::default()
            },
            SystemState::Docs { .. } => self.workspace.docs_apart_menu_state(),
            SystemState::Workspace { .. } => self
                .workspace
                .menu_state(self.sprites.as_ref(), self.code.as_ref()),
            SystemState::Boot { .. }
            | SystemState::Credits { .. }
            | SystemState::Running { .. }
            | SystemState::Paused { .. } => MenuState::default(),
        }
    }
}
