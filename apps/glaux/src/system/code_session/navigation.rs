//! The places the user jumped between, kept apart from undo and redo.
//!
//! A jump is a deliberate move to another spot: a definition, a symbol, a line of the
//! log. Typing and moving the caret never record one. Back and forward swap places
//! between two stacks and never add entries of their own.
//!
//! A place names its file by path, so a tab closed since can be reopened. It also keeps
//! the text of its line, because offsets drift once a document is edited. Arriving at a
//! place that no longer matches is reported rather than hidden.

use super::CodeSession;
use crate::cartridge::LoadedCartridge;
use crate::cartridge::source::SourcePath;
use crate::system::code_cursor::Caret;
use crate::system::code_document::{CodeDocument, DocumentId};
use crate::system::code_selection::Selection;

/// The most places each direction remembers.
const CAPACITY: usize = 100;

/// How much of a line is kept to recognise it later.
const LINE_KEY_CHARS: usize = 120;

/// Where a jump leads, as resolved by whoever knows the cartridge.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Destination {
    pub path: SourcePath,
    pub at: Spot,
}

/// A position inside a target document.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Spot {
    /// A byte offset in the text as it is now.
    Offset(usize),
    /// The start of a line, counted from 0.
    Line(usize),
}

/// How faithfully a place was restored.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Landing {
    /// The place is where it was: the text did not change, or its line did not.
    Exact,
    /// The document changed under the place. The caret is at the start of the same line
    /// number, which now holds other text.
    LineChanged,
}

/// What [`CodeSession::go_back`] and [`CodeSession::go_forward`] did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Arrival {
    pub id: DocumentId,
    pub landing: Landing,
    /// The first line to show, when the document is exactly as the place left it.
    /// Otherwise the caller scrolls to the caret.
    pub top_line: Option<usize>,
    /// Places passed over because their file is gone or their line no longer exists.
    pub skipped: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Place {
    path: SourcePath,
    document: DocumentId,
    revision: u64,
    offset: usize,
    line: usize,
    /// The line's content when the place was taken, see [`line_key`].
    line_key: String,
    /// Bytes from the start of the line to the caret.
    column: usize,
    top_line: usize,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct History {
    back: Vec<Place>,
    forward: Vec<Place>,
}

impl History {
    fn push_back(&mut self, place: Place) {
        push_capped(&mut self.back, place);
    }

    /// Follows `from` to `to` after a rename so places keep finding their file.
    pub(super) fn renamed(&mut self, from: &SourcePath, to: &SourcePath) {
        for place in self.back.iter_mut().chain(&mut self.forward) {
            if place.path == *from {
                place.path = to.clone();
            }
        }
    }
}

fn push_capped(stack: &mut Vec<Place>, place: Place) {
    if stack.last() == Some(&place) {
        return;
    }
    if stack.len() == CAPACITY {
        stack.remove(0);
    }
    stack.push(place);
}

impl CodeSession {
    /// Whether [`go_back`](Self::go_back) has a place to return to.
    pub fn can_go_back(&self) -> bool {
        !self.navigation.back.is_empty()
    }

    /// Whether [`go_forward`](Self::go_forward) has a place to go to.
    pub fn can_go_forward(&self) -> bool {
        !self.navigation.forward.is_empty()
    }

    /// Moves to `target`, opening its file if needed, and remembers where the jump
    /// started so [`go_back`](Self::go_back) can return there. A jump to the spot the
    /// caret is already at records nothing, and a new jump forgets what was ahead.
    ///
    /// Only the destination tab's selection changes, and the editing history is not
    /// touched. The caller shows the new caret. On error nothing changes.
    pub fn jump(
        &mut self,
        cartridge: &LoadedCartridge,
        target: &Destination,
    ) -> Result<DocumentId, super::CodeDocumentError> {
        let departure = self.here();
        let id = self.open(cartridge, &target.path)?;
        let Some(document) = self.documents.get(id) else {
            return Ok(id);
        };
        let offset = match target.at {
            Spot::Offset(offset) => offset,
            Spot::Line(line) => document
                .line_range(line)
                .map_or(document.text().len(), |r| r.start),
        };
        let caret = Caret::at(document, offset);
        let selection = Selection::collapsed(caret);
        self.tabs.set_selection(id, selection);
        if let Some(departure) = departure
            && !(departure.document == id && departure.offset == caret.byte())
        {
            self.navigation.push_back(departure);
            self.navigation.forward.clear();
        }
        Ok(id)
    }

    /// Returns to the previous place, which becomes the one [`go_forward`](Self::go_forward)
    /// leads to. Places whose file is gone are passed over. `None` when there is nowhere
    /// to go back to, leaving everything as it was.
    pub fn go_back(&mut self, cartridge: &LoadedCartridge) -> Option<Arrival> {
        self.travel(cartridge, true)
    }

    /// The reverse of [`go_back`](Self::go_back).
    pub fn go_forward(&mut self, cartridge: &LoadedCartridge) -> Option<Arrival> {
        self.travel(cartridge, false)
    }

    fn travel(&mut self, cartridge: &LoadedCartridge, backwards: bool) -> Option<Arrival> {
        let current = self.here();
        let mut skipped = 0;
        loop {
            let stack = if backwards {
                &mut self.navigation.back
            } else {
                &mut self.navigation.forward
            };
            let place = stack.pop()?;
            let Some((id, landing)) = self.arrive(cartridge, &place) else {
                skipped += 1;
                continue;
            };
            if let Some(current) = current {
                let other = if backwards {
                    &mut self.navigation.forward
                } else {
                    &mut self.navigation.back
                };
                push_capped(other, current);
            }
            let exact = self
                .documents
                .get(id)
                .is_some_and(|document| same_text(&place, document));
            return Some(Arrival {
                id,
                landing,
                top_line: exact.then_some(place.top_line),
                skipped,
            });
        }
    }

    /// Opens `place`'s file and puts its caret back, or leaves the session as it was.
    fn arrive(
        &mut self,
        cartridge: &LoadedCartridge,
        place: &Place,
    ) -> Option<(DocumentId, Landing)> {
        let previous = self.tabs.active();
        let was_open = self.is_open(&place.path);
        let id = self.open(cartridge, &place.path).ok()?;
        let document = self.documents.get(id)?;
        let restored = restore(place, document);
        match restored {
            Some((offset, landing)) => {
                let selection = Selection::collapsed(Caret::at(document, offset));
                self.tabs.set_selection(id, selection);
                Some((id, landing))
            }
            None => {
                if !was_open {
                    self.close(id);
                }
                if let Some(previous) = previous {
                    self.tabs.activate(previous);
                }
                None
            }
        }
    }

    /// The active tab's caret as a place to come back to.
    fn here(&self) -> Option<Place> {
        let document = self.active_document()?;
        let id = document.id();
        let offset = self.tabs.selection(id)?.caret().byte();
        let line = document.line_of(offset);
        let start = document.line_range(line).map_or(0, |range| range.start);
        Some(Place {
            path: document.path().clone(),
            document: id,
            revision: document.revision(),
            offset,
            line,
            line_key: line_key(document, line),
            column: offset.saturating_sub(start),
            top_line: self.tabs.view(id).map_or(0, |view| view.top_line),
        })
    }
}

fn same_text(place: &Place, document: &CodeDocument) -> bool {
    place.document == document.id() && place.revision == document.revision()
}

/// Where `place` is in `document` now, or `None` when its line no longer exists.
fn restore(place: &Place, document: &CodeDocument) -> Option<(usize, Landing)> {
    if same_text(place, document) {
        return Some((place.offset, Landing::Exact));
    }
    let line = document.line_range(place.line)?;
    if line_key(document, place.line) == place.line_key {
        let length = line.len();
        return Some((line.start + place.column.min(length), Landing::Exact));
    }
    Some((line.start, Landing::LineChanged))
}

/// The start of a line's content, enough to tell whether it is the same line later.
fn line_key(document: &CodeDocument, line: usize) -> String {
    document
        .line_range(line)
        .and_then(|range| document.text().get(range))
        .map(|text| text.trim().chars().take(LINE_KEY_CHARS).collect())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests;
