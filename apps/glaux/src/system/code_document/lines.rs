//! Where each line of a document's text starts.

use std::ops::Range;

/// The byte order mark a file may start with. It is part of the file, not
/// of its first line.
const BOM: char = '\u{feff}';

/// The lines of one text, split at `\n`.
///
/// A line's content never includes its `\n`, the `\r` before it, or a
/// leading byte order mark. The text after the last `\n` is always a line,
/// so a text ending in `\n` has an empty last line and an empty text has
/// one empty line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Lines {
    /// Byte offset of each line's content in the text. Never empty.
    starts: Vec<usize>,
    /// Length of the text the offsets were taken from.
    len: usize,
}

impl Lines {
    pub fn new(text: &str) -> Self {
        let first = if text.starts_with(BOM) {
            BOM.len_utf8()
        } else {
            0
        };
        let starts = std::iter::once(first)
            .chain(text.match_indices('\n').map(|(at, _)| at + 1))
            .collect();
        Self {
            starts,
            len: text.len(),
        }
    }

    pub fn count(&self) -> usize {
        self.starts.len()
    }

    /// Byte range of line `n`'s content in `text`, the text these lines were
    /// taken from. `None` past the last line.
    pub fn range(&self, text: &str, n: usize) -> Option<Range<usize>> {
        let start = *self.starts.get(n)?;
        let end = match self.starts.get(n + 1) {
            Some(&next) => {
                let newline = next - 1;
                if text.as_bytes()[..newline].ends_with(b"\r") {
                    newline - 1
                } else {
                    newline
                }
            }
            None => self.len,
        };
        Some(start..end)
    }
}

#[cfg(test)]
mod tests;
