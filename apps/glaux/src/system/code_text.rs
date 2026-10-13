//! How a document's text maps onto the character cells of a text view:
//! the columns each character takes, where wrapped lines fold, and the
//! extent of the whole document in rows and columns.
//!
//! Positions are always byte offsets into the document's unaltered text,
//! so whatever a cell shows, it still names the bytes it came from.

use std::ops::Range;

use super::code_document::CodeDocument;

/// One character of a line as a view lays it out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cell {
    /// The character exactly as the text holds it.
    pub ch: char,
    /// Offset of the character in the document's text.
    pub byte: usize,
    /// Column of the cell's left edge, counted from the start of its line.
    pub column: usize,
    /// Columns the character takes: up to the next tab stop for a tab, one
    /// for anything else.
    pub width: usize,
}

impl Cell {
    /// Whether the cell is blank space a wrapped line may fold after.
    pub fn is_space(&self) -> bool {
        matches!(self.ch, ' ' | '\t')
    }
}

/// The cells of `text`, which starts at byte `byte` of the document and at
/// column `column` of its line.
pub fn cells(text: &str, byte: usize, column: usize, tab_size: usize) -> Cells<'_> {
    Cells {
        chars: text.char_indices(),
        byte,
        column,
        tab_size: tab_size.max(1),
    }
}

/// Iterator returned by [`cells`].
#[derive(Debug, Clone)]
pub struct Cells<'a> {
    chars: std::str::CharIndices<'a>,
    byte: usize,
    column: usize,
    tab_size: usize,
}

impl Iterator for Cells<'_> {
    type Item = Cell;

    fn next(&mut self) -> Option<Cell> {
        let (offset, ch) = self.chars.next()?;
        let width = if ch == '\t' {
            self.tab_size - self.column % self.tab_size
        } else {
            1
        };
        let cell = Cell {
            ch,
            byte: self.byte + offset,
            column: self.column,
            width,
        };
        self.column += width;
        Some(cell)
    }
}

/// Columns `text` takes from the start of a line.
pub fn line_width(text: &str, tab_size: usize) -> usize {
    cells(text, 0, 0, tab_size)
        .last()
        .map_or(0, |cell| cell.column + cell.width)
}

/// Where one row of a wrapped line starts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RowStart {
    /// Offset in the document's text.
    pub byte: usize,
    /// Column within the line.
    pub column: usize,
}

/// Where the row after the one starting at `from` begins, when `line`
/// (the content of the line starting at byte `line_start`) is folded to
/// `columns`. `None` when the rest of the line fits on that row.
///
/// A row folds after the last blank that fits, so words stay whole; a word
/// wider than the row is cut at its edge. Blanks at a fold stay at the end
/// of the row they follow, even past its edge.
pub fn next_row(
    line: &str,
    line_start: usize,
    from: RowStart,
    columns: usize,
    tab_size: usize,
) -> Option<RowStart> {
    let end = from.column + columns.max(1);
    // The start of the last word on this row that follows a blank after
    // another word, so indentation alone never makes a row.
    let mut fold = None;
    let mut after_space = false;
    let mut seen_word = false;
    for cell in cells(
        &line[from.byte - line_start..],
        from.byte,
        from.column,
        tab_size,
    ) {
        if cell.is_space() {
            after_space = seen_word;
            continue;
        }
        let start = RowStart {
            byte: cell.byte,
            column: cell.column,
        };
        if after_space {
            fold = Some(start);
        }
        after_space = false;
        seen_word = true;
        // Every row holds at least its first character, so the row after
        // always starts further along the line.
        if cell.column + cell.width > end && cell.column > from.column {
            return Some(fold.unwrap_or(start));
        }
    }
    None
}

/// What a document's extent was measured with. Measurements taken with
/// another key no longer describe the view.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MetricsKey {
    pub tab_size: usize,
    /// Columns lines fold at, or `None` when they don't fold.
    pub wrap_columns: Option<usize>,
}

/// The extent of a whole document in a text view.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextMetrics {
    key: MetricsKey,
    /// Columns of the widest line.
    widest: usize,
    /// Rows above each line and, last, rows in all. Empty when lines don't
    /// fold, as each line is then one row.
    row_starts: Vec<usize>,
    lines: usize,
}

impl TextMetrics {
    /// Measures `document` with `key`.
    pub fn measure(document: &CodeDocument, key: MetricsKey) -> Self {
        let lines = document.line_count();
        let mut widest = 0;
        let mut row_starts = Vec::new();
        if key.wrap_columns.is_some() {
            row_starts.reserve(lines + 1);
        }
        let mut rows = 0;
        for n in 0..lines {
            let (line, range) = line_and_range(document, n);
            widest = widest.max(line_width(line, key.tab_size));
            if let Some(columns) = key.wrap_columns {
                row_starts.push(rows);
                rows += wrapped_rows(line, range.start, columns, key.tab_size);
            }
        }
        if key.wrap_columns.is_some() {
            row_starts.push(rows);
        }
        Self {
            key,
            widest,
            row_starts,
            lines,
        }
    }

    pub fn key(&self) -> MetricsKey {
        self.key
    }

    /// Columns of the widest line.
    pub fn widest(&self) -> usize {
        self.widest
    }

    /// Rows the whole document takes.
    pub fn rows(&self) -> usize {
        self.row_starts.last().copied().unwrap_or(self.lines)
    }

    /// The row, counted over the whole document, of row `row` of `line`.
    /// Positions past the end are moved to the last row.
    pub fn row_index(&self, line: usize, row: usize) -> usize {
        let last = self.rows().saturating_sub(1);
        if self.row_starts.is_empty() {
            return line.min(last);
        }
        let line = line.min(self.lines.saturating_sub(1));
        let in_line = self.row_starts[line + 1] - self.row_starts[line];
        (self.row_starts[line] + row.min(in_line.saturating_sub(1))).min(last)
    }

    /// The line and row within it of document row `index`, the inverse of
    /// [`row_index`](Self::row_index).
    pub fn position(&self, index: usize) -> (usize, usize) {
        let index = index.min(self.rows().saturating_sub(1));
        if self.row_starts.is_empty() {
            return (index, 0);
        }
        let line = self.row_starts[..self.lines]
            .partition_point(|&start| start <= index)
            .saturating_sub(1);
        (line, index - self.row_starts[line])
    }
}

/// Rows `line` takes when folded to `columns`.
fn wrapped_rows(line: &str, line_start: usize, columns: usize, tab_size: usize) -> usize {
    let mut row = RowStart {
        byte: line_start,
        column: 0,
    };
    let mut rows = 1;
    while let Some(next) = next_row(line, line_start, row, columns, tab_size) {
        row = next;
        rows += 1;
    }
    rows
}

/// Line `n` of `document` and its byte range, which exists for every line
/// below the document's line count.
fn line_and_range(document: &CodeDocument, n: usize) -> (&str, Range<usize>) {
    let range = document.line_range(n).unwrap_or(0..0);
    (&document.text()[range.clone()], range)
}

/// One row of a text view: a whole line, or one fold of a wrapped line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VisualRow<'a> {
    /// The source line, counted from 0.
    pub line: usize,
    /// Which fold of its line the row is, 0 for the line's first row.
    pub row_in_line: usize,
    /// The row's text, a slice of its line.
    pub text: &'a str,
    /// Where `text` starts in the document and in its line.
    pub start: RowStart,
    /// Whether the line goes on in the next row.
    pub folds: bool,
}

impl VisualRow<'_> {
    /// The row's cells.
    pub fn cells(&self, tab_size: usize) -> Cells<'_> {
        cells(self.text, self.start.byte, self.start.column, tab_size)
    }

    /// The document position on this row nearest column `column` of its
    /// line, where a character spanning the column, such as a tab, gives
    /// way to whichever of its edges is closer.
    ///
    /// Columns past the row's end give its end. A row its line folds
    /// after ends before its last character, since the position after it
    /// starts the next row.
    pub fn byte_near_column(&self, column: usize, tab_size: usize) -> usize {
        let mut last = None;
        for cell in self.cells(tab_size) {
            if column < cell.column + cell.width {
                let after = cell.byte + cell.ch.len_utf8();
                let ends_fold = self.folds && after == self.end();
                return if (column - cell.column.min(column)) * 2 < cell.width || ends_fold {
                    cell.byte
                } else {
                    after
                };
            }
            last = Some(cell);
        }
        match last {
            Some(cell) if self.folds => cell.byte,
            _ => self.end(),
        }
    }

    /// Offset in the document just past the row's text.
    fn end(&self) -> usize {
        self.start.byte + self.text.len()
    }
}

/// Where a document position shows in a text view.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Spot {
    /// The source line, counted from 0.
    pub line: usize,
    /// Which fold of its line the position is on, 0 for the first.
    pub row_in_line: usize,
    /// Where that fold starts.
    pub row_start: RowStart,
    /// Column of the position within its line, tabs expanded.
    pub column: usize,
}

/// Where position `byte` of `document` shows when the view is laid out
/// with `key`. `byte` must be a character boundary within a line's
/// content; a position where a line folds shows at the start of the next
/// row.
pub fn spot(document: &CodeDocument, byte: usize, key: MetricsKey) -> Spot {
    let line = document.line_of(byte);
    let (text, range) = line_and_range(document, line);
    let byte = byte.clamp(range.start, range.end);
    let mut row_start = RowStart {
        byte: range.start,
        column: 0,
    };
    let mut row_in_line = 0;
    if let Some(columns) = key.wrap_columns {
        while let Some(next) = next_row(text, range.start, row_start, columns, key.tab_size)
            .filter(|next| next.byte <= byte)
        {
            row_start = next;
            row_in_line += 1;
        }
    }
    let column = cells(
        &text[row_start.byte - range.start..byte - range.start],
        row_start.byte,
        row_start.column,
        key.tab_size,
    )
    .last()
    .map_or(row_start.column, |cell| cell.column + cell.width);
    Spot {
        line,
        row_in_line,
        row_start,
        column,
    }
}

/// The rows of `document` from row `row` of line `line` on, unfolded, or
/// folded at `wrap_columns`.
pub fn visual_rows(
    document: &CodeDocument,
    line: usize,
    row: usize,
    wrap_columns: Option<usize>,
    tab_size: usize,
) -> VisualRows<'_> {
    VisualRows {
        document,
        line,
        first_line: line,
        skip: row,
        next: None,
        row_in_line: 0,
        wrap_columns,
        tab_size,
    }
}

/// Iterator returned by [`visual_rows`].
#[derive(Debug, Clone)]
pub struct VisualRows<'a> {
    document: &'a CodeDocument,
    line: usize,
    first_line: usize,
    /// Rows of the first line to pass over.
    skip: usize,
    /// Where the next row of `line` starts, once its first row is out.
    next: Option<RowStart>,
    row_in_line: usize,
    wrap_columns: Option<usize>,
    tab_size: usize,
}

impl<'a> Iterator for VisualRows<'a> {
    type Item = VisualRow<'a>;

    fn next(&mut self) -> Option<VisualRow<'a>> {
        loop {
            let range = self.document.line_range(self.line)?;
            let line = &self.document.text()[range.clone()];
            let start = self.next.take().unwrap_or(RowStart {
                byte: range.start,
                column: 0,
            });
            let following = self
                .wrap_columns
                .and_then(|columns| next_row(line, range.start, start, columns, self.tab_size));
            let end = following.map_or(range.end, |next| next.byte);
            let row = VisualRow {
                line: self.line,
                row_in_line: self.row_in_line,
                text: &self.document.text()[start.byte..end],
                start,
                folds: following.is_some(),
            };
            match following {
                Some(next) => {
                    self.next = Some(next);
                    self.row_in_line += 1;
                }
                None => {
                    self.line += 1;
                    self.row_in_line = 0;
                }
            }
            if row.line == self.first_line && row.row_in_line < self.skip {
                continue;
            }
            return Some(row);
        }
    }
}

#[cfg(test)]
mod tests;
