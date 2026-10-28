//! The findings of the external analyzer, as the editor lets the user reach
//! them.

use super::CodeEditor;
use crate::system::editing::cursor::Caret;
use crate::system::editing::diagnostics::{EditorDiagnostic, Severity};
use crate::system::editing::selection::Selection;
use crate::system::editing::session::CodeSession;

impl CodeEditor {
    /// Moves the caret to the start of the next finding after it, or the
    /// previous one before it. Past the last finding it wraps to the first.
    /// Does nothing when the active document has none.
    pub(super) fn next_problem(&mut self, session: &mut CodeSession, reverse: bool) {
        let Some(document) = session.active_document() else {
            return;
        };
        let id = document.id();
        let found = session.diagnostics(id);
        let caret = session
            .tabs()
            .selection(id)
            .unwrap_or_default()
            .caret()
            .byte();
        let target = if reverse {
            found
                .iter()
                .rev()
                .find(|d| d.range.start < caret)
                .or(found.last())
        } else {
            found
                .iter()
                .find(|d| d.range.start > caret)
                .or(found.first())
        };
        let Some(start) = target.map(|d| d.range.start) else {
            return;
        };
        let selection = Selection::collapsed(Caret::at(document, start));
        session.tabs_mut().set_selection(id, selection);
        self.show_arrival(session, id, None);
    }

    /// The most serious finding that covers the caret, if any.
    pub(super) fn problem_at_caret<'a>(
        &self,
        session: &'a CodeSession,
    ) -> Option<&'a EditorDiagnostic> {
        let document = session.active_document()?;
        let id = document.id();
        let caret = session.tabs().selection(id)?.caret().byte();
        session
            .diagnostics(id)
            .iter()
            .filter(|d| d.range.contains(&caret) || (d.range.is_empty() && d.range.start == caret))
            .min_by_key(|d| d.severity)
    }
}

/// Whether the margin, the underline and the scroll rail show a finding.
/// Informational notes are only read in the status bar.
pub(super) fn is_marked(severity: Severity) -> bool {
    severity != Severity::Info
}
