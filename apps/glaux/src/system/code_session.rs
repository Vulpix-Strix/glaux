//! The script documents open on the loaded cartridge and the tabs showing
//! them.
//!
//! Everything here names documents by [`DocumentId`], which only means
//! something to the store that issued it, so the whole session is dropped
//! together when another cartridge is loaded or the cartridge is unloaded.

mod tabs;

pub use tabs::{DocumentTabs, DocumentView};

use super::code_document::{CodeDocument, CodeDocumentError, CodeDocuments, DocumentId};
use crate::cartridge::LoadedCartridge;
use crate::cartridge::source::SourcePath;

/// The Code view's documents and tabs for one cartridge.
#[derive(Debug, Default)]
pub struct CodeSession {
    documents: CodeDocuments,
    tabs: DocumentTabs,
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

    /// Closes `id`'s tab and its document. Returns whether it was open.
    pub fn close(&mut self, id: DocumentId) -> bool {
        let closed = self.tabs.close(id);
        self.documents.close(id);
        closed
    }

    pub fn document(&self, id: DocumentId) -> Option<&CodeDocument> {
        self.documents.get(id)
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
