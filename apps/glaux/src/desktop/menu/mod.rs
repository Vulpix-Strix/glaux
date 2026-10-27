//! The macOS menu bar.
//!
//! Every command that has a keyboard shortcut sends the event the shortcut
//! sends, so the menu and the keyboard cannot disagree about what a command
//! does. The accelerators all use Command and the host reads only Ctrl, so
//! both work side by side and a game never sees a key a menu item took.

mod macos;

pub(crate) use macos::NativeMenu;

use glaux_input::InputEvent;
use muda::accelerator::{Code, Modifiers};

use crate::system::MenuState;

/// An item of the menu bar that Glaux handles itself. The rest of the bar,
/// such as Quit and Hide, is AppKit's.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum MenuCommand {
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
    GoToDefinition,
    GoToSymbol,
    /// Returns to the place before the last jump in the Code view, or to the
    /// previous page in the documentation.
    NavigateBack,
    NavigateForward,
    ToggleSidebar,
    /// Opens the documentation browser on its front page. F1 is not the
    /// same: in the Code view it looks up the symbol under the caret.
    Documentation,
    Run,
    Stop,
    Restart,
    /// Runs under the debugger, stopping at the breakpoints.
    Debug,
    ToggleBreakpoint,
    StepOver,
    StepInto,
    StepOut,
    ToggleLogs,
}

impl MenuCommand {
    pub(crate) const ALL: [Self; 28] = [
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
        Self::GoToDefinition,
        Self::GoToSymbol,
        Self::NavigateBack,
        Self::NavigateForward,
        Self::ToggleSidebar,
        Self::Documentation,
        Self::Run,
        Self::Stop,
        Self::Restart,
        Self::Debug,
        Self::ToggleBreakpoint,
        Self::StepOver,
        Self::StepInto,
        Self::StepOut,
        Self::ToggleLogs,
    ];

    pub(crate) fn id(self) -> &'static str {
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
            Self::GoToDefinition => "go.definition",
            Self::GoToSymbol => "go.symbol",
            Self::NavigateBack => "go.back",
            Self::NavigateForward => "go.forward",
            Self::ToggleSidebar => "view.toggle-sidebar",
            Self::Documentation => "help.documentation",
            Self::Run => "run.run",
            Self::Stop => "run.stop",
            Self::Restart => "run.restart",
            Self::Debug => "run.debug",
            Self::ToggleBreakpoint => "run.toggle-breakpoint",
            Self::StepOver => "run.step-over",
            Self::StepInto => "run.step-into",
            Self::StepOut => "run.step-out",
            Self::ToggleLogs => "run.toggle-logs",
        }
    }

    pub(crate) fn label(self) -> &'static str {
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
            Self::GoToDefinition => "Go to Definition",
            Self::GoToSymbol => "Go to Symbol",
            Self::NavigateBack => "Back",
            Self::NavigateForward => "Forward",
            Self::ToggleSidebar => "Toggle Sidebar",
            Self::Documentation => "Documentation",
            Self::Run => "Run",
            Self::Stop => "Stop",
            Self::Restart => "Restart",
            Self::Debug => "Debug",
            Self::ToggleBreakpoint => "Toggle Breakpoint",
            Self::StepOver => "Step Over",
            Self::StepInto => "Step Into",
            Self::StepOut => "Step Out",
            Self::ToggleLogs => "Toggle Logs",
        }
    }

    pub(crate) fn shortcut(self) -> (Modifiers, Code) {
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
            Self::GoToDefinition => (command | Modifiers::SHIFT, Code::KeyD),
            Self::GoToSymbol => (command | Modifiers::SHIFT, Code::KeyO),
            Self::NavigateBack => (command, Code::BracketLeft),
            Self::NavigateForward => (command, Code::BracketRight),
            Self::ToggleSidebar => (command, Code::KeyB),
            Self::Documentation => (command | Modifiers::SHIFT, Code::Slash),
            Self::Run => (command, Code::KeyR),
            Self::Stop => (command, Code::Period),
            Self::Restart => (command | Modifiers::SHIFT, Code::KeyR),
            Self::Debug => (command | Modifiers::ALT, Code::KeyR),
            Self::ToggleBreakpoint => (command, Code::Backslash),
            Self::StepOver => (command, Code::Quote),
            Self::StepInto => (command, Code::Semicolon),
            Self::StepOut => (command | Modifiers::SHIFT, Code::Quote),
            Self::ToggleLogs => (command | Modifiers::SHIFT, Code::KeyL),
        }
    }

    /// The event the command's keyboard shortcut sends. `None` for
    /// [`Documentation`](Self::Documentation), which has no event.
    pub(crate) fn input_event(self) -> Option<InputEvent> {
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
            Self::GoToDefinition => InputEvent::GoToDefinition,
            Self::GoToSymbol => InputEvent::GoToSymbol,
            Self::NavigateBack => InputEvent::Back,
            Self::NavigateForward => InputEvent::Forward,
            Self::ToggleSidebar => InputEvent::ToggleSidebar,
            Self::Documentation => return None,
            Self::Run => InputEvent::Run,
            Self::Stop => InputEvent::Stop,
            Self::Restart => InputEvent::Restart,
            Self::Debug => InputEvent::Debug,
            Self::ToggleBreakpoint => InputEvent::ToggleBreakpoint,
            Self::StepOver => InputEvent::StepOver,
            Self::StepInto => InputEvent::StepInto,
            Self::StepOut => InputEvent::StepOut,
            Self::ToggleLogs => InputEvent::ToggleLogs,
        })
    }

    /// Whether `state` offers the command.
    pub(crate) fn is_enabled(self, state: MenuState) -> bool {
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
            Self::GoToDefinition => state.go_to_definition,
            Self::GoToSymbol => state.go_to_symbol,
            Self::NavigateBack => state.navigate_back,
            Self::NavigateForward => state.navigate_forward,
            Self::ToggleSidebar => state.toggle_sidebar,
            Self::Documentation => state.documentation,
            Self::Run => state.run,
            Self::Stop => state.stop,
            Self::Restart => state.restart,
            Self::Debug => state.debug,
            Self::ToggleBreakpoint => state.toggle_breakpoint,
            Self::StepOver => state.step_over,
            Self::StepInto => state.step_into,
            Self::StepOut => state.step_out,
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
                Some(InputEvent::GoToDefinition),
                Some(InputEvent::GoToSymbol),
                Some(InputEvent::Back),
                Some(InputEvent::Forward),
                Some(InputEvent::ToggleSidebar),
                None,
                Some(InputEvent::Run),
                Some(InputEvent::Stop),
                Some(InputEvent::Restart),
                Some(InputEvent::Debug),
                Some(InputEvent::ToggleBreakpoint),
                Some(InputEvent::StepOver),
                Some(InputEvent::StepInto),
                Some(InputEvent::StepOut),
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
            MenuCommand::GoToDefinition => state.go_to_definition = true,
            MenuCommand::GoToSymbol => state.go_to_symbol = true,
            MenuCommand::NavigateBack => state.navigate_back = true,
            MenuCommand::NavigateForward => state.navigate_forward = true,
            MenuCommand::ToggleSidebar => state.toggle_sidebar = true,
            MenuCommand::Documentation => state.documentation = true,
            MenuCommand::Run => state.run = true,
            MenuCommand::Stop => state.stop = true,
            MenuCommand::Restart => state.restart = true,
            MenuCommand::Debug => state.debug = true,
            MenuCommand::ToggleBreakpoint => state.toggle_breakpoint = true,
            MenuCommand::StepOver => state.step_over = true,
            MenuCommand::StepInto => state.step_into = true,
            MenuCommand::StepOut => state.step_out = true,
            MenuCommand::ToggleLogs => state.toggle_logs = true,
        }
        state
    }
}
