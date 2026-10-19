//! Suggestions for the word at the caret, listed under it with the
//! documentation of the one picked.

mod layout;
mod panel;
mod render;

use glaux_input::{InputEvent, Point, PointerButton};
use glaux_sense::{CandidateKind, Completions, Insertion};
use glaux_syntax::SyntaxKind;

use super::CodeEditor;
use crate::system::code_document::{CodeDocument, DocumentId};
use crate::system::code_edit::Edit;
use crate::system::code_history::EditKind;
use crate::system::code_session::CodeSession;
use crate::system::code_text::spot;
use layout::{CompletionLayout, Place, VISIBLE_ROWS};
pub(super) use panel::DocPanel;

/// The status bar hint while suggestions are listed.
pub const COMPLETION_HINT: &str = "Up/Down: pick  Tab: accept  Esc: close";

/// What the popup did with an input event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompletionAction {
    /// The event was used and the list stays.
    Stay,
    /// The event is not for the list. It goes on to the text, and the list
    /// stays until the text shows it no longer fits.
    Pass,
    /// The event was used and the list closes.
    Close,
    /// The list closes and the event goes on to the text.
    Dismiss,
    /// The picked suggestion is to be inserted. The list closes.
    Accept,
}

/// The edit that inserting a suggestion makes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Accepted {
    pub edit: Edit,
    /// Where the caret ends up in the edited text, when not right after
    /// the inserted text.
    pub caret: Option<usize>,
}

/// The suggestions listed for one document, which one is picked, and the
/// scrolling of the list.
///
/// A popup describes the text as it was when the suggestions were worked
/// out. See [`is_current`](Self::is_current).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompletionPopup {
    document: DocumentId,
    completions: Completions,
    /// The panel of each candidate, in the same order.
    panels: Vec<Option<DocPanel>>,
    selected: usize,
    /// The first candidate the list shows.
    first: usize,
    /// Whether the user moved the pick, which is what lets Enter accept it.
    navigated: bool,
}

impl CompletionPopup {
    /// A popup listing `completions` for `document`, with the first
    /// candidate picked, or `None` when there is nothing to list. `panels`
    /// holds each candidate's documentation in the same order.
    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "nothing asks the session for suggestions yet")
    )]
    pub fn new(
        document: DocumentId,
        completions: Completions,
        panels: Vec<Option<DocPanel>>,
    ) -> Option<Self> {
        if completions.items.is_empty() {
            return None;
        }
        Some(Self {
            document,
            completions,
            panels,
            selected: 0,
            first: 0,
            navigated: false,
        })
    }

    /// The range of text an accepted suggestion replaces.
    pub fn anchor(&self) -> std::ops::Range<usize> {
        self.completions.anchor.clone()
    }

    #[cfg(test)]
    pub fn selected(&self) -> usize {
        self.selected
    }

    #[cfg(test)]
    pub fn first(&self) -> usize {
        self.first
    }

    /// Whether the user moved the pick since the list opened.
    #[cfg(test)]
    pub fn navigated(&self) -> bool {
        self.navigated
    }

    /// Whether the suggestions still fit `document` with its caret at
    /// `caret`: the same text, with the caret where the word being
    /// completed ends.
    pub fn is_current(&self, document: &CodeDocument, caret: usize) -> bool {
        document.id() == self.document
            && document.revision() == self.completions.revision
            && caret == self.completions.anchor.end
    }

    fn panel(&self) -> Option<&DocPanel> {
        self.panels.get(self.selected)?.as_ref()
    }

    fn layout(&self, place: Place<'_>, anchor: (i32, i32)) -> CompletionLayout {
        CompletionLayout::new(
            place,
            &self.completions.items,
            self.first,
            self.selected,
            self.panel(),
            anchor,
        )
    }

    /// Moves the pick `by` candidates, stopping at the ends, and scrolls
    /// the list as little as it takes to show it.
    fn step(&mut self, by: isize) {
        let last = self.completions.items.len() - 1;
        self.selected = self.selected.saturating_add_signed(by).min(last);
        self.navigated = true;
        if self.selected < self.first {
            self.first = self.selected;
        } else if self.selected >= self.first + VISIBLE_ROWS {
            self.first = self.selected + 1 - VISIBLE_ROWS;
        }
    }

    /// Scrolls the list `rows` down, or up when negative, moving the pick
    /// only as far as it takes to keep it in view.
    fn scroll(&mut self, rows: isize) {
        let shown = self.completions.items.len().min(VISIBLE_ROWS);
        let furthest = self.completions.items.len() - shown;
        self.first = self.first.saturating_add_signed(rows).min(furthest);
        self.selected = self.selected.clamp(self.first, self.first + shown - 1);
    }

    /// Applies `event`. `layout` is where the list is drawn, `None` while
    /// it is not, and `pointer` is where the pointer last was.
    pub fn handle_input(
        &mut self,
        event: InputEvent,
        layout: Option<&CompletionLayout>,
        pointer: Option<Point>,
    ) -> CompletionAction {
        match event {
            InputEvent::Break => CompletionAction::Close,
            InputEvent::Up { select: false } => self.stepped(-1),
            InputEvent::Down { select: false } => self.stepped(1),
            InputEvent::PageUp { select: false } => self.stepped(-(VISIBLE_ROWS as isize)),
            InputEvent::PageDown { select: false } => self.stepped(VISIBLE_ROWS as isize),
            InputEvent::Tab { reverse: false } => CompletionAction::Accept,
            // Enter only takes what the user chose. Otherwise it is the
            // line break it always is, so typing is never interrupted.
            InputEvent::Enter if self.navigated => CompletionAction::Accept,
            InputEvent::Text(_)
            | InputEvent::Backspace
            | InputEvent::PointerMove(_)
            | InputEvent::PointerUp { .. }
            | InputEvent::PointerCancel => CompletionAction::Pass,
            InputEvent::Wheel { delta } => {
                let over = pointer
                    .zip(layout)
                    .is_some_and(|(at, layout)| layout.list_contains(at));
                if over {
                    self.scroll(-isize::from(delta));
                    CompletionAction::Stay
                } else {
                    CompletionAction::Dismiss
                }
            }
            InputEvent::PointerDown {
                button: PointerButton::Primary,
                at,
                ..
            } => match layout.and_then(|layout| layout.row_at(at)) {
                Some(index) => {
                    self.selected = index;
                    CompletionAction::Accept
                }
                None => CompletionAction::Dismiss,
            },
            _ => CompletionAction::Dismiss,
        }
    }

    fn stepped(&mut self, by: isize) -> CompletionAction {
        self.step(by);
        CompletionAction::Stay
    }

    /// The edit that inserts the picked suggestion.
    pub fn accepted(&self) -> Option<Accepted> {
        let anchor = self.completions.anchor.clone();
        let candidate = self.completions.items.get(self.selected)?;
        let (text, caret) = match &candidate.insertion {
            Insertion::Name(name) => (name.clone(), None),
            Insertion::Declaration {
                text,
                caret_in_text,
            } => (text.clone(), Some(anchor.start + caret_in_text)),
        };
        Some(Accepted {
            edit: Edit {
                range: anchor,
                text,
            },
            caret,
        })
    }
}

/// The syntax category whose color marks `kind` in the list, and the
/// letter that does as well for those who can't tell the colors apart.
fn marker(kind: CandidateKind) -> (char, SyntaxKind) {
    match kind {
        CandidateKind::Function | CandidateKind::Callback => ('f', SyntaxKind::Function),
        CandidateKind::Variable => ('v', SyntaxKind::Variable),
        CandidateKind::Namespace => ('m', SyntaxKind::Type),
        CandidateKind::Keyword => ('k', SyntaxKind::Keyword),
        CandidateKind::Constant => ('c', SyntaxKind::Constant),
    }
}

impl CodeEditor {
    /// Lists `completions` for `document`, the word at its caret. Does
    /// nothing when there is nothing to list.
    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "nothing asks the session for suggestions yet")
    )]
    pub fn show_completions(
        &mut self,
        document: DocumentId,
        completions: Completions,
        panels: Vec<Option<DocPanel>>,
    ) {
        self.completion = CompletionPopup::new(document, completions, panels);
    }

    /// Where the list and its panel are drawn, or `None` when the popup
    /// is not showing: it is closed, it no longer fits the text, or its
    /// word is scrolled out of sight.
    pub(super) fn completion_layout(&self, session: &CodeSession) -> Option<CompletionLayout> {
        let popup = self.completion.as_ref()?;
        let document = session.active_document()?;
        let id = document.id();
        let selection = session.tabs().selection(id)?;
        if !selection.is_empty() || !popup.is_current(document, selection.caret().byte()) {
            return None;
        }
        let geometry = self.geometry(document);
        let tabs = session.tabs();
        let (view, metrics) = (tabs.view(id)?, tabs.metrics(id)?);
        if !metrics.describes(document, geometry.metrics_key()) {
            return None;
        }
        let word = spot(
            document,
            popup.anchor().start,
            metrics.key(),
            &self.layout.font,
        );
        let anchor = geometry.caret_point(*view, &word, metrics)?;
        let place = Place {
            font: &self.layout.font,
            area: self.text_area(),
            cell_w: geometry.cell_w,
            cell_h: geometry.cell_h,
        };
        Some(popup.layout(place, anchor))
    }

    /// Closes the popup if the text or the caret moved on from the word
    /// it was listed for.
    pub(super) fn sync_completion(&mut self, session: &CodeSession) {
        let Some(popup) = &self.completion else {
            return;
        };
        let current = session.active_document().is_some_and(|document| {
            session
                .tabs()
                .selection(document.id())
                .is_some_and(|selection| {
                    selection.is_empty() && popup.is_current(document, selection.caret().byte())
                })
        });
        if !current {
            self.completion = None;
        }
    }

    /// Handles `event` while suggestions are listed. `None` passes it on
    /// to the text, which is how typing goes on while the list is open.
    pub(super) fn completion_input(
        &mut self,
        event: InputEvent,
        session: &mut CodeSession,
    ) -> Option<Option<super::CodeRequest>> {
        self.sync_completion(session);
        self.completion.as_ref()?;
        let layout = self.completion_layout(session);
        let popup = self.completion.as_mut()?;
        match popup.handle_input(event, layout.as_ref(), self.pointer) {
            CompletionAction::Stay => Some(None),
            CompletionAction::Pass => None,
            CompletionAction::Close => {
                self.completion = None;
                Some(None)
            }
            CompletionAction::Dismiss => {
                self.completion = None;
                None
            }
            CompletionAction::Accept => {
                let popup = self.completion.take()?;
                self.insert_completion(session, &popup);
                Some(None)
            }
        }
    }

    /// Inserts the suggestion `popup` has picked in place of the word it
    /// was listed for, as one edit.
    fn insert_completion(&mut self, session: &mut CodeSession, popup: &CompletionPopup) {
        let Some(accepted) = popup.accepted() else {
            return;
        };
        let current = session.active_document().is_some_and(|document| {
            session
                .tabs()
                .selection(document.id())
                .is_some_and(|selection| popup.is_current(document, selection.caret().byte()))
        });
        if !current {
            return;
        }
        let Accepted { edit, caret } = accepted;
        self.commit(
            session,
            popup.document,
            edit,
            EditKind::Structural,
            caret.map(|caret| (caret, caret)),
        );
    }
}

#[cfg(test)]
mod tests;
