//! Undo and redo for one code document.
//!
//! Each step is one replacement of the text, with the selection before and
//! after it. Consecutive keystrokes of the same kind grow the newest step
//! instead of adding one, so a typed word or a run of Backspaces undoes as
//! a whole.

use std::collections::VecDeque;

use super::code_cursor::Caret;
use super::code_document::{CodeDocument, EditError, Replaced};
use super::code_selection::Selection;

/// Steps the history keeps. Older ones are forgotten first.
const STEP_LIMIT: usize = 1024;

/// Bytes of text the undo and redo steps may hold together, removed and
/// inserted text both counting. Older steps are forgotten first, but the
/// newest undo step is always kept, however large.
const BYTE_LIMIT: usize = 1 << 20;

/// What kind of keystroke made an edit, which decides whether it joins the
/// step before it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditKind {
    /// A typed character, over the selection or at the caret.
    Typing,
    /// Backspace with nothing selected.
    Backspace,
    /// Delete with nothing selected.
    Delete,
    /// Anything that is a step of its own: a line break, indentation, or
    /// deleting a selection.
    Structural,
}

/// A selection's ends as offsets, rebuilt against the text it applies to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Ends {
    anchor: usize,
    caret: usize,
}

impl Ends {
    fn of(selection: Selection) -> Self {
        Self {
            anchor: selection.anchor(),
            caret: selection.caret().byte(),
        }
    }

    fn collapsed_at(byte: usize) -> Self {
        Self {
            anchor: byte,
            caret: byte,
        }
    }

    fn is_collapsed(self) -> bool {
        self.anchor == self.caret
    }

    fn selection(self, document: &CodeDocument) -> Selection {
        Selection::between(document, self.anchor, Caret::at(document, self.caret))
    }
}

/// One replacement: `removed` was at `at` and `inserted` took its place.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Step {
    at: usize,
    removed: String,
    inserted: String,
    kind: EditKind,
    before: Ends,
    after: Ends,
}

impl Step {
    fn bytes(&self) -> usize {
        self.removed.len() + self.inserted.len()
    }

    /// Grows this step by the edit `next` if it continues it: same kind,
    /// made where this one left the caret, and adjacent to it. Returns
    /// whether it did.
    fn absorb(&mut self, next: &Step) -> bool {
        if next.kind != self.kind || !next.before.is_collapsed() || next.before != self.after {
            return false;
        }
        let joined = match self.kind {
            EditKind::Typing => next.removed.is_empty() && next.at == self.at + self.inserted.len(),
            EditKind::Backspace => {
                next.inserted.is_empty() && next.at + next.removed.len() == self.at
            }
            EditKind::Delete => next.inserted.is_empty() && next.at == self.at,
            EditKind::Structural => false,
        };
        if !joined {
            return false;
        }
        match self.kind {
            EditKind::Typing => self.inserted.push_str(&next.inserted),
            EditKind::Backspace => {
                self.removed.insert_str(0, &next.removed);
                self.at = next.at;
            }
            EditKind::Delete => self.removed.push_str(&next.removed),
            EditKind::Structural => {}
        }
        self.after = next.after;
        true
    }
}

/// A document's undo and redo steps.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EditHistory {
    undo: VecDeque<Step>,
    /// The last one is the next redone.
    redo: Vec<Step>,
    /// Keeps the next edit from joining the newest step.
    sealed: bool,
}

impl EditHistory {
    /// Records the edit `replaced` describes, which inserted `inserted`
    /// while `before` was selected and left the caret after it, joining
    /// the newest step when it continues it. Steps that were undone can no
    /// longer be redone.
    pub fn record(
        &mut self,
        replaced: &Replaced,
        inserted: &str,
        kind: EditKind,
        before: Selection,
    ) {
        self.redo.clear();
        let step = Step {
            at: replaced.range.start,
            removed: replaced.removed.clone(),
            inserted: inserted.to_owned(),
            kind,
            before: Ends::of(before),
            after: Ends::collapsed_at(replaced.range.end),
        };
        let sealed = std::mem::replace(&mut self.sealed, false);
        let joined = !sealed && self.undo.back_mut().is_some_and(|last| last.absorb(&step));
        if !joined {
            self.undo.push_back(step);
        }
        self.trim();
    }

    /// Makes the next edit start a step of its own.
    pub fn seal(&mut self) {
        self.sealed = true;
    }

    /// Reverses the newest step in `document`, returning the selection it
    /// had before it, or `None` with nothing to undo.
    ///
    /// Steps are reversed in exactly the opposite order they were made, so
    /// `document` must hold the text the newest step left. A refused
    /// replacement means it does not; the step is then dropped and the
    /// error returned.
    pub fn undo(&mut self, document: &mut CodeDocument) -> Option<Result<Selection, EditError>> {
        let step = self.undo.pop_back()?;
        let range = step.at..step.at + step.inserted.len();
        let result = document
            .replace(range, &step.removed)
            .map(|_| step.before.selection(document));
        if result.is_ok() {
            self.redo.push(step);
        }
        self.sealed = true;
        Some(result)
    }

    /// Makes the last undone step again in `document`, returning the
    /// selection it left, or `None` with nothing to redo. As with
    /// [`undo`](Self::undo), a refused replacement drops the step.
    pub fn redo(&mut self, document: &mut CodeDocument) -> Option<Result<Selection, EditError>> {
        let step = self.redo.pop()?;
        let range = step.at..step.at + step.removed.len();
        let result = document
            .replace(range, &step.inserted)
            .map(|_| step.after.selection(document));
        if result.is_ok() {
            self.undo.push_back(step);
        }
        self.sealed = true;
        Some(result)
    }

    /// Forgets the oldest undo steps past [`STEP_LIMIT`] steps or
    /// [`BYTE_LIMIT`] bytes. Only called after recording, which leaves
    /// nothing to redo.
    fn trim(&mut self) {
        let mut stored: usize = self.undo.iter().map(Step::bytes).sum();
        while self.undo.len() > STEP_LIMIT || (stored > BYTE_LIMIT && self.undo.len() > 1) {
            let Some(oldest) = self.undo.pop_front() else {
                break;
            };
            stored -= oldest.bytes();
        }
    }
}

#[cfg(test)]
mod tests;
