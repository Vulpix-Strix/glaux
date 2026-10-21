//! Find, replace and go to line in the Code view.

use std::ops::Range;

use glaux_i18n::Arg;
use glaux_input::{InputEvent, PointerButton};

use super::find::{FindAction, FindBar};
use super::go_to_line::{GoToAction, GoToLine};
use super::{CodeEditor, CodeRequest};
use crate::system::code_cursor::Caret;
use crate::system::code_document::DocumentId;
use crate::system::code_edit::Edit;
use crate::system::code_history::EditKind;
use crate::system::code_search::{next_from, previous_from};
use crate::system::code_selection::Selection;
use crate::system::code_session::{CodeSession, DocumentView};
use crate::ui_text;

/// Most bytes of a selection that becomes the query when the bar opens.
const MAX_SEEDED_QUERY: usize = 128;

/// Where to look for the match to show.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Seek {
    /// The first match from the selection's start, so typing a query
    /// keeps showing the match it grows from.
    Here,
    Next,
    Previous,
}

impl CodeEditor {
    /// Opens the find bar on the active document, or gives it the keys if
    /// it is open, with the replacement row too when `replacing`. A
    /// selection within one line becomes the query.
    pub(super) fn open_find(&mut self, session: &CodeSession, replacing: bool) {
        let Some(document) = session.active_document() else {
            return;
        };
        let seeded = session
            .tabs()
            .selection(document.id())
            .filter(|selection| !selection.is_empty())
            .and_then(|selection| document.text().get(selection.range()))
            .filter(|text| !text.contains(['\n', '\r']) && text.len() <= MAX_SEEDED_QUERY);
        match &mut self.find {
            Some(bar) => bar.reopen(seeded, replacing),
            None => self.find = Some(FindBar::new(seeded.unwrap_or(""), replacing)),
        }
        self.refresh_find(session);
    }

    /// Searches the active document again if it changed since the bar last
    /// searched it.
    pub(super) fn refresh_find(&mut self, session: &CodeSession) {
        if let (Some(bar), Some(document)) = (&mut self.find, session.active_document()) {
            bar.refresh(document);
        }
    }

    /// Handles `event` while the find bar is open. `None` passes it on to
    /// the document: inputs for the text while the text has the keys,
    /// pointer input outside the bar, and commands that act on the
    /// document whichever part has the keys.
    pub(super) fn find_input(
        &mut self,
        event: InputEvent,
        session: &mut CodeSession,
    ) -> Option<Option<CodeRequest>> {
        let layout = self.find_layout()?;
        let bar = self.find.as_mut()?;
        if let InputEvent::PointerDown { button, at, .. } = event {
            if layout.panel.contains(at) {
                if button != PointerButton::Primary {
                    return Some(None);
                }
                let action = bar.click(at, &layout);
                return Some(self.perform_find(action, session));
            }
            // A press in the text gives it the keys back.
            bar.unfocus();
            return None;
        }
        if event.is_pointer() {
            return None;
        }
        if bar.focus().is_none() {
            if event == InputEvent::Break {
                self.find = None;
                return Some(None);
            }
            return None;
        }
        match bar.handle_input(event) {
            FindAction::Unhandled if is_document_command(event) => None,
            FindAction::Unhandled => Some(None),
            action => Some(self.perform_find(action, session)),
        }
    }

    fn perform_find(
        &mut self,
        action: FindAction,
        session: &mut CodeSession,
    ) -> Option<CodeRequest> {
        match action {
            FindAction::None | FindAction::Unhandled => {}
            FindAction::Search => self.seek(session, Seek::Here),
            FindAction::Next => self.seek(session, Seek::Next),
            FindAction::Previous => self.seek(session, Seek::Previous),
            FindAction::ReplaceOne => self.replace_one(session),
            FindAction::ReplaceAll => return self.replace_all(session),
            FindAction::Copy(text) => return Some(CodeRequest::Copy(text)),
            FindAction::Paste => return Some(CodeRequest::Paste),
            FindAction::Close => self.find = None,
        }
        None
    }

    /// Pastes the clipboard's text into the find bar's focused field,
    /// showing the nearest match when the query changed.
    pub(super) fn paste_into_find(&mut self, session: &mut CodeSession, text: &str) {
        if self.find.as_mut().is_some_and(|bar| bar.paste(text)) {
            self.seek(session, Seek::Here);
        }
    }

    /// The active document's id, its matches and its selection, searched
    /// for its current text.
    fn matches(
        &mut self,
        session: &CodeSession,
    ) -> Option<(DocumentId, Vec<Range<usize>>, Selection)> {
        self.refresh_find(session);
        let document = session.active_document()?;
        let id = document.id();
        let matches = self.find.as_ref()?.matches_in(document).to_vec();
        let selection = session.tabs().selection(id)?;
        Some((id, matches, selection))
    }

    /// Selects the match `seek` finds from the selection and scrolls to it.
    fn seek(&mut self, session: &mut CodeSession, seek: Seek) {
        let Some((id, matches, selection)) = self.matches(session) else {
            return;
        };
        let range = selection.range();
        let current = matches.contains(&range);
        let index = match seek {
            Seek::Here => next_from(&matches, range.start),
            Seek::Next if current => next_from(&matches, range.end),
            Seek::Next => next_from(&matches, range.start),
            Seek::Previous => previous_from(&matches, range.start),
        };
        if let Some(found) = index.map(|index| matches[index].clone()) {
            self.select_range(session, id, found);
        }
    }

    /// Selects `range` of `id`, caret at its end, scrolled into view.
    fn select_range(&mut self, session: &mut CodeSession, id: DocumentId, range: Range<usize>) {
        let Some(document) = session.document(id) else {
            return;
        };
        let selection = Selection::between(document, range.start, Caret::at(document, range.end));
        let view = self.revealing(session, id, selection.caret());
        self.show_selection(session, id, selection, view);
    }

    /// Replaces the selected match with the replacement and shows the next
    /// one. With no match selected, it only shows the next one.
    fn replace_one(&mut self, session: &mut CodeSession) {
        let Some((id, matches, selection)) = self.matches(session) else {
            return;
        };
        let Some(replacement) = self.find.as_ref().map(FindBar::replacement) else {
            return;
        };
        let range = selection.range();
        if !matches.contains(&range) {
            self.seek(session, Seek::Next);
            return;
        }
        let edit = Edit {
            range,
            text: replacement,
        };
        if self.commit(session, id, edit, EditKind::Structural, None) {
            self.seek(session, Seek::Here);
        }
    }

    /// Replaces every match as one undo step, reporting how many.
    fn replace_all(&mut self, session: &mut CodeSession) -> Option<CodeRequest> {
        let (id, matches, _) = self.matches(session)?;
        let replacement = self.find.as_ref()?.replacement();
        let (first, last) = (matches.first()?.start, matches.last()?.end);
        let document = session.document(id)?;
        let text = document.text();
        let mut replaced = String::with_capacity(last - first);
        let mut copied = first;
        for found in &matches {
            replaced.push_str(&text[copied..found.start]);
            replaced.push_str(&replacement);
            copied = found.end;
        }
        let edit = Edit {
            range: first..last,
            text: replaced,
        };
        self.commit(session, id, edit, EditKind::Structural, None)
            .then(|| {
                CodeRequest::Notice(ui_text::get_with(
                    "code-replaced",
                    &[("value", Arg::Text(&(matches.len().to_string())))],
                ))
            })
    }

    /// Opens GO TO LINE on the active document.
    pub(super) fn open_go_to(&mut self, session: &CodeSession) {
        let Some(document) = session.active_document() else {
            return;
        };
        let caret = session
            .tabs()
            .selection(document.id())
            .unwrap_or_default()
            .caret()
            .byte();
        self.go_to = Some(GoToLine::new(
            document.line_count(),
            document.line_of(caret) + 1,
        ));
    }

    pub(super) fn go_to_input(
        &mut self,
        event: InputEvent,
        session: &mut CodeSession,
    ) -> Option<CodeRequest> {
        let layout = self.prompt_layout(self.go_to.as_ref()?.prompt());
        match self.go_to.as_mut()?.handle_input(event, &layout) {
            GoToAction::None => None,
            GoToAction::Go(line) => {
                self.go_to = None;
                self.go_to_line(session, line);
                None
            }
            GoToAction::Close => {
                self.go_to = None;
                None
            }
            GoToAction::Copy(text) => Some(CodeRequest::Copy(text)),
            GoToAction::Paste => Some(CodeRequest::Paste),
        }
    }

    /// Puts the caret at the start of `line` of the active document, the
    /// line shown a third of the way down the view.
    pub(super) fn go_to_line(&mut self, session: &mut CodeSession, line: usize) {
        let Some(document) = session.active_document() else {
            return;
        };
        let id = document.id();
        let Some(start) = document.line_range(line).map(|range| range.start) else {
            return;
        };
        let selection = Selection::collapsed(Caret::at(document, start));
        let geometry = self.geometry(document);
        let view = session.tabs().metrics(id).map(|metrics| {
            let top = DocumentView {
                top_line: line.saturating_sub(geometry.rows / 3),
                top_row: 0,
                left_column: 0,
            };
            geometry.clamp(top, metrics)
        });
        if let (Some(view), Some(stored)) = (view, session.tabs_mut().view_mut(id)) {
            *stored = view;
        }
        let view = self.revealing(session, id, selection.caret());
        self.show_selection(session, id, selection, view);
    }
}

/// Whether `event` acts on the document even while the find bar has the
/// keys.
fn is_document_command(event: InputEvent) -> bool {
    matches!(
        event,
        InputEvent::Save
            | InputEvent::OpenFile
            | InputEvent::NewFile
            | InputEvent::GoToLine
            | InputEvent::Find
            | InputEvent::Replace
            | InputEvent::CloseDocument
            | InputEvent::NextDocument
            | InputEvent::PreviousDocument
    )
}
