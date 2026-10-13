//! The editing keys of the Code view.

use glaux_input::InputEvent;
use glaux_machine::font::BitmapFont;

use super::CodeEditor;
use crate::system::code_cursor::Caret;
use crate::system::code_document::{CodeDocument, EditError};
use crate::system::code_edit::{self, Edit};
use crate::system::code_selection::Selection;
use crate::system::code_session::CodeSession;
use crate::system::code_text::MetricsKey;

impl CodeEditor {
    /// Applies `event` to the active document if it is an editing key,
    /// returning whether it was one.
    pub(super) fn edit_input(&mut self, event: InputEvent, session: &mut CodeSession) -> bool {
        match event {
            InputEvent::Text(ch) => self.edit(session, |_, selection, _, _| {
                code_edit::typed(selection, ch)
            }),
            InputEvent::Enter => self.edit(session, |document, selection, _, _| {
                Some(code_edit::newline(document, selection))
            }),
            InputEvent::Backspace => self.edit(session, |document, selection, _, _| {
                code_edit::backspace(document, selection)
            }),
            InputEvent::Delete => self.edit(session, |document, selection, _, _| {
                code_edit::delete(document, selection)
            }),
            InputEvent::Tab => self.edit(session, |document, selection, key, font| {
                Some(code_edit::tab(document, selection, key, font))
            }),
            _ => return false,
        }
        true
    }

    /// Makes the edit `edit` works out for the active document and its
    /// selection, then leaves the caret after the inserted text with
    /// nothing selected, and scrolls as little as it takes to show it.
    /// Does nothing until the document's measurements describe the view.
    fn edit(
        &mut self,
        session: &mut CodeSession,
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
        let Some(Edit { range, text }) = edit(document, selection, key, &self.layout.font) else {
            return;
        };
        let replaced = match session.edit(id, range, &text) {
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
        // Ranges a press or drag recorded name text that may have moved.
        self.selecting = None;
        self.last_click = None;
        let Some(document) = session.document(id) else {
            return;
        };
        let selection = Selection::collapsed(Caret::at(document, replaced.range.end));
        session.tabs_mut().set_selection(id, selection);
        self.refresh_metrics(session);
        let view = self.revealing(session, id, selection.caret());
        self.show_selection(session, id, selection, view);
    }
}
