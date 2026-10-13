//! Script source files of the loaded cartridge, open as text documents.
//!
//! Documents hold file data only. Cursor, selection, scroll position and
//! tab order are state of the views showing them.

mod lines;

use std::ops::Range;

use lines::{BOM, Lines};

use crate::cartridge::source::{SourcePath, SourceWriteError};
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
    pub fn bytes(&self) -> &[u8] {
        self.text.as_bytes()
    }
}

/// The line ending a document's new lines get.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineEnding {
    Lf,
    CrLf,
}

impl LineEnding {
    /// The convention of `text`: that of its first line ending, or `\n`
    /// when it has none.
    fn of(text: &str) -> Self {
        match text.find('\n') {
            Some(at) if text[..at].ends_with('\r') => Self::CrLf,
            _ => Self::Lf,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Lf => "\n",
            Self::CrLf => "\r\n",
        }
    }
}

/// What a document indents its lines with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Indent {
    Tabs,
    Spaces,
}

impl Indent {
    /// The style of `text`: tabs only when more lines start with a tab
    /// than with a space, so files that don't say fall back to spaces.
    fn of(text: &str) -> Self {
        let (mut tabs, mut spaces) = (0usize, 0usize);
        for line in text.trim_start_matches(BOM).split('\n') {
            match line.as_bytes().first() {
                Some(b'\t') => tabs += 1,
                Some(b' ') => spaces += 1,
                _ => {}
            }
        }
        if tabs > spaces {
            Self::Tabs
        } else {
            Self::Spaces
        }
    }
}

/// What a successful [`CodeDocument::replace`] changed: enough to apply
/// the reverse edit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Replaced {
    /// Where the inserted text now sits in the document's text.
    pub range: Range<usize>,
    /// The text the edit took out.
    pub removed: String,
}

/// Why [`CodeDocument::replace`] refused an edit. A refused edit changes
/// nothing.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum EditError {
    #[error("edit range {start}..{end} is outside the text's {len} bytes")]
    OutOfBounds {
        start: usize,
        end: usize,
        len: usize,
    },
    #[error("edit boundary {0} is inside a character")]
    NotCharBoundary(usize),
    #[error("edit boundary {0} is inside the byte order mark")]
    InsideBom(usize),
    #[error("edit boundary {0} is between a \\r and its \\n")]
    SplitsLineEnding(usize),
    #[error("edit would start the text with a byte order mark")]
    CreatesBom,
}

/// One script file of the cartridge as text.
///
/// The text is the file's contents unaltered: line endings, a byte order
/// mark and a missing final newline are all kept, so writing it back
/// reproduces the file byte for byte. Edits change only the bytes they
/// replace.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeDocument {
    id: DocumentId,
    path: SourcePath,
    text: String,
    lines: Lines,
    origin: Origin,
    /// Taken from the file when it was read, so it holds even after edits
    /// remove every line ending.
    line_ending: LineEnding,
    /// Taken from the file when it was read, so it stays the same however
    /// the text is edited.
    indent: Indent,
    /// Counts successful edits. It never goes back, even when an edit
    /// restores earlier text.
    revision: u64,
}

impl CodeDocument {
    /// A document holding `text`, for tests of code that only reads
    /// documents.
    #[cfg(test)]
    pub(crate) fn from_text(path: &str, text: &str) -> Self {
        Self::new(
            DocumentId(0),
            SourcePath::parse(path).expect("test paths are valid"),
            text.to_owned(),
        )
    }

    fn new(id: DocumentId, path: SourcePath, text: String) -> Self {
        Self {
            id,
            path,
            lines: Lines::new(&text),
            origin: Origin { text: text.clone() },
            line_ending: LineEnding::of(&text),
            indent: Indent::of(&text),
            revision: 0,
            text,
        }
    }

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

    /// Lines in the text, counting a last line without a newline and the
    /// empty line after a final newline.
    pub fn line_count(&self) -> usize {
        self.lines.count()
    }

    /// Byte range in [`text`](Self::text) of line `n`'s content, counted
    /// from 0: without its line ending, and for the first line without a
    /// byte order mark. `None` past the last line.
    pub fn line_range(&self, n: usize) -> Option<Range<usize>> {
        self.lines.range(&self.text, n)
    }

    /// The line, counted from 0, holding byte `byte` of
    /// [`text`](Self::text). A line's ending belongs to it, and a byte
    /// order mark to the first line. Offsets past the end are on the last
    /// line.
    pub fn line_of(&self, byte: usize) -> usize {
        self.lines.line_of(byte)
    }

    #[cfg_attr(not(test), expect(dead_code, reason = "no caller outside tests"))]
    pub fn origin(&self) -> &Origin {
        &self.origin
    }

    /// The line ending new lines get: the one the file used when read.
    pub fn line_ending(&self) -> LineEnding {
        self.line_ending
    }

    /// What indentation inserts: the style the file used when read.
    pub fn indent(&self) -> Indent {
        self.indent
    }

    /// Changes whenever the text does, so anything derived from the text
    /// can tell it is out of date. Unrelated to [`is_modified`](Self::is_modified):
    /// an edit that restores the file's text still changes it.
    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// Whether the text differs from what the file held when read.
    pub fn is_modified(&self) -> bool {
        self.text != self.origin.text
    }

    /// Replaces the bytes in `range` of the text with `text`.
    ///
    /// Both ends of `range` must be character boundaries outside the byte
    /// order mark and not between a `\r` and its `\n`, and the edit must
    /// not give a text without a byte order mark one. Otherwise nothing
    /// changes. `text` is inserted exactly as given; callers choose its
    /// line endings.
    pub fn replace(&mut self, range: Range<usize>, text: &str) -> Result<Replaced, EditError> {
        self.check(&range, text)?;
        let removed = self.text[range.clone()].to_owned();
        self.text.replace_range(range.clone(), text);
        self.lines = Lines::new(&self.text);
        self.revision += 1;
        Ok(Replaced {
            range: range.start..range.start + text.len(),
            removed,
        })
    }

    fn check(&self, range: &Range<usize>, text: &str) -> Result<(), EditError> {
        let current = &self.text;
        if range.start > range.end || range.end > current.len() {
            return Err(EditError::OutOfBounds {
                start: range.start,
                end: range.end,
                len: current.len(),
            });
        }
        let bom = if current.starts_with(BOM) {
            BOM.len_utf8()
        } else {
            0
        };
        for at in [range.start, range.end] {
            if !current.is_char_boundary(at) {
                return Err(EditError::NotCharBoundary(at));
            }
            if at < bom {
                return Err(EditError::InsideBom(at));
            }
            if current[..at].ends_with('\r') && current[at..].starts_with('\n') {
                return Err(EditError::SplitsLineEnding(at));
            }
        }
        let starts_with_bom = if text.is_empty() {
            current[range.end..].starts_with(BOM)
        } else {
            text.starts_with(BOM)
        };
        if bom == 0 && range.start == 0 && starts_with_bom {
            return Err(EditError::CreatesBom);
        }
        Ok(())
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
    /// The text already matched the file as last read or saved; nothing
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

    /// Writes document `id`'s text to its file, provided the file still
    /// holds exactly what the document was read from or last saved to.
    ///
    /// The text is written byte for byte, so its byte order mark, line
    /// endings and final newline, or lack of one, are kept. The file is
    /// replaced whole or not at all; see
    /// [`LoadedCartridge::replace_source`]. Only a successful write makes
    /// the text the document's new baseline.
    ///
    /// The file is compared before anything is written and once more just
    /// before it is replaced; a change seen either time is a
    /// [`CodeSaveError::Conflict`]. No lock is taken: a change in the
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
    /// outside Glaux, provided it still holds exactly `seen` (`None`:
    /// provided it is still absent). A further change is another conflict.
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

        let disk = match cartridge.read_source(&path) {
            Ok(bytes) => Some(bytes),
            Err(FsError::NotFound(_) | FsError::NotADirectory(_)) => None,
            Err(err) => return Err(failed(err)),
        };
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

#[cfg(test)]
mod tests;
