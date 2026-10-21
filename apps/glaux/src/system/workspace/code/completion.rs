//! Suggestions for the word at the caret, listed under it with the
//! documentation of the one picked.

mod layout;
mod panel;
mod render;

use glaux_docs::ScriptLanguage;
use glaux_input::{InputEvent, Point, PointerButton};
use glaux_sense::{Candidate, CandidateKind, Completions, Insertion, Trigger};
use glaux_syntax::SyntaxKind;

use super::CodeEditor;
use super::popup::Place;
use crate::cartridge::Language;
use crate::system::code_document::{CodeDocument, DocumentId};
use crate::system::code_edit::{Edit, IndentUnit};
use crate::system::code_history::EditKind;
use crate::system::code_session::CodeSession;
use crate::system::code_text::spot;
use crate::system::docs;
use crate::ui_text;
use layout::{CompletionLayout, VISIBLE_ROWS};
pub(super) use panel::{DocPanel, entry_signature, local_panel, type_name};
use panel::{PanelSource, panel_for};

/// The status bar hint while suggestions are listed.
pub fn completion_hint() -> String {
    ui_text::get("code-up-down-pick-tab-accept-esc-close")
}

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

    /// The label of each candidate, in list order.
    #[cfg(test)]
    pub fn labels(&self) -> Vec<&str> {
        self.completions
            .items
            .iter()
            .map(|candidate| candidate.label.as_str())
            .collect()
    }

    /// The candidate the list has picked.
    pub fn picked(&self) -> Option<&Candidate> {
        self.completions.items.get(self.selected)
    }

    /// The documentation shown beside the picked candidate.
    #[cfg(test)]
    pub fn picked_panel(&self) -> Option<&DocPanel> {
        self.panel()
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

/// Whether `ch` can be part of an identifier.
fn is_word(ch: char) -> bool {
    ch == '_' || ch.is_alphanumeric()
}

/// What typing or deleting asks of the list, given whether one was open
/// before the event. `None` leaves the list as the text left it.
fn trigger_for(event: InputEvent, was_open: bool) -> Option<Trigger> {
    match event {
        InputEvent::Complete => Some(Trigger::Manual),
        // Narrowing a list that is open answers from a single letter.
        InputEvent::Text(ch) if is_word(ch) => Some(if was_open {
            Trigger::Manual
        } else {
            Trigger::Auto
        }),
        InputEvent::Text('.' | ':') => Some(Trigger::Member),
        InputEvent::Backspace if was_open => Some(Trigger::Manual),
        _ => None,
    }
}

impl CodeEditor {
    /// Opens, narrows or closes the list after `event` was applied, which
    /// is the only place suggestions are asked for. `revision` is the
    /// active document's before the event: typing and deleting only count
    /// when they changed the text.
    ///
    /// Ctrl+Space always asks. Typing asks after a separator or, once two
    /// characters are in, a word, only when the caret is at the end of the
    /// word. Pasting, undoing, redoing and moving never open the list.
    pub(super) fn request_completion(
        &mut self,
        event: InputEvent,
        was_open: bool,
        revision: Option<u64>,
        session: &mut CodeSession,
    ) {
        let Some(trigger) = trigger_for(event, was_open) else {
            return;
        };
        let manual = event == InputEvent::Complete;
        if !manual && session.active_document().map(CodeDocument::revision) == revision {
            return;
        }
        self.completion = None;
        let Some((id, selection, _)) = self.editable(session) else {
            return;
        };
        if !selection.is_empty() {
            return;
        }
        let caret = selection.caret().byte();
        let Some(document) = session.document(id) else {
            return;
        };
        let text = document.text();
        // A caret inside a word would replace only half of it.
        if trigger == Trigger::Auto && text[caret..].chars().next().is_some_and(is_word) {
            return;
        }
        let deleting = event == InputEvent::Backspace;
        if deleting && !is_word_end(text, caret) && !ends_member(text, caret) {
            return;
        }
        let unit = IndentUnit::of(
            document,
            self.settings.indent,
            self.settings.indent_columns(),
        );
        let Some(completions) =
            session.completions(id, caret, trigger, &unit.text(), docs::library().api())
        else {
            return;
        };
        let Some(document) = session.document(id) else {
            return;
        };
        let language = match document.language() {
            Language::Luau => ScriptLanguage::Luau,
            Language::Rhai => ScriptLanguage::Rhai,
        };
        let summary = |id: &glaux_docs::DocId| {
            let page = docs::library().page(id, docs::chain())?.page;
            page.summary().map(str::to_owned)
        };
        let line_of = |byte: usize| document.line_of(byte) + 1;
        let source = PanelSource {
            language,
            api: docs::library().api(),
            summary: &summary,
            line_of: &line_of,
        };
        let panels = completions
            .items
            .iter()
            .map(|candidate| panel_for(candidate, &source))
            .collect();
        self.completion = CompletionPopup::new(id, completions, panels);
    }

    /// Lists `completions` for `document`, the word at its caret. Does
    /// nothing when there is nothing to list.
    #[cfg(test)]
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

/// Whether the text before `caret` ends in a word.
fn is_word_end(text: &str, caret: usize) -> bool {
    text[..caret].chars().next_back().is_some_and(is_word)
}

/// Whether the text before `caret` ends in a member separator.
fn ends_member(text: &str, caret: usize) -> bool {
    let before = &text[..caret];
    before.ends_with('.') || before.ends_with("::")
}

#[cfg(test)]
mod tests;
