//! Script source files of the loaded cartridge, open as text documents.
//!
//! Documents hold file data only. Cursor, selection, scroll position and
//! tab order are state of the views showing them.

use crate::cartridge::source::SourcePath;
use crate::cartridge::{Language, LoadedCartridge};
use crate::cartridge_fs::FsError;

/// Identifies one open document within its [`CodeDocuments`].
///
/// Ids are never reused by the store that issued them, so a file closed and
/// opened again gets a new one. They mean nothing to any other store: a new
/// store issues the same ids again, so references to documents must be
/// dropped together with the store that holds them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DocumentId(u64);

#[cfg(test)]
impl DocumentId {
    /// An id for tests of code that only stores and compares ids.
    pub(crate) fn from_raw(raw: u64) -> Self {
        Self(raw)
    }
}

/// What a document's file held when it was read.
///
/// It is the baseline unsaved changes and changes on disk are measured
/// against, so it changes only once a save or a reload has completed, never
/// while the text is edited or when a save fails.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Origin {
    text: String,
}

impl Origin {
    /// The file's exact bytes.
    #[cfg_attr(not(test), expect(dead_code, reason = "no caller outside tests"))]
    pub fn bytes(&self) -> &[u8] {
        self.text.as_bytes()
    }
}

/// One script file of the cartridge as text.
///
/// The text is the file's contents unaltered: line endings, a byte order
/// mark and a missing final newline are all kept, so writing it back
/// reproduces the file byte for byte.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeDocument {
    id: DocumentId,
    path: SourcePath,
    text: String,
    origin: Origin,
}

impl CodeDocument {
    pub fn id(&self) -> DocumentId {
        self.id
    }

    pub fn path(&self) -> &SourcePath {
        &self.path
    }

    /// The language the file's extension names.
    pub fn language(&self) -> Language {
        self.path.language()
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    #[cfg_attr(not(test), expect(dead_code, reason = "no caller outside tests"))]
    pub fn origin(&self) -> &Origin {
        &self.origin
    }
}

/// A script file that could not be opened. Messages name the path from the
/// cartridge root, so they are safe to show in the shell.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CodeDocumentError {
    #[error("No such file: {0}")]
    NotFound(SourcePath),
    #[error("Not a file: {0}")]
    NotAFile(SourcePath),
    #[error("Cannot read {0}")]
    Unreadable(SourcePath),
    /// `line` is the 1-based line holding the first invalid byte.
    #[error("{path} line {line}: not valid UTF-8 text")]
    InvalidUtf8 { path: SourcePath, line: usize },
}

/// The documents open on one cartridge, at most one per file.
///
/// Documents and their ids belong to the cartridge they were read from:
/// when another cartridge is loaded, the store is replaced and every id it
/// issued is discarded with it.
#[derive(Debug, Default)]
pub struct CodeDocuments {
    next_id: u64,
    documents: Vec<CodeDocument>,
}

impl CodeDocuments {
    /// Opens the file at `path` of `cartridge` and returns its document's
    /// id.
    ///
    /// A file already open is not read again: its existing document's id
    /// is returned. On error nothing is opened.
    pub fn open(
        &mut self,
        cartridge: &LoadedCartridge,
        path: &SourcePath,
    ) -> Result<DocumentId, CodeDocumentError> {
        if let Some(open) = self.documents.iter().find(|doc| doc.path == *path) {
            return Ok(open.id);
        }
        let bytes = cartridge.read_source(path).map_err(|err| match err {
            FsError::NotFound(_) | FsError::NotADirectory(_) => {
                CodeDocumentError::NotFound(path.clone())
            }
            FsError::NotAFile(_) => CodeDocumentError::NotAFile(path.clone()),
            FsError::Unreadable(_)
            | FsError::AlreadyExists(_)
            | FsError::Unwritable(_)
            | FsError::ReadOnly(_)
            | FsError::Unavailable => CodeDocumentError::Unreadable(path.clone()),
        })?;
        let text = String::from_utf8(bytes).map_err(|err| {
            let valid = &err.as_bytes()[..err.utf8_error().valid_up_to()];
            CodeDocumentError::InvalidUtf8 {
                path: path.clone(),
                line: valid.iter().filter(|&&byte| byte == b'\n').count() + 1,
            }
        })?;

        let id = DocumentId(self.next_id);
        self.next_id += 1;
        tracing::info!(file = %path, "code document opened");
        self.documents.push(CodeDocument {
            id,
            path: path.clone(),
            origin: Origin { text: text.clone() },
            text,
        });
        Ok(id)
    }

    pub fn get(&self, id: DocumentId) -> Option<&CodeDocument> {
        self.documents.iter().find(|doc| doc.id == id)
    }

    /// Closes the document `id`, returning it, or `None` if it is not open.
    pub fn close(&mut self, id: DocumentId) -> Option<CodeDocument> {
        let index = self.documents.iter().position(|doc| doc.id == id)?;
        Some(self.documents.remove(index))
    }
}

#[cfg(test)]
mod tests;
