//! Reading script files into documents and writing them back.
//!
//! A [`CodeDocuments`] store holds the documents open on one cartridge and
//! knows how each differs from its file on disk, so it can refuse to
//! overwrite a file that changed outside Glaux.

use super::{CodeDocument, DocumentId};
use crate::cartridge::LoadedCartridge;
use crate::cartridge::source::{SourcePath, SourceWriteError};
use crate::cartridge_fs::FsError;

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

/// How a script file found on disk differs from what saving expected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceConflictKind {
    /// The file holds other bytes.
    Modified,
    /// The file is gone, or no longer reachable inside the cartridge.
    Deleted,
}

/// A script file that changed outside Glaux since the document last read,
/// saved or was shown it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceConflict {
    pub path: SourcePath,
    pub kind: SourceConflictKind,
    /// What the file holds now, `None` when it is absent. An overwrite
    /// expects exactly this.
    pub disk: Option<Vec<u8>>,
}

/// A successful [`CodeDocuments::save`] or [`CodeDocuments::overwrite`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CodeSaved {
    /// The text was written to `path` as `bytes` bytes.
    Written { path: SourcePath, bytes: usize },
    /// The text already matched the file as last read or saved. Nothing
    /// was read or written.
    NothingToSave(SourcePath),
}

/// A failed [`CodeDocuments::save`] or [`CodeDocuments::overwrite`]. The
/// file and the document's baseline are left as they were.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CodeSaveError {
    /// No document has that id.
    NotOpen,
    /// The file changed outside Glaux, so nothing was written.
    Conflict(SourceConflict),
    /// The file could not be checked or written.
    Failed { path: SourcePath, error: FsError },
}

/// How a document's file compares with what the document was read from or
/// last saved to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiskState {
    /// The file holds exactly the baseline. Holds those bytes.
    InSync(Vec<u8>),
    /// The file was changed outside Glaux. Holds what it holds now.
    Modified(Vec<u8>),
    /// The file no longer exists.
    Missing,
}

/// Why a document cannot be moved to another path.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CodeRenameError {
    #[error("The script is not open")]
    NotOpen,
    /// Another open document holds the path.
    #[error("{0} is already open in another tab")]
    PathTaken(SourcePath),
    /// A document keeps the language its path names.
    #[error("{0} is not a script of the same language")]
    Language(SourcePath),
}

/// The documents open on one cartridge, at most one per file.
///
/// Documents and their ids belong to the cartridge they were read from:
/// when another cartridge is loaded, the store is replaced and every id it
/// issued is discarded with it.
#[derive(Debug, Default)]
pub struct CodeDocuments {
    next_id: u64,
    pub(super) documents: Vec<CodeDocument>,
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
        tracing::debug!(file = %path, "code document opened");
        self.documents
            .push(CodeDocument::new(id, path.clone(), text));
        Ok(id)
    }

    pub fn get(&self, id: DocumentId) -> Option<&CodeDocument> {
        self.documents.iter().find(|doc| doc.id == id)
    }

    pub fn get_mut(&mut self, id: DocumentId) -> Option<&mut CodeDocument> {
        self.documents.iter_mut().find(|doc| doc.id == id)
    }

    /// Whether any open document differs from its file.
    pub fn is_modified(&self) -> bool {
        self.documents.iter().any(CodeDocument::is_modified)
    }

    /// How document `id`'s file compares with its baseline, read now.
    /// `None` if `id` is not open.
    pub fn disk_state(
        &self,
        cartridge: &LoadedCartridge,
        id: DocumentId,
    ) -> Option<Result<DiskState, FsError>> {
        let document = self.get(id)?;
        Some(read_disk(cartridge, &document.path).map(|disk| match disk {
            None => DiskState::Missing,
            Some(bytes) if bytes == document.origin.bytes() => DiskState::InSync(bytes),
            Some(bytes) => DiskState::Modified(bytes),
        }))
    }

    /// Whether document `id` can be moved to `to`, which checks everything
    /// [`rename`](Self::rename) does before changing anything.
    pub fn can_rename(&self, id: DocumentId, to: &SourcePath) -> Result<(), CodeRenameError> {
        let document = self.get(id).ok_or(CodeRenameError::NotOpen)?;
        if document.path.language() != to.language() {
            return Err(CodeRenameError::Language(to.clone()));
        }
        if self
            .documents
            .iter()
            .any(|doc| doc.id != id && doc.path == *to)
        {
            return Err(CodeRenameError::PathTaken(to.clone()));
        }
        Ok(())
    }

    /// Points document `id` at `to`, after its file was moved there. The
    /// id, text, baseline and revision stay as they are, so the document
    /// keeps its modified state and a later save writes `to`. On error
    /// nothing changes.
    pub fn rename(&mut self, id: DocumentId, to: &SourcePath) -> Result<(), CodeRenameError> {
        self.can_rename(id, to)?;
        let document = self.get_mut(id).ok_or(CodeRenameError::NotOpen)?;
        tracing::debug!(from = %document.path, to = %to, "code document moved");
        document.set_path(to.clone());
        Ok(())
    }

    /// Writes document `id`'s text to its file, provided the file still
    /// holds exactly what the document was read from or last saved to.
    ///
    /// The text is written byte for byte, so its byte order mark, line
    /// endings and final newline, or lack of one, are kept. The file is
    /// replaced whole or not at all. See
    /// [`LoadedCartridge::replace_source`]. Only a successful write makes
    /// the text the document's new baseline.
    ///
    /// The file is compared before anything is written and once more just
    /// before it is replaced. A change seen either time is a
    /// [`CodeSaveError::Conflict`]. No lock is taken, so a change in the
    /// instant between that last comparison and the replacement is
    /// overwritten.
    pub fn save(
        &mut self,
        cartridge: &LoadedCartridge,
        id: DocumentId,
    ) -> Result<CodeSaved, CodeSaveError> {
        self.write(cartridge, id, None)
    }

    /// Like [`save`](Self::save), but writes over a file that changed
    /// outside Glaux, provided it still holds exactly `seen` (`None` means
    /// it must still be absent). A further change is another conflict.
    pub fn overwrite(
        &mut self,
        cartridge: &LoadedCartridge,
        id: DocumentId,
        seen: Option<&[u8]>,
    ) -> Result<CodeSaved, CodeSaveError> {
        self.write(cartridge, id, Some(seen))
    }

    /// Saves `id`, expecting its file to hold `seen` when given, or else
    /// its baseline.
    fn write(
        &mut self,
        cartridge: &LoadedCartridge,
        id: DocumentId,
        seen: Option<Option<&[u8]>>,
    ) -> Result<CodeSaved, CodeSaveError> {
        let document = self.get_mut(id).ok_or(CodeSaveError::NotOpen)?;
        let path = document.path.clone();
        if !document.is_modified() {
            return Ok(CodeSaved::NothingToSave(path));
        }
        let expected = seen.unwrap_or(Some(document.origin.bytes()));
        let failed = |error: FsError| CodeSaveError::Failed {
            path: path.clone(),
            error,
        };
        let conflict = |disk: Option<Vec<u8>>| {
            CodeSaveError::Conflict(SourceConflict {
                path: path.clone(),
                kind: if disk.is_some() {
                    SourceConflictKind::Modified
                } else {
                    SourceConflictKind::Deleted
                },
                disk,
            })
        };

        let disk = read_disk(cartridge, &path).map_err(failed)?;
        if disk.as_deref() != expected {
            tracing::info!(file = %path, "script changed on disk, not saved");
            return Err(conflict(disk));
        }

        let bytes = document.text.as_bytes();
        match cartridge.replace_source(&path, bytes, expected) {
            Ok(()) => {
                let len = bytes.len();
                document.origin.text.clone_from(&document.text);
                tracing::info!(file = %path, bytes = len, "script saved");
                Ok(CodeSaved::Written { path, bytes: len })
            }
            Err(SourceWriteError::Changed(current)) => {
                tracing::info!(file = %path, "script changed on disk while saving, not saved");
                Err(conflict(current))
            }
            Err(SourceWriteError::Failed(err)) => Err(failed(err)),
        }
    }

    /// Closes the document `id`, returning it, or `None` if it is not open.
    /// Unsaved edits are discarded with it.
    pub fn close(&mut self, id: DocumentId) -> Option<CodeDocument> {
        let index = self.documents.iter().position(|doc| doc.id == id)?;
        Some(self.documents.remove(index))
    }
}

/// The bytes of the file at `path` now, `None` when it is absent.
fn read_disk(cartridge: &LoadedCartridge, path: &SourcePath) -> Result<Option<Vec<u8>>, FsError> {
    match cartridge.read_source(path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(FsError::NotFound(_) | FsError::NotADirectory(_)) => Ok(None),
        Err(err) => Err(err),
    }
}
