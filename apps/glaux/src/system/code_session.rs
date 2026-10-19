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

use glaux_docs::{ApiCatalog, ScriptLanguage};
use glaux_sense::{Completions, Request, SignatureHelp, SignatureRequest, Trigger};

use super::code_cursor::Caret;
use super::code_document::{
    CodeDocument, CodeDocumentError, CodeDocuments, CodeRenameError, CodeSaveError, CodeSaved,
    DiskState, DocumentId, EditError, Replaced,
};
use super::code_history::{EditHistory, EditKind};
use super::code_selection::Selection;
use super::code_syntax::DocumentSyntax;
use crate::cartridge::source::{SourceEditError, SourcePath};
use crate::cartridge::{Language, LoadedCartridge};
use crate::cartridge_fs::FsError;

/// The Code view's documents and tabs for one cartridge.
#[derive(Debug, Default)]
pub struct CodeSession {
    documents: CodeDocuments,
    tabs: DocumentTabs,
    /// Each open document's undo and redo steps, kept across tab switches
    /// and saves and dropped when it closes.
    histories: HashMap<DocumentId, EditHistory>,
    /// Each open document's highlighting, dropped when it closes.
    syntax: HashMap<DocumentId, DocumentSyntax>,
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
        self.syntax.remove(&id);
        closed
    }

    pub fn document(&self, id: DocumentId) -> Option<&CodeDocument> {
        self.documents.get(id)
    }

    /// Brings `id`'s highlighting up to date for `lines`, the lines shown,
    /// parsing again only if the text changed. Does nothing if `id` is not
    /// open.
    pub fn refresh_syntax(&mut self, id: DocumentId, lines: Range<usize>) {
        let Some(document) = self.documents.get(id) else {
            return;
        };
        let last = document.line_count().saturating_sub(1);
        let start = document
            .line_range(lines.start.min(last))
            .map_or(0, |range| range.start);
        let end = document
            .line_range(lines.end.saturating_sub(1).min(last))
            .map_or(0, |range| range.end);
        self.syntax
            .entry(id)
            .or_insert_with(|| DocumentSyntax::new(document.language()))
            .refresh(document, start..end.max(start));
    }

    /// The highlight spans of the lines last given to
    /// [`refresh_syntax`](Self::refresh_syntax), sorted and apart.
    pub fn syntax_spans(&self, id: DocumentId) -> &[glaux_syntax::Span] {
        self.syntax.get(&id).map_or(&[], DocumentSyntax::spans)
    }

    /// What can be written at `caret` in `id`, answered from the text as it
    /// is now, unsaved edits included. The answer carries the document's
    /// revision so it can be told apart from a later text. `indent` is one
    /// step of indentation for inserted declarations. `None` if `id` is not
    /// open or its syntax is unavailable.
    pub fn completions(
        &mut self,
        id: DocumentId,
        caret: usize,
        trigger: Trigger,
        indent: &str,
        api: &ApiCatalog,
    ) -> Option<Completions> {
        let document = self.documents.get(id)?;
        let highlighter = self
            .syntax
            .entry(id)
            .or_insert_with(|| DocumentSyntax::new(document.language()))
            .parsed(document)?;
        let request = Request {
            language: script_language(document.language()),
            text: document.text(),
            caret,
            revision: document.revision(),
            trigger,
            line_ending: document.line_ending().as_str(),
            indent,
        };
        Some(glaux_sense::complete(&request, api, highlighter))
    }

    /// The call `id`'s caret at `caret` is typing arguments for, answered
    /// from the text as it is now, unsaved edits included. The answer
    /// carries the document's revision so it can be told apart from a later
    /// text. `None` if `id` is not open, its syntax is unavailable or the
    /// caret is not in a call to a function the engine knows.
    pub fn signature_help(
        &mut self,
        id: DocumentId,
        caret: usize,
        api: &ApiCatalog,
    ) -> Option<SignatureHelp> {
        let document = self.documents.get(id)?;
        let highlighter = self
            .syntax
            .entry(id)
            .or_insert_with(|| DocumentSyntax::new(document.language()))
            .parsed(document)?;
        let request = SignatureRequest {
            language: script_language(document.language()),
            text: document.text(),
            caret,
            revision: document.revision(),
        };
        glaux_sense::signature_help(&request, api, highlighter)
    }

    /// Replaces `range` of `id`'s text with `text`, see
    /// [`CodeDocument::replace`], and records it in `id`'s history as a
    /// `kind` edit made with its tab's current selection. `after`, given
    /// as anchor and caret offsets in the edited text, is the selection
    /// redoing it restores. Without it, the caret after the inserted text.
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
    /// no longer matched the text, nothing. The history is dropped rather
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

    /// Writes `id`'s text to its file. See [`CodeDocuments::save`]. The
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
    /// it still holds `seen`. See [`CodeDocuments::overwrite`].
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

    /// Moves `id`'s file to `to` and points the document there.
    ///
    /// Everything that can be refused is checked before the file moves:
    /// another document holding `to`, a file that changed on disk outside
    /// Glaux or is gone, the entry script. Then the file moves, and then
    /// the document follows. Should the document refuse after all, the file
    /// is moved back, so the disk and the tab never name different paths.
    /// The file moves as it is, so unsaved edits stay in the document and
    /// are written to `to` by the next save. Id, tab, selection and
    /// history are kept. Moving to the path it already has does nothing.
    pub fn move_script(
        &mut self,
        cartridge: &LoadedCartridge,
        id: DocumentId,
        to: &SourcePath,
    ) -> Result<(), MoveScriptError> {
        let from = self
            .documents
            .get(id)
            .ok_or(CodeRenameError::NotOpen)?
            .path()
            .clone();
        if from == *to {
            return Ok(());
        }
        self.documents.can_rename(id, to)?;
        match self.documents.disk_state(cartridge, id) {
            Some(Ok(DiskState::InSync(_))) | None => {}
            Some(Ok(DiskState::Modified(_))) => return Err(MoveScriptError::Changed(from)),
            Some(Ok(DiskState::Missing)) => return Err(MoveScriptError::Missing(from)),
            Some(Err(err)) => return Err(SourceEditError::Fs(err).into()),
        }
        cartridge.rename_source(&from, to)?;
        if let Err(err) = self.documents.rename(id, to) {
            // Checked above, so this is not expected. Keep disk and tab
            // together anyway.
            tracing::warn!(error = %err, "document refused a move after its file moved, moving the file back");
            if let Err(undo) = cartridge.rename_source(to, &from) {
                tracing::warn!(error = %undo, "file could not be moved back");
            }
            return Err(err.into());
        }
        Ok(())
    }

    /// How `id`'s file compares with its baseline. See
    /// [`CodeDocuments::disk_state`].
    pub fn disk_state(
        &self,
        cartridge: &LoadedCartridge,
        id: DocumentId,
    ) -> Option<Result<DiskState, FsError>> {
        self.documents.disk_state(cartridge, id)
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

/// Why [`CodeSession::move_script`] left the script where it was.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum MoveScriptError {
    #[error(transparent)]
    Document(#[from] CodeRenameError),
    #[error("{0} changed on disk outside Glaux. Close and reopen it before renaming.")]
    Changed(SourcePath),
    #[error("{0} no longer exists on disk.")]
    Missing(SourcePath),
    #[error(transparent)]
    File(#[from] SourceEditError),
}

fn script_language(language: Language) -> ScriptLanguage {
    match language {
        Language::Luau => ScriptLanguage::Luau,
        Language::Rhai => ScriptLanguage::Rhai,
    }
}

#[cfg(test)]
mod tests;
