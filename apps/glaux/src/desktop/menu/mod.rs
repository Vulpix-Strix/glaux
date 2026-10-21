//! The macOS menu bar.
//!
//! Every command that has a keyboard shortcut sends the event the shortcut
//! sends, so the menu and the keyboard cannot disagree about what a command
//! does. The accelerators all use Command and the host reads only Ctrl, so
//! both work side by side and a game never sees a key a menu item took.

mod macos;

pub use macos::NativeMenu;

use glaux_input::InputEvent;
use muda::accelerator::{Code, Modifiers};

use crate::system::MenuState;

/// An item of the menu bar that Glaux handles itself. The rest of the bar,
/// such as Quit and Hide, is AppKit's.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MenuCommand {
    NewFile,
    OpenFile,
    CloseDocument,
    Save,
    Undo,
    Redo,
    Cut,
    Copy,
    Paste,
    SelectAll,
    Find,
    Replace,
    GoToLine,
    ToggleSidebar,
    /// Opens the documentation browser on its front page. F1 is not the
    /// same: in the Code view it looks up the symbol under the caret.
    Documentation,
    Run,
    Stop,
    Restart,
    ToggleLogs,
}

impl MenuCommand {
    pub const ALL: [Self; 19] = [
        Self::NewFile,
        Self::OpenFile,
        Self::CloseDocument,
        Self::Save,
        Self::Undo,
        Self::Redo,
        Self::Cut,
        Self::Copy,
        Self::Paste,
        Self::SelectAll,
        Self::Find,
        Self::Replace,
        Self::GoToLine,
        Self::ToggleSidebar,
        Self::Documentation,
        Self::Run,
        Self::Stop,
        Self::Restart,
        Self::ToggleLogs,
    ];

    pub fn id(self) -> &'static str {
        match self {
            Self::NewFile => "file.new",
            Self::OpenFile => "file.open",
            Self::CloseDocument => "file.close",
            Self::Save => "file.save",
            Self::Undo => "edit.undo",
            Self::Redo => "edit.redo",
            Self::Cut => "edit.cut",
            Self::Copy => "edit.copy",
            Self::Paste => "edit.paste",
            Self::SelectAll => "edit.select-all",
            Self::Find => "edit.find",
            Self::Replace => "edit.replace",
            Self::GoToLine => "edit.go-to-line",
            Self::ToggleSidebar => "view.toggle-sidebar",
            Self::Documentation => "help.documentation",
            Self::Run => "run.run",
            Self::Stop => "run.stop",
            Self::Restart => "run.restart",
            Self::ToggleLogs => "run.toggle-logs",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::NewFile => "New File",
            Self::OpenFile => "Open File",
            Self::CloseDocument => "Close Document",
            Self::Save => "Save",
            Self::Undo => "Undo",
            Self::Redo => "Redo",
            Self::Cut => "Cut",
            Self::Copy => "Copy",
            Self::Paste => "Paste",
            Self::SelectAll => "Select All",
            Self::Find => "Find",
            Self::Replace => "Replace",
            Self::GoToLine => "Go to Line",
            Self::ToggleSidebar => "Toggle Sidebar",
            Self::Documentation => "Documentation",
            Self::Run => "Run",
            Self::Stop => "Stop",
            Self::Restart => "Restart",
            Self::ToggleLogs => "Toggle Logs",
        }
    }

    pub fn shortcut(self) -> (Modifiers, Code) {
        let command = Modifiers::META;
        match self {
            Self::NewFile => (command, Code::KeyN),
            Self::OpenFile => (command, Code::KeyO),
            Self::CloseDocument => (command, Code::KeyW),
            Self::Save => (command, Code::KeyS),
            Self::Undo => (command, Code::KeyZ),
            Self::Redo => (command | Modifiers::SHIFT, Code::KeyZ),
            Self::Cut => (command, Code::KeyX),
            Self::Copy => (command, Code::KeyC),
            Self::Paste => (command, Code::KeyV),
            Self::SelectAll => (command, Code::KeyA),
            Self::Find => (command, Code::KeyF),
            Self::Replace => (command | Modifiers::ALT, Code::KeyF),
            Self::GoToLine => (command, Code::KeyL),
            Self::ToggleSidebar => (command, Code::KeyB),
            Self::Documentation => (command | Modifiers::SHIFT, Code::Slash),
            Self::Run => (command, Code::KeyR),
            Self::Stop => (command, Code::Period),
            Self::Restart => (command | Modifiers::SHIFT, Code::KeyR),
            Self::ToggleLogs => (command | Modifiers::SHIFT, Code::KeyL),
        }
    }

    /// The event the command's keyboard shortcut sends. `None` for
    /// [`Documentation`](Self::Documentation), which has no event.
    pub fn input_event(self) -> Option<InputEvent> {
        Some(match self {
            Self::NewFile => InputEvent::NewFile,
            Self::OpenFile => InputEvent::OpenFile,
            Self::CloseDocument => InputEvent::CloseDocument,
            Self::Save => InputEvent::Save,
            Self::Undo => InputEvent::Undo,
            Self::Redo => InputEvent::Redo,
            Self::Cut => InputEvent::Cut,
            Self::Copy => InputEvent::Copy,
            Self::Paste => InputEvent::Paste,
            Self::SelectAll => InputEvent::SelectAll,
            Self::Find => InputEvent::Find,
            Self::Replace => InputEvent::Replace,
            Self::GoToLine => InputEvent::GoToLine,
            Self::ToggleSidebar => InputEvent::ToggleSidebar,
            Self::Documentation => return None,
            Self::Run => InputEvent::Run,
            Self::Stop => InputEvent::Stop,
            Self::Restart => InputEvent::Restart,
            Self::ToggleLogs => InputEvent::ToggleLogs,
        })
    }

    /// Whether `state` offers the command.
    pub fn is_enabled(self, state: MenuState) -> bool {
        match self {
            Self::NewFile => state.new_file,
            Self::OpenFile => state.open_file,
            Self::CloseDocument => state.close_document,
            Self::Save => state.save,
            Self::Undo => state.undo,
            Self::Redo => state.redo,
            Self::Cut => state.cut,
            Self::Copy => state.copy,
            Self::Paste => state.paste,
            Self::SelectAll => state.select_all,
            Self::Find => state.find,
            Self::Replace => state.replace,
            Self::GoToLine => state.go_to_line,
            Self::ToggleSidebar => state.toggle_sidebar,
            Self::Documentation => state.documentation,
            Self::Run => state.run,
            Self::Stop => state.stop,
            Self::Restart => state.restart,
            Self::ToggleLogs => state.toggle_logs,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn ids_and_shortcuts_are_unique() {
        let ids: HashSet<_> = MenuCommand::ALL.iter().map(|c| c.id()).collect();
        assert_eq!(ids.len(), MenuCommand::ALL.len());
        let shortcuts: HashSet<_> = MenuCommand::ALL
            .iter()
            .map(|c| {
                let (mods, code) = c.shortcut();
                (mods.bits(), code)
            })
            .collect();
        assert_eq!(shortcuts.len(), MenuCommand::ALL.len());
    }

    #[test]
    fn every_shortcut_uses_command() {
        for command in MenuCommand::ALL {
            let (mods, _) = command.shortcut();
            assert!(mods.contains(Modifiers::META), "{command:?}");
        }
    }

    #[test]
    fn commands_send_the_events_of_their_keyboard_shortcuts() {
        let sent: Vec<_> = MenuCommand::ALL.map(MenuCommand::input_event).to_vec();
        assert_eq!(
            sent,
            vec![
                Some(InputEvent::NewFile),
                Some(InputEvent::OpenFile),
                Some(InputEvent::CloseDocument),
                Some(InputEvent::Save),
                Some(InputEvent::Undo),
                Some(InputEvent::Redo),
                Some(InputEvent::Cut),
                Some(InputEvent::Copy),
                Some(InputEvent::Paste),
                Some(InputEvent::SelectAll),
                Some(InputEvent::Find),
                Some(InputEvent::Replace),
                Some(InputEvent::GoToLine),
                Some(InputEvent::ToggleSidebar),
                None,
                Some(InputEvent::Run),
                Some(InputEvent::Stop),
                Some(InputEvent::Restart),
                Some(InputEvent::ToggleLogs),
            ]
        );
    }

    #[test]
    fn documentation_is_not_f1() {
        assert_eq!(MenuCommand::Documentation.input_event(), None);
    }

    #[test]
    fn each_command_reads_its_own_field_of_the_state() {
        for command in MenuCommand::ALL {
            let others = MenuCommand::ALL
                .into_iter()
                .filter(|&other| other != command && other.is_enabled(only(command)))
                .count();
            assert!(command.is_enabled(only(command)), "{command:?}");
            assert_eq!(others, 0, "{command:?}");
        }
    }

    /// A state that offers `command` and nothing else.
    fn only(command: MenuCommand) -> MenuState {
        let mut state = MenuState::default();
        match command {
            MenuCommand::NewFile => state.new_file = true,
            MenuCommand::OpenFile => state.open_file = true,
            MenuCommand::CloseDocument => state.close_document = true,
            MenuCommand::Save => state.save = true,
            MenuCommand::Undo => state.undo = true,
            MenuCommand::Redo => state.redo = true,
            MenuCommand::Cut => state.cut = true,
            MenuCommand::Copy => state.copy = true,
            MenuCommand::Paste => state.paste = true,
            MenuCommand::SelectAll => state.select_all = true,
            MenuCommand::Find => state.find = true,
            MenuCommand::Replace => state.replace = true,
            MenuCommand::GoToLine => state.go_to_line = true,
            MenuCommand::ToggleSidebar => state.toggle_sidebar = true,
            MenuCommand::Documentation => state.documentation = true,
            MenuCommand::Run => state.run = true,
            MenuCommand::Stop => state.stop = true,
            MenuCommand::Restart => state.restart = true,
            MenuCommand::ToggleLogs => state.toggle_logs = true,
        }
        state
    }
}
