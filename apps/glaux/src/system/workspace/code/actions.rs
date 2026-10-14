//! What the Code view's context menus offer, and carrying it out through
//! the same paths as the keyboard shortcuts.

use glaux_input::{InputEvent, Point};

use super::{CodeEditor, CodeRequest, close_tab};
use crate::system::code_document::DocumentId;
use crate::system::code_session::CodeSession;
use crate::system::workspace::context_menu::{ContextMenu, MenuEntry};

/// An action a Code view menu runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CodeAction {
    Undo,
    Redo,
    Cut,
    Copy,
    Paste,
    SelectAll,
    Find,
    Replace,
    GoToLine,
    NewFile,
    OpenFile,
    CloseTab(DocumentId),
}

impl CodeAction {
    /// The input that does the same from the keyboard, for actions that
    /// have one.
    fn shortcut_event(self) -> Option<InputEvent> {
        Some(match self {
            Self::Undo => InputEvent::Undo,
            Self::Redo => InputEvent::Redo,
            Self::Cut => InputEvent::Cut,
            Self::Copy => InputEvent::Copy,
            Self::Paste => InputEvent::Paste,
            Self::SelectAll => InputEvent::SelectAll,
            Self::Find => InputEvent::Find,
            Self::Replace => InputEvent::Replace,
            Self::GoToLine => InputEvent::GoToLine,
            Self::NewFile => InputEvent::NewFile,
            Self::OpenFile => InputEvent::OpenFile,
            Self::CloseTab(_) => return None,
        })
    }
}

/// The menu for the active document's text, with what doesn't apply to
/// `session` as it is disabled.
pub fn text_menu(session: &CodeSession, at: Point) -> ContextMenu<CodeAction> {
    let id = session.tabs().active();
    let can = |check: fn(&CodeSession, DocumentId) -> bool| id.is_some_and(|id| check(session, id));
    let selected = id
        .and_then(|id| session.tabs().selection(id))
        .is_some_and(|selection| !selection.is_empty());
    ContextMenu::new(
        vec![
            MenuEntry::when(
                can(CodeSession::can_undo),
                "Undo",
                Some("Ctrl+Z"),
                CodeAction::Undo,
            ),
            MenuEntry::when(
                can(CodeSession::can_redo),
                "Redo",
                Some("Ctrl+Y"),
                CodeAction::Redo,
            ),
            MenuEntry::Separator,
            MenuEntry::when(selected, "Cut", Some("Ctrl+X"), CodeAction::Cut),
            MenuEntry::when(selected, "Copy", Some("Ctrl+C"), CodeAction::Copy),
            MenuEntry::item("Paste", Some("Ctrl+V"), CodeAction::Paste),
            MenuEntry::Separator,
            MenuEntry::item("Select all", Some("Ctrl+A"), CodeAction::SelectAll),
            MenuEntry::Separator,
            MenuEntry::item("Find...", Some("Ctrl+F"), CodeAction::Find),
            MenuEntry::item("Replace...", Some("Ctrl+H"), CodeAction::Replace),
            MenuEntry::item("Go to line...", Some("Ctrl+G"), CodeAction::GoToLine),
        ],
        at,
    )
}

/// The menu for the tab strip and the empty view: files to create or
/// open, and closing `tab` when the press landed on one.
pub fn file_menu(tab: Option<DocumentId>, at: Point) -> ContextMenu<CodeAction> {
    let mut entries = vec![
        MenuEntry::item("New file...", Some("Ctrl+N"), CodeAction::NewFile),
        MenuEntry::item("Open file...", Some("Ctrl+P"), CodeAction::OpenFile),
    ];
    if let Some(id) = tab {
        entries.push(MenuEntry::Separator);
        entries.push(MenuEntry::item(
            "Close tab",
            Some("Ctrl+W"),
            CodeAction::CloseTab(id),
        ));
    }
    ContextMenu::new(entries, at)
}

impl CodeEditor {
    /// Carries out `action` as its keyboard shortcut would.
    pub(super) fn run_action(
        &mut self,
        action: CodeAction,
        session: &mut CodeSession,
    ) -> Option<CodeRequest> {
        match action.shortcut_event() {
            Some(event) => self.input(event, session),
            None => match action {
                CodeAction::CloseTab(id) => close_tab(session, id),
                _ => None,
            },
        }
    }
}
