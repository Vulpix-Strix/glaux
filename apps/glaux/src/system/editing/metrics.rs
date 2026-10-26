//! How a document's text maps onto the character cells of a text view:
//! the columns each character takes, where wrapped lines fold, and the
//! extent of the whole document in rows and columns.
//!
//! Positions are always byte offsets into the document's unaltered text,
//! so whatever a cell shows, it still names the bytes it came from.

use std::ops::Range;

use glaux_machine::font::BitmapFont;

use super::document::CodeDocument;

/// How many columns characters take: a tab reaches the next tab stop, and
/// anything else takes the cells of the glyph `font` draws it with, so a
/// double-width glyph takes two.
#[derive(Debug, Clone, Copy)]
pub struct Columns<'a> {
    pub font: &'a BitmapFont,
    pub tab_size: usize,
}

impl Columns<'_> {
    /// Columns `ch` takes when it starts at column `column` of its line.
    fn width(&self, ch: char, column: usize) -> usize {
        if ch == '\t' {
            let tab_size = self.tab_size.max(1);
            tab_size - column % tab_size
        } else {
            self.font.cells(ch)
        }
    }
}

/// One character of a line as a view lays it out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cell {
    /// The character exactly as the text holds it.
    pub ch: char,
    /// Offset of the character in the document's text.
    pub byte: usize,
    /// Column of the cell's left edge, counted from the start of its line.
    pub column: usize,
    /// Columns the character takes, as [`Columns`] measures it.
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
pub fn cells<'a>(text: &'a str, byte: usize, column: usize, columns: Columns<'a>) -> Cells<'a> {
    Cells {
        chars: text.char_indices(),
        byte,
        column,
        columns,
    }
}

/// Iterator returned by [`cells`].
#[derive(Debug, Clone)]
pub struct Cells<'a> {
    chars: std::str::CharIndices<'a>,
    byte: usize,
    column: usize,
    columns: Columns<'a>,
}

impl Iterator for Cells<'_> {
    type Item = Cell;

    fn next(&mut self) -> Option<Cell> {
        let (offset, ch) = self.chars.next()?;
        let width = self.columns.width(ch, self.column);
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
pub fn line_width(text: &str, columns: Columns<'_>) -> usize {
    cells(text, 0, 0, columns)
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
/// `width` columns. `None` when the rest of the line fits on that row.
///
/// A row folds after the last blank that fits, so words stay whole. A word
/// wider than the row is cut at its edge. Blanks at a fold stay at the end
/// of the row they follow, even past its edge.
pub fn next_row(
    line: &str,
    line_start: usize,
    from: RowStart,
    width: usize,
    columns: Columns<'_>,
) -> Option<RowStart> {
    let end = from.column + width.max(1);
    // The start of the last word on this row that follows a blank after
    // another word, so indentation alone never makes a row.
    let mut fold = None;
    let mut after_space = false;
    let mut seen_word = false;
    for cell in cells(
        &line[from.byte - line_start..],
        from.byte,
        from.column,
        columns,
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
///
/// The font is not part of the key: a view measures every document with the
/// one system font it draws with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MetricsKey {
    pub tab_size: usize,
    /// Columns lines fold at, or `None` when they don't fold.
    pub wrap_columns: Option<usize>,
}

impl MetricsKey {
    /// How characters take columns with this key and `font`.
    pub fn columns(self, font: &BitmapFont) -> Columns<'_> {
        Columns {
            font,
            tab_size: self.tab_size,
        }
    }
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
    /// The document's [`revision`](CodeDocument::revision) when measured.
    revision: u64,
}

impl TextMetrics {
    /// Measures `document` with `key`, as `font` draws it.
    pub fn measure(document: &CodeDocument, key: MetricsKey, font: &BitmapFont) -> Self {
        let columns = key.columns(font);
        let lines = document.line_count();
        let mut widest = 0;
        let mut row_starts = Vec::new();
        if key.wrap_columns.is_some() {
            row_starts.reserve(lines + 1);
        }
        let mut rows = 0;
        for n in 0..lines {
            let (line, range) = line_and_range(document, n);
            widest = widest.max(line_width(line, columns));
            if let Some(width) = key.wrap_columns {
                row_starts.push(rows);
                rows += wrapped_rows(line, range.start, width, columns);
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
            revision: document.revision(),
        }
    }

    pub fn key(&self) -> MetricsKey {
        self.key
    }

    /// Whether these measurements still describe `document` as a view
    /// measuring with `key` shows it: taken with `key`, and since its text
    /// last changed.
    pub fn describes(&self, document: &CodeDocument, key: MetricsKey) -> bool {
        self.key == key && self.revision == document.revision()
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

/// Rows `line` takes when folded to `width` columns.
fn wrapped_rows(line: &str, line_start: usize, width: usize, columns: Columns<'_>) -> usize {
    let mut row = RowStart {
        byte: line_start,
        column: 0,
    };
    let mut rows = 1;
    while let Some(next) = next_row(line, line_start, row, width, columns) {
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
    pub fn cells<'b>(&'b self, columns: Columns<'b>) -> Cells<'b> {
        cells(self.text, self.start.byte, self.start.column, columns)
    }

    /// The document position on this row nearest the left edge of column
    /// `column` of its line. See [`byte_near`](Self::byte_near).
    pub fn byte_near_column(&self, column: usize, columns: Columns<'_>) -> usize {
        self.byte_near(column.saturating_mul(2), columns)
    }

    /// The document position on this row nearest the point `half_columns`
    /// half-columns into its line, where a character spanning the point,
    /// such as a tab or a double-width glyph, gives way to whichever of its
    /// edges is closer. Half-columns let a point inside a two-column
    /// character fall on either side of its middle.
    ///
    /// Points past the row's end give its end. A row its line folds after
    /// ends before its last character, since the position after it starts
    /// the next row.
    pub fn byte_near(&self, half_columns: usize, columns: Columns<'_>) -> usize {
        let mut last = None;
        for cell in self.cells(columns) {
            if half_columns < 2 * (cell.column + cell.width) {
                let after = cell.byte + cell.ch.len_utf8();
                let ends_fold = self.folds && after == self.end();
                let into = half_columns.saturating_sub(2 * cell.column);
                return if into < cell.width || ends_fold {
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
/// with `key` and `font`. `byte` must be a character boundary within a
/// line's content. A position where a line folds shows at the start of the
/// next row.
pub fn spot(document: &CodeDocument, byte: usize, key: MetricsKey, font: &BitmapFont) -> Spot {
    let columns = key.columns(font);
    let line = document.line_of(byte);
    let (text, range) = line_and_range(document, line);
    let byte = byte.clamp(range.start, range.end);
    let mut row_start = RowStart {
        byte: range.start,
        column: 0,
    };
    let mut row_in_line = 0;
    if let Some(width) = key.wrap_columns {
        while let Some(next) =
            next_row(text, range.start, row_start, width, columns).filter(|next| next.byte <= byte)
        {
            row_start = next;
            row_in_line += 1;
        }
    }
    let column = cells(
        &text[row_start.byte - range.start..byte - range.start],
        row_start.byte,
        row_start.column,
        columns,
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
pub fn visual_rows<'a>(
    document: &'a CodeDocument,
    line: usize,
    row: usize,
    wrap_columns: Option<usize>,
    columns: Columns<'a>,
) -> VisualRows<'a> {
    VisualRows {
        document,
        line,
        first_line: line,
        skip: row,
        next: None,
        row_in_line: 0,
        wrap_columns,
        columns,
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
    columns: Columns<'a>,
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
                .and_then(|width| next_row(line, range.start, start, width, self.columns));
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
