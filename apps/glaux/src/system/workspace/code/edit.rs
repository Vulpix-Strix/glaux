//! The editing keys of the Code view.

use glaux_input::InputEvent;
use glaux_machine::font::BitmapFont;

use super::{CodeEditor, CodeRequest, UNDO_IDLE_TICKS};
use crate::system::editing::cursor::Caret;
use crate::system::editing::document::{CodeDocument, DocumentId, EditError};
use crate::system::editing::edit::{self, Edit, IndentUnit, Reselecting};
use crate::system::editing::history::EditKind;
use crate::system::editing::metrics::MetricsKey;
use crate::system::editing::selection::Selection;
use crate::system::editing::session::CodeSession;
use crate::ui_text;

impl CodeEditor {
    /// Applies `event` to the active document if it is an editing key,
    /// returning `None` if it is not one, and otherwise the work it asks
    /// for, if any.
    pub(super) fn edit_input(
        &mut self,
        event: InputEvent,
        session: &mut CodeSession,
    ) -> Option<Option<CodeRequest>> {
        if session.is_read_only()
            && matches!(
                event,
                InputEvent::Text(_)
                    | InputEvent::Enter
                    | InputEvent::Backspace
                    | InputEvent::Delete
                    | InputEvent::Tab { .. }
                    | InputEvent::Undo
                    | InputEvent::Redo
                    | InputEvent::Cut
                    | InputEvent::Paste
            )
        {
            let notice = ui_text::get("code-read-only-while-debugging");
            return Some(Some(CodeRequest::Notice(notice)));
        }
        match event {
            InputEvent::Text(ch) => {
                self.edit(session, EditKind::Typing, |_, selection, _, _| {
                    edit::typed(selection, ch)
                });
            }
            InputEvent::Enter => {
                self.edit(
                    session,
                    EditKind::Structural,
                    |document, selection, _, _| Some(edit::newline(document, selection)),
                );
            }
            InputEvent::Backspace => {
                self.edit(session, EditKind::Backspace, |document, selection, _, _| {
                    edit::backspace(document, selection)
                });
            }
            InputEvent::Delete => {
                self.edit(session, EditKind::Delete, |document, selection, _, _| {
                    edit::delete(document, selection)
                });
            }
            InputEvent::Tab { reverse } => self.indent(session, reverse),
            InputEvent::Undo => self.step_history(session, CodeSession::undo),
            InputEvent::Redo => self.step_history(session, CodeSession::redo),
            InputEvent::Copy => return Some(selected_text(session).map(CodeRequest::Copy)),
            InputEvent::Cut => {
                let text = selected_text(session);
                if text.is_some() {
                    self.edit(
                        session,
                        EditKind::Structural,
                        |document, selection, _, _| edit::delete(document, selection),
                    );
                }
                return Some(text.map(CodeRequest::Copy));
            }
            InputEvent::Paste => {
                let editable = session.active_document().is_some();
                return Some(editable.then_some(CodeRequest::Paste));
            }
            _ => return None,
        }
        Some(None)
    }

    /// Inserts the clipboard's `text` over the active document's
    /// selection, as one undo step.
    pub(super) fn paste_into_document(&mut self, session: &mut CodeSession, text: &str) {
        self.edit(
            session,
            EditKind::Structural,
            |document, selection, _, _| edit::paste(document, selection, text),
        );
    }

    /// Tab and Shift+Tab. A selection holding a line break indents or
    /// outdents each line it touches, keeping them selected. Otherwise Tab
    /// inserts indentation and Shift+Tab outdents the caret's line.
    fn indent(&mut self, session: &mut CodeSession, reverse: bool) {
        let Some(document) = session.active_document() else {
            return;
        };
        let unit = IndentUnit::of(
            document,
            self.settings.indent,
            self.settings.indent_columns(),
        );
        let Some((id, selection, key)) = self.editable(session) else {
            return;
        };
        let Some(document) = session.document(id) else {
            return;
        };
        let lines = if reverse {
            edit::outdent_lines(document, selection, unit, key.tab_size)
        } else if edit::spans_lines(document, selection) {
            edit::indent_lines(document, selection, unit)
        } else {
            let edit = edit::tab(document, selection, unit, key, &self.layout.font);
            self.commit(session, id, edit, EditKind::Structural, None);
            return;
        };
        if let Some(Reselecting {
            edit,
            anchor,
            caret,
        }) = lines
        {
            self.commit(
                session,
                id,
                edit,
                EditKind::Structural,
                Some((anchor, caret)),
            );
        }
    }

    /// The active document's id, selection and measuring key, once its
    /// measurements describe the view.
    pub(super) fn editable(
        &self,
        session: &CodeSession,
    ) -> Option<(DocumentId, Selection, MetricsKey)> {
        let document = session.active_document()?;
        let id = document.id();
        let key = self.geometry(document).metrics_key();
        let tabs = session.tabs();
        let (selection, metrics) = (tabs.selection(id)?, tabs.metrics(id)?);
        metrics
            .describes(document, key)
            .then_some((id, selection, key))
    }

    /// Makes the edit `edit` works out for the active document and its
    /// selection, then leaves the caret after the inserted text with
    /// nothing selected, and scrolls as little as it takes to show it.
    /// Does nothing until the document's measurements describe the view.
    ///
    /// The edit is recorded as `kind`, except that Backspace and Delete
    /// over a selection are steps of their own.
    fn edit(
        &mut self,
        session: &mut CodeSession,
        kind: EditKind,
        edit: impl FnOnce(&CodeDocument, Selection, MetricsKey, &BitmapFont) -> Option<Edit>,
    ) {
        let Some((id, selection, key)) = self.editable(session) else {
            return;
        };
        let Some(document) = session.document(id) else {
            return;
        };
        let kind = match kind {
            EditKind::Backspace | EditKind::Delete if !selection.is_empty() => EditKind::Structural,
            kind => kind,
        };
        if let Some(edit) = edit(document, selection, key, &self.layout.font) {
            self.commit(session, id, edit, kind, None);
        }
    }

    /// Replaces text in `id` as `edit` says, recorded as a `kind` edit,
    /// then selects `after`, given as anchor and caret offsets in the
    /// edited text, or else leaves the caret after the inserted text, and
    /// scrolls as little as it takes to show it. Returns whether the
    /// document took the edit.
    ///
    /// An edit after a pause of [`UNDO_IDLE_TICKS`] starts a new undo
    /// step.
    pub(super) fn commit(
        &mut self,
        session: &mut CodeSession,
        id: DocumentId,
        edit: Edit,
        kind: EditKind,
        after: Option<(usize, usize)>,
    ) -> bool {
        let Edit { range, text } = edit;
        if self.idle >= UNDO_IDLE_TICKS {
            session.seal_history(id);
        }
        let replaced = match session.edit(id, range, &text, kind, after) {
            Some(Ok(replaced)) => replaced,
            // Deleting up to a U+FEFF at the start is the one refusal a
            // user can cause. Any other means a range was worked out wrong.
            Some(Err(err @ EditError::CreatesBom)) => {
                tracing::debug!(error = %err, "code edit refused");
                return false;
            }
            Some(Err(err)) => {
                tracing::warn!(error = %err, "code edit refused");
                return false;
            }
            None => return false,
        };
        self.idle = 0;
        let Some(document) = session.document(id) else {
            return true;
        };
        let selection = match after {
            Some((anchor, caret)) => {
                Selection::between(document, anchor, Caret::at(document, caret))
            }
            None => Selection::collapsed(Caret::at(document, replaced.range.end)),
        };
        self.show_changed_text(session, id, selection);
        true
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

/// Whether the active document has a selection to cut or copy.
pub(super) fn has_selection(session: &CodeSession) -> bool {
    session
        .active_document()
        .and_then(|document| session.tabs().selection(document.id()))
        .is_some_and(|selection| !selection.is_empty())
}

/// The text selected in the active document, `None` when nothing is.
pub(super) fn selected_text(session: &CodeSession) -> Option<String> {
    let document = session.active_document()?;
    let selection = session.tabs().selection(document.id())?;
    if selection.is_empty() {
        return None;
    }
    document.text().get(selection.range()).map(str::to_owned)
}
