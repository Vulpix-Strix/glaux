//! Script source files of the loaded cartridge, open as text documents.
//!
//! Documents hold file data only. Cursor, selection, scroll position and
//! tab order are state of the views showing them.

mod lines;
mod store;

use std::ops::Range;

use lines::{BOM, Lines};
pub use store::{
    CodeDocumentError, CodeDocuments, CodeRenameError, CodeSaveError, CodeSaved, DiskState,
    SourceConflict, SourceConflictKind,
};

use crate::cartridge::Language;
use crate::cartridge::source::SourcePath;

/// Identifies one open document within its [`CodeDocuments`].
///
/// Ids are never reused by the store that issued them, so a file closed and
/// opened again gets a new one. They mean nothing to any other store. A new
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

/// Widths a space indentation step may have, in columns.
const INDENT_WIDTHS: std::ops::RangeInclusive<usize> = 1..=8;

/// The columns one indentation step of `text` takes when it indents with
/// spaces: the most common increase in leading spaces from one non-blank
/// line to the next, the narrowest on a tie. `None` when no line indents
/// further than the one before it.
///
/// Lines whose indentation holds a tab are skipped, as are blank lines,
/// which leave the comparison to the next non-blank one.
fn space_width(text: &str) -> Option<usize> {
    let mut counts = [0usize; *INDENT_WIDTHS.end() + 1];
    let mut previous = 0;
    for line in text.trim_start_matches(BOM).split('\n') {
        let line = line.strip_suffix('\r').unwrap_or(line);
        let content = line.trim_start_matches([' ', '\t']);
        if content.is_empty() {
            continue;
        }
        let indent = &line[..line.len() - content.len()];
        if indent.contains('\t') {
            continue;
        }
        let spaces = indent.len();
        if let Some(step) = spaces
            .checked_sub(previous)
            .filter(|step| INDENT_WIDTHS.contains(step))
        {
            counts[step] += 1;
        }
        previous = spaces;
    }
    INDENT_WIDTHS
        .filter(|&width| counts[width] > 0)
        .max_by(|&a, &b| counts[a].cmp(&counts[b]).then(b.cmp(&a)))
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
    /// Columns of one space indentation step in the file when it was read,
    /// if it shows one.
    indent_width: Option<usize>,
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
            indent_width: space_width(&text),
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

    /// Points the document at the file it was moved to. Text, baseline and
    /// everything derived from the text are untouched.
    fn set_path(&mut self, path: SourcePath) {
        self.path = path;
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

    /// The line ending new lines get, which is the one the file used when read.
    pub fn line_ending(&self) -> LineEnding {
        self.line_ending
    }

    /// What indentation inserts, which is the style the file used when read.
    pub fn indent(&self) -> Indent {
        self.indent
    }

    /// Columns one space indentation step took in the file when it was
    /// read, `None` when the file shows no step.
    pub fn indent_width(&self) -> Option<usize> {
        self.indent_width
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
    /// changes. `text` is inserted exactly as given. Callers choose its
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

#[cfg(test)]
mod tests;
