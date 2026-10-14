//! The editing keys of the Code view.

use glaux_input::InputEvent;
use glaux_machine::font::BitmapFont;

use super::{CodeEditor, UNDO_IDLE_TICKS};
use crate::system::code_cursor::Caret;
use crate::system::code_document::{CodeDocument, DocumentId, EditError};
use crate::system::code_edit::{self, Edit};
use crate::system::code_history::EditKind;
use crate::system::code_selection::Selection;
use crate::system::code_session::CodeSession;
use crate::system::code_text::MetricsKey;

impl CodeEditor {
    /// Applies `event` to the active document if it is an editing key,
    /// returning whether it was one.
    pub(super) fn edit_input(&mut self, event: InputEvent, session: &mut CodeSession) -> bool {
        match event {
            InputEvent::Text(ch) => self.edit(session, EditKind::Typing, |_, selection, _, _| {
                code_edit::typed(selection, ch)
            }),
            InputEvent::Enter => {
                self.edit(
                    session,
                    EditKind::Structural,
                    |document, selection, _, _| Some(code_edit::newline(document, selection)),
                );
            }
            InputEvent::Backspace => {
                self.edit(session, EditKind::Backspace, |document, selection, _, _| {
                    code_edit::backspace(document, selection)
                });
            }
            InputEvent::Delete => {
                self.edit(session, EditKind::Delete, |document, selection, _, _| {
                    code_edit::delete(document, selection)
                });
            }
            InputEvent::Tab => {
                self.edit(
                    session,
                    EditKind::Structural,
                    |document, selection, key, font| {
                        Some(code_edit::tab(document, selection, key, font))
                    },
                );
            }
            InputEvent::Undo => self.step_history(session, CodeSession::undo),
            InputEvent::Redo => self.step_history(session, CodeSession::redo),
            _ => return false,
        }
        true
    }

    /// Makes the edit `edit` works out for the active document and its
    /// selection, then leaves the caret after the inserted text with
    /// nothing selected, and scrolls as little as it takes to show it.
    /// Does nothing until the document's measurements describe the view.
    ///
    /// The edit is recorded as `kind`, except that Backspace and Delete
    /// over a selection are steps of their own. An edit after a pause of
    /// [`UNDO_IDLE_TICKS`] starts a new undo step.
    fn edit(
        &mut self,
        session: &mut CodeSession,
        kind: EditKind,
        edit: impl FnOnce(&CodeDocument, Selection, MetricsKey, &BitmapFont) -> Option<Edit>,
    ) {
        let Some(document) = session.active_document() else {
            return;
        };
        let id = document.id();
        let key = self.geometry(document).metrics_key();
        let tabs = session.tabs();
        let (Some(selection), Some(metrics)) = (tabs.selection(id), tabs.metrics(id)) else {
            return;
        };
        if !metrics.describes(document, key) {
            return;
        }
        let kind = match kind {
            EditKind::Backspace | EditKind::Delete if !selection.is_empty() => EditKind::Structural,
            kind => kind,
        };
        let Some(Edit { range, text }) = edit(document, selection, key, &self.layout.font) else {
            return;
        };
        if self.idle >= UNDO_IDLE_TICKS {
            session.seal_history(id);
        }
        let replaced = match session.edit(id, range, &text, kind) {
            Some(Ok(replaced)) => replaced,
            // Deleting up to a U+FEFF at the start is the one refusal a
            // user can cause; any other means a range was worked out wrong.
            Some(Err(err @ EditError::CreatesBom)) => {
                tracing::debug!(error = %err, "code edit refused");
                return;
            }
            Some(Err(err)) => {
                tracing::warn!(error = %err, "code edit refused");
                return;
            }
            None => return,
        };
        self.idle = 0;
        let Some(document) = session.document(id) else {
            return;
        };
        let selection = Selection::collapsed(Caret::at(document, replaced.range.end));
        self.show_changed_text(session, id, selection);
    }

    /// Undoes or redoes, with `step`, the active document's history, and
    /// shows the selection it restores.
    fn step_history(
        &mut self,
        session: &mut CodeSession,
        step: fn(&mut CodeSession, DocumentId) -> Option<Selection>,
    ) {
        let Some(id) = session.tabs().active() else {
            return;
        };
        if let Some(selection) = step(session, id) {
            self.show_changed_text(session, id, selection);
        }
    }

    /// Shows `selection` in `id`'s document after its text changed,
    /// measuring it again and scrolling as little as it takes to show the
    /// caret.
    fn show_changed_text(
        &mut self,
        session: &mut CodeSession,
        id: DocumentId,
        selection: Selection,
    ) {
        // Ranges a press or drag recorded name text that may have moved.
        self.selecting = None;
        self.last_click = None;
        session.tabs_mut().set_selection(id, selection);
        self.refresh_metrics(session);
        let view = self.revealing(session, id, selection.caret());
        self.show_selection(session, id, selection, view);
    }
}
