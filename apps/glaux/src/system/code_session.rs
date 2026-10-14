//! The script documents open on the loaded cartridge and the tabs showing
//! them.
//!
//! Everything here names documents by [`DocumentId`], which only means
//! something to the store that issued it, so the whole session is dropped
//! together when another cartridge is loaded or the cartridge is unloaded.

mod tabs;

pub use tabs::{DocumentTabs, DocumentView};

use std::collections::HashMap;
use std::ops::Range;

use super::code_cursor::Caret;
use super::code_document::{
    CodeDocument, CodeDocumentError, CodeDocuments, CodeSaveError, CodeSaved, DocumentId,
    EditError, Replaced,
};
use super::code_history::{EditHistory, EditKind};
use super::code_selection::Selection;
use crate::cartridge::LoadedCartridge;
use crate::cartridge::source::SourcePath;

/// The Code view's documents and tabs for one cartridge.
#[derive(Debug, Default)]
pub struct CodeSession {
    documents: CodeDocuments,
    tabs: DocumentTabs,
    /// Each open document's undo and redo steps, kept across tab switches
    /// and saves and dropped when it closes.
    histories: HashMap<DocumentId, EditHistory>,
    /// Whether the entry script was opened, or tried, on the first visit
    /// to the Code view. Never reset, so closing every tab stays closed.
    entry_requested: bool,
}

impl CodeSession {
    /// Opens the file at `path` of `cartridge` in a tab and activates it.
    /// A file already open only has its tab activated. On error nothing
    /// changes.
    pub fn open(
        &mut self,
        cartridge: &LoadedCartridge,
        path: &SourcePath,
    ) -> Result<DocumentId, CodeDocumentError> {
        let id = self.documents.open(cartridge, path)?;
        self.tabs.open(id);
        Ok(id)
    }

    /// Closes `id`'s tab and its document, discarding any unsaved edits
    /// and its history. Returns whether it was open.
    pub fn close(&mut self, id: DocumentId) -> bool {
        let closed = self.tabs.close(id);
        self.documents.close(id);
        self.histories.remove(&id);
        closed
    }

    pub fn document(&self, id: DocumentId) -> Option<&CodeDocument> {
        self.documents.get(id)
    }

    /// Replaces `range` of `id`'s text with `text`, see
    /// [`CodeDocument::replace`], and records it in `id`'s history as a
    /// `kind` edit made with its tab's current selection. `after`, given
    /// as anchor and caret offsets in the edited text, is the selection
    /// redoing it restores; without it, the caret after the inserted text.
    /// `None` if `id` is not open.
    pub fn edit(
        &mut self,
        id: DocumentId,
        range: Range<usize>,
        text: &str,
        kind: EditKind,
        after: Option<(usize, usize)>,
    ) -> Option<Result<Replaced, EditError>> {
        let document = self.documents.get_mut(id)?;
        let result = document.replace(range, text);
        if let Ok(replaced) = &result {
            let before = self.tabs.selection(id).unwrap_or_default();
            let after = after.map(|(anchor, caret)| {
                Selection::between(document, anchor, Caret::at(document, caret))
            });
            self.histories
                .entry(id)
                .or_default()
                .record(replaced, text, kind, before, after);
        }
        Some(result)
    }

    /// Whether `id` has an edit to undo.
    pub fn can_undo(&self, id: DocumentId) -> bool {
        self.histories.get(&id).is_some_and(EditHistory::can_undo)
    }

    /// Whether `id` has an undone edit to redo.
    pub fn can_redo(&self, id: DocumentId) -> bool {
        self.histories.get(&id).is_some_and(EditHistory::can_redo)
    }

    /// Makes `id`'s next edit start an undo step of its own.
    pub fn seal_history(&mut self, id: DocumentId) {
        if let Some(history) = self.histories.get_mut(&id) {
            history.seal();
        }
    }

    /// Reverses `id`'s newest undo step, returning the selection it had
    /// before it. `None` if `id` is not open or has nothing to undo.
    pub fn undo(&mut self, id: DocumentId) -> Option<Selection> {
        let document = self.documents.get_mut(id)?;
        let result = self.histories.get_mut(&id)?.undo(document)?;
        self.applied(id, result)
    }

    /// Makes `id`'s last undone step again, returning the selection it
    /// left. `None` if `id` is not open or has nothing to redo.
    pub fn redo(&mut self, id: DocumentId) -> Option<Selection> {
        let document = self.documents.get_mut(id)?;
        let result = self.histories.get_mut(&id)?.redo(document)?;
        self.applied(id, result)
    }

    /// The selection an undo or redo of `id` left, or, when the history
    /// no longer matched the text, nothing: the history is dropped rather
    /// than replayed against text it does not describe.
    fn applied(
        &mut self,
        id: DocumentId,
        result: Result<Selection, EditError>,
    ) -> Option<Selection> {
        match result {
            Ok(selection) => Some(selection),
            Err(err) => {
                tracing::warn!(error = %err, "undo history does not match the text, dropped");
                self.histories.remove(&id);
                None
            }
        }
    }

    /// Writes `id`'s text to its file; see [`CodeDocuments::save`]. The
    /// next edit starts a new undo step, so undo returns exactly to the
    /// saved text.
    pub fn save(
        &mut self,
        cartridge: &LoadedCartridge,
        id: DocumentId,
    ) -> Result<CodeSaved, CodeSaveError> {
        self.seal_history(id);
        self.documents.save(cartridge, id)
    }

    /// Writes `id`'s text over a file that changed outside Glaux, provided
    /// it still holds `seen`; see [`CodeDocuments::overwrite`].
    pub fn overwrite(
        &mut self,
        cartridge: &LoadedCartridge,
        id: DocumentId,
        seen: Option<&[u8]>,
    ) -> Result<CodeSaved, CodeSaveError> {
        self.seal_history(id);
        self.documents.overwrite(cartridge, id, seen)
    }

    /// Whether any open document differs from its file.
    pub fn is_modified(&self) -> bool {
        self.documents.is_modified()
    }

    /// The documents that differ from their files, in tab order.
    pub fn modified(&self) -> Vec<&CodeDocument> {
        self.tabs
            .ids()
            .filter_map(|id| self.documents.get(id))
            .filter(|document| document.is_modified())
            .collect()
    }

    /// The active tab's document.
    pub fn active_document(&self) -> Option<&CodeDocument> {
        self.tabs.active().and_then(|id| self.documents.get(id))
    }

    /// Whether `path` is open in a tab.
    pub fn is_open(&self, path: &SourcePath) -> bool {
        self.tabs
            .ids()
            .filter_map(|id| self.documents.get(id))
            .any(|document| document.path() == path)
    }

    pub fn tabs(&self) -> &DocumentTabs {
        &self.tabs
    }

    pub fn tabs_mut(&mut self) -> &mut DocumentTabs {
        &mut self.tabs
    }

    /// Marks the entry script as requested, returning whether it was not
    /// yet.
    pub fn take_entry_request(&mut self) -> bool {
        !std::mem::replace(&mut self.entry_requested, true)
    }
}

#[cfg(test)]
mod tests;
