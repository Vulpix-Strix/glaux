//! Turns a page's blocks into the rows the browser draws.
//!
//! Every row is one line of text in the system font, so the layout works in
//! cells and knows nothing about the framebuffer. Rows differ only in the
//! pixels of padding they carry, and a few carry decoration for the renderer
//! to draw around the text: the box of a code block, the rules of a table.

use std::collections::HashMap;
use std::ops::Range;

use glaux_docs::{AlertKind, BlockKind, Inline, Page, Section, plain_text};
use glaux_machine::font::BitmapFont;
use glaux_syntax::{Grammar, Highlighter, SyntaxKind};

use super::LanguageFilter;
use crate::system::workspace::{BAR, take_cells};
use crate::system::{ACCENT, DIM, PALE, TEXT, WARN, syntax_theme};

/// Pixels between two blocks.
const BLOCK_GAP: u8 = 4;
/// Pixels above a section heading.
const HEADING_GAP_BEFORE: u8 = 6;
/// Pixels between a section heading's rule and its text below.
const HEADING_GAP_AFTER: u8 = 3;
/// Pixels between a box's border and its text, above and below.
const BOX_PAD: u8 = 2;
/// Columns a tab takes in a code block.
const TAB_COLUMNS: usize = 4;
/// Marks a row that continues the line above it in a code block.
pub const CONTINUATION: &str = "↳";
/// Starts an item of a bulleted list.
const BULLET: &str = "•";
/// Cells a wide enough table gives the gap between two columns.
const GAP: usize = 1;

/// How a stretch of text is drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Style {
    pub ink: u8,
    /// A band behind the text.
    pub bg: Option<u8>,
    pub underline: bool,
    /// The link the text belongs to: an index into [`Layout::links`].
    pub link: Option<usize>,
}

impl Style {
    const fn plain(ink: u8) -> Self {
        Self {
            ink,
            bg: None,
            underline: false,
            link: None,
        }
    }
}

/// Text in one style, starting at a cell of its row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Run {
    pub col: usize,
    pub cells: usize,
    pub text: String,
    pub style: Style,
}

/// The border around a group of rows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Frame {
    pub ink: u8,
    /// A box with all four sides. Otherwise only a bar on the left.
    pub boxed: bool,
    pub first: bool,
    pub last: bool,
}

/// The rules that make a table out of its rows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableLines {
    /// Cells across the table, margins included.
    pub width: usize,
    /// A rule across the top of this row.
    pub rule_above: bool,
    /// That rule is the table's outer edge rather than a line between rows.
    pub outer_top: bool,
    /// A rule across the bottom of this row, which ends the table.
    pub last: bool,
    pub header: bool,
    /// Cells where a column starts, each with a vertical rule just left of
    /// it.
    pub dividers: Vec<usize>,
}

/// What is drawn around a row's text.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Deco {
    /// A band across the row behind the text.
    pub fill: Option<u8>,
    pub frame: Option<Frame>,
    pub table: Option<TableLines>,
    /// A rule along the bottom of the row, as under a section heading.
    pub underline: Option<u8>,
    /// A thematic break: a rule through the middle of the row.
    pub rule: bool,
}

/// One line of the page.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Row {
    /// For a blank gap, its height in pixels. Such a row has no text.
    pub gap: Option<u8>,
    /// Pixels above and below the text line.
    pub top: u8,
    pub bottom: u8,
    pub runs: Vec<Run>,
    pub deco: Deco,
}

impl Row {
    fn gap(pixels: u8) -> Self {
        Self {
            gap: Some(pixels),
            ..Self::default()
        }
    }

    fn line(runs: Vec<Run>) -> Self {
        Self {
            runs,
            ..Self::default()
        }
    }

    /// The row's height in pixels for text lines `cell_h` tall.
    pub fn height(&self, cell_h: i32) -> i32 {
        match self.gap {
            Some(pixels) => i32::from(pixels),
            None => cell_h + i32::from(self.top) + i32::from(self.bottom),
        }
    }
}

/// A heading as the browser can jump to it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Heading {
    pub level: u8,
    pub title: String,
    pub anchor: String,
    /// The row the heading's text is on.
    pub row: usize,
}

/// A page laid out for one width.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Layout {
    pub cell_h: i32,
    pub rows: Vec<Row>,
    /// The target of each link, in the order they appear.
    pub links: Vec<String>,
    /// The first row each link is on, by link index.
    pub link_rows: Vec<usize>,
    /// The headings the filter leaves visible, top to bottom.
    pub headings: Vec<Heading>,
}

impl Layout {
    /// The rows that fit whole in `height` pixels when the view starts at
    /// row `scroll`.
    pub fn visible(&self, scroll: usize, height: i32) -> Range<usize> {
        let mut used = 0;
        let mut end = scroll.min(self.rows.len());
        for row in self.rows.iter().skip(scroll) {
            used += row.height(self.cell_h);
            if used > height {
                break;
            }
            end += 1;
        }
        scroll.min(self.rows.len())..end
    }

    /// The farthest the view can scroll with `height` pixels of room: the
    /// last row it starts at that still shows everything below.
    pub fn max_scroll(&self, height: i32) -> usize {
        let mut used = 0;
        let mut first = self.rows.len();
        for (index, row) in self.rows.iter().enumerate().rev() {
            used += row.height(self.cell_h);
            if used > height {
                break;
            }
            first = index;
        }
        first.min(self.rows.len().saturating_sub(1))
    }

    /// The link whose text covers `col` of row `row`.
    pub fn link_at(&self, row: usize, col: usize) -> Option<usize> {
        self.rows.get(row)?.runs.iter().find_map(|run| {
            let covers = (run.col..run.col + run.cells).contains(&col);
            run.style.link.filter(|_| covers)
        })
    }

    /// The row of the heading with this anchor.
    pub fn anchor_row(&self, anchor: &str) -> Option<usize> {
        self.headings
            .iter()
            .find(|heading| heading.anchor == anchor)
            .map(|heading| heading.row)
    }
}

/// What a layout is built from.
pub struct Params<'a> {
    pub page: &'a Page,
    /// Text right-aligned in the page header, such as `fn  graphics`.
    pub tag: String,
    /// Cells across the text area.
    pub cols: usize,
    pub filter: LanguageFilter,
    pub font: &'a BitmapFont,
}

/// Lays out `params.page`.
pub fn build(params: &Params<'_>) -> Layout {
    let mut builder = Builder {
        font: params.font,
        links: Vec::new(),
        highlighters: HashMap::new(),
    };
    let width = params.cols.max(1);
    let mut rows = vec![builder.header(params, width)];
    let mut headings = Vec::new();
    let mut seen_title = false;
    let mut heading_index = 0;
    let mut after_heading = false;
    for block in params.page.blocks() {
        let outline_index = matches!(block.kind, BlockKind::Heading { .. }).then(|| {
            heading_index += 1;
            heading_index - 1
        });
        if !params.filter.shows(block.scope) {
            continue;
        }
        if matches!(block.kind, BlockKind::Heading { level: 1, .. }) && !seen_title {
            // The header already shows the title.
            seen_title = true;
            continue;
        }
        let mut out = Vec::new();
        if let BlockKind::Heading { level, inlines } = &block.kind {
            builder.heading(*level, inlines, width, &mut out);
            let text_row = out.iter().position(|row| row.gap.is_none()).unwrap_or(0);
            headings.push(Heading {
                level: *level,
                title: plain_text(inlines),
                anchor: outline_index
                    .and_then(|index| params.page.outline().get(index))
                    .map(Section::anchor)
                    .unwrap_or_default(),
                row: rows.len() + text_row,
            });
            after_heading = true;
        } else {
            if !after_heading {
                rows.push(Row::gap(BLOCK_GAP));
            }
            builder.block(&block.kind, width, false, &mut out);
            after_heading = false;
        }
        rows.extend(out);
    }
    let mut link_rows = vec![usize::MAX; builder.links.len()];
    for (index, row) in rows.iter().enumerate() {
        for run in &row.runs {
            if let Some(link) = run.style.link {
                link_rows[link] = link_rows[link].min(index);
            }
        }
    }
    Layout {
        cell_h: i32::from(params.font.metrics().cell_height()),
        rows,
        links: builder.links,
        link_rows,
        headings,
    }
}

/// A layout of one line of `text`, for a page that cannot be shown.
pub fn message(font: &BitmapFont, text: &str) -> Layout {
    let cells = font.text_cells(text);
    Layout {
        cell_h: i32::from(font.metrics().cell_height()),
        rows: vec![Row::line(vec![Run {
            col: 0,
            cells,
            text: text.to_owned(),
            style: Style::plain(DIM),
        }])],
        links: Vec::new(),
        link_rows: Vec::new(),
        headings: Vec::new(),
    }
}

struct Builder<'a> {
    font: &'a BitmapFont,
    links: Vec<String>,
    /// One parser per grammar, made when a page first needs it.
    highlighters: HashMap<Grammar, Option<Highlighter>>,
}

/// A piece of text with its style, before it is cut into lines.
#[derive(Debug, Clone)]
struct Piece {
    text: String,
    style: Style,
}

impl Builder<'_> {
    fn cells(&self, text: &str) -> usize {
        self.font.text_cells(text)
    }

    fn run(&self, col: usize, text: String, style: Style) -> Run {
        Run {
            col,
            cells: self.cells(&text),
            text,
            style,
        }
    }

    /// The bar across the top of the page: its title, and `params.tag` at
    /// the right.
    fn header(&self, params: &Params<'_>, width: usize) -> Row {
        let title = params
            .page
            .title()
            .map_or_else(|| params.page.id().to_string(), str::to_owned);
        let tag = &params.tag;
        let tag_cells = self.cells(tag);
        let room = width.saturating_sub(tag_cells + 1);
        let shown_tag = !tag.is_empty() && self.cells(&title) <= room;
        let title = self.clip(&title, if shown_tag { room } else { width });
        let mut runs = vec![self.run(0, title, Style::plain(TEXT))];
        if shown_tag {
            runs.push(self.run(width - tag_cells, tag.clone(), Style::plain(PALE)));
        }
        Row {
            top: 1,
            bottom: 1,
            runs,
            deco: Deco {
                fill: Some(BAR),
                ..Deco::default()
            },
            ..Row::default()
        }
    }

    /// `text` cut to `cells`, marking the cut with `..`.
    fn clip(&self, text: &str, cells: usize) -> String {
        if self.cells(text) <= cells {
            return text.to_owned();
        }
        let kept = take_cells(self.font, text, cells.saturating_sub(2));
        format!("{kept}..")
    }

    fn heading(&mut self, level: u8, inlines: &[Inline], width: usize, out: &mut Vec<Row>) {
        let (ink, rule) = match level {
            1 | 2 => (TEXT, true),
            _ => (PALE, false),
        };
        let mut pieces = Vec::new();
        self.flatten(inlines, Style::plain(ink), &mut pieces);
        let mut lines = wrap(self.font, &pieces, width);
        out.push(Row::gap(HEADING_GAP_BEFORE));
        let count = lines.len();
        for (index, runs) in lines.drain(..).enumerate() {
            let mut row = Row::line(runs);
            if rule && index + 1 == count {
                row.deco.underline = Some(BAR);
            }
            out.push(row);
        }
        out.push(Row::gap(HEADING_GAP_AFTER));
    }

    fn block(&mut self, kind: &BlockKind, width: usize, nested: bool, out: &mut Vec<Row>) {
        match kind {
            BlockKind::Heading { inlines, .. } | BlockKind::Paragraph(inlines) => {
                let mut pieces = Vec::new();
                let ink = if matches!(kind, BlockKind::Heading { .. }) {
                    TEXT
                } else {
                    PALE
                };
                self.flatten(inlines, Style::plain(ink), &mut pieces);
                out.extend(wrap(self.font, &pieces, width).into_iter().map(Row::line));
            }
            BlockKind::Code { lang, lines } => {
                self.code(lang.as_deref(), lines, width, nested, out)
            }
            BlockKind::List {
                ordered,
                start,
                items,
            } => self.list(*ordered, *start, items, width, out),
            BlockKind::Table { header, rows } => self.table(header, rows, width, out),
            BlockKind::Alert {
                kind,
                title,
                blocks,
            } => self.alert(*kind, title.as_deref(), blocks, width, out),
            BlockKind::Quote(blocks) => {
                let start = out.len();
                self.nested(blocks, width.saturating_sub(2), 2, out);
                frame(&mut out[start..], DIM, false);
            }
            BlockKind::Rule => out.push(Row {
                deco: Deco {
                    rule: true,
                    ..Deco::default()
                },
                ..Row::default()
            }),
        }
    }

    /// Lays out `blocks` inside a container, `width` cells wide and
    /// starting `indent` cells to the right.
    fn nested(&mut self, blocks: &[BlockKind], width: usize, indent: usize, out: &mut Vec<Row>) {
        let mut rows = Vec::new();
        for (index, kind) in blocks.iter().enumerate() {
            if index > 0 {
                rows.push(Row::gap(BLOCK_GAP / 2));
            }
            self.block(kind, width, true, &mut rows);
        }
        for row in &mut rows {
            for run in &mut row.runs {
                run.col += indent;
            }
        }
        out.extend(rows);
    }

    fn list(
        &mut self,
        ordered: bool,
        start: usize,
        items: &[Vec<BlockKind>],
        width: usize,
        out: &mut Vec<Row>,
    ) {
        for (index, item) in items.iter().enumerate() {
            let marker = if ordered {
                format!("{}.", start + index)
            } else {
                BULLET.to_owned()
            };
            let indent = self.cells(&marker) + 1;
            let first = out.len();
            self.nested(item, width.saturating_sub(indent), indent, out);
            let marker = self.run(0, marker, Style::plain(DIM));
            // An empty item still shows its marker.
            match out[first..].iter_mut().find(|row| row.gap.is_none()) {
                Some(row) => row.runs.insert(0, marker),
                None => out.push(Row::line(vec![marker])),
            }
        }
    }

    fn alert(
        &mut self,
        kind: AlertKind,
        title: Option<&str>,
        blocks: &[BlockKind],
        width: usize,
        out: &mut Vec<Row>,
    ) {
        let (name, ink) = match kind {
            AlertKind::Note => ("Note", ACCENT),
            AlertKind::Tip => ("Tip", ACCENT),
            AlertKind::Important => ("Important", ACCENT),
            AlertKind::Warning => ("Warning", WARN),
            AlertKind::Caution => ("Caution", WARN),
        };
        let start = out.len();
        let title = title.unwrap_or(name);
        let title = self.clip(title, width.saturating_sub(2));
        out.push(Row::line(vec![self.run(1, title, Style::plain(ink))]));
        self.nested(blocks, width.saturating_sub(2), 1, out);
        frame(&mut out[start..], ink, true);
    }

    fn code(
        &mut self,
        lang: Option<&str>,
        lines: &[String],
        width: usize,
        nested: bool,
        out: &mut Vec<Row>,
    ) {
        let inner = width.saturating_sub(2).max(2);
        let start = out.len();
        let source = lines.join("\n");
        let inks = self.code_inks(lang, &source);
        let mut offset = 0;
        for line in lines {
            let chars: Vec<(char, u8)> = line
                .char_indices()
                .map(|(at, ch)| (ch, inks.get(offset + at).copied().unwrap_or(TEXT)))
                .collect();
            offset += line.len() + 1;
            let rows = self.code_line(&chars, inner);
            out.extend(rows);
        }
        if out.len() == start {
            out.push(Row::line(Vec::new()));
        }
        for row in &mut out[start..] {
            for run in &mut row.runs {
                run.col += 1;
            }
        }
        if !nested {
            frame(&mut out[start..], DIM, true);
        }
    }

    /// The ink of each byte of `source`, which is plain text unless `lang`
    /// names a script language.
    fn code_inks(&mut self, lang: Option<&str>, source: &str) -> Vec<u8> {
        let mut inks = vec![TEXT; source.len() + 1];
        let grammar = match lang.map(str::to_ascii_lowercase).as_deref() {
            Some("luau") => Grammar::Luau,
            Some("rhai") => Grammar::Rhai,
            _ => return inks,
        };
        let highlighter = self
            .highlighters
            .entry(grammar)
            .or_insert_with(|| Highlighter::new(grammar).ok());
        let Some(highlighter) = highlighter else {
            return inks;
        };
        highlighter.update(source);
        for span in highlighter.spans(0..source.len()) {
            let ink = syntax_theme::color(span.kind);
            for byte in span.range {
                if let Some(slot) = inks.get_mut(byte) {
                    *slot = ink;
                }
            }
        }
        inks
    }

    /// One line of code as rows `width` cells wide. A row that continues
    /// the line before it starts with a dim marker, and its text one cell
    /// further right.
    fn code_line(&self, chars: &[(char, u8)], width: usize) -> Vec<Row> {
        // Tabs become spaces so the cells they take are known.
        let mut cells: Vec<(char, u8)> = Vec::new();
        for &(ch, ink) in chars {
            if ch == '\t' {
                cells.extend(std::iter::repeat_n((' ', ink), TAB_COLUMNS));
            } else if !ch.is_control() {
                cells.push((ch, ink));
            }
        }
        let mut rows = Vec::new();
        let mut at = 0;
        loop {
            let continued = !rows.is_empty();
            let room = if continued { width - 1 } else { width };
            let mut used = 0;
            let mut end = at;
            while let Some(&(ch, _)) = cells.get(end) {
                let take = self.font.cells(ch);
                if used + take > room {
                    break;
                }
                used += take;
                end += 1;
            }
            // A glyph wider than the row still takes a row of its own.
            if end == at && at < cells.len() {
                end += 1;
            }
            let mut runs = Vec::new();
            let mut col = usize::from(continued);
            if continued {
                runs.push(self.run(0, CONTINUATION.to_owned(), Style::plain(DIM)));
            }
            let mut text = String::new();
            let mut ink = None;
            for &(ch, this) in &cells[at..end] {
                if ink.is_some_and(|ink| ink != this) {
                    let run = self.run(
                        col,
                        std::mem::take(&mut text),
                        Style::plain(ink.unwrap_or(TEXT)),
                    );
                    col += run.cells;
                    runs.push(run);
                }
                ink = Some(this);
                text.push(ch);
            }
            if let Some(ink) = ink {
                runs.push(self.run(col, text, Style::plain(ink)));
            }
            rows.push(Row::line(runs));
            at = end;
            if at >= cells.len() {
                break;
            }
        }
        rows
    }

    fn table(
        &mut self,
        header: &[Vec<Inline>],
        body: &[Vec<Vec<Inline>>],
        width: usize,
        out: &mut Vec<Row>,
    ) {
        let columns = header
            .len()
            .max(body.iter().map(Vec::len).max().unwrap_or(0))
            .max(1);
        let mut grid: Vec<Vec<Vec<Piece>>> = Vec::new();
        for (index, cells) in std::iter::once(header)
            .chain(body.iter().map(Vec::as_slice))
            .enumerate()
        {
            let base = Style::plain(if index == 0 { TEXT } else { PALE });
            let mut row = Vec::new();
            for column in 0..columns {
                let mut pieces = Vec::new();
                if let Some(cell) = cells.get(column) {
                    self.flatten(cell, base, &mut pieces);
                }
                row.push(pieces);
            }
            grid.push(row);
        }

        let natural: Vec<usize> = (0..columns)
            .map(|c| {
                grid.iter()
                    .map(|row| self.piece_cells(&row[c]))
                    .max()
                    .unwrap_or(0)
            })
            .collect();
        let narrowest: Vec<usize> = (0..columns)
            .map(|c| {
                grid.iter()
                    .map(|row| self.longest_word(&row[c]))
                    .max()
                    .unwrap_or(0)
                    .max(1)
            })
            .collect();
        // Each column also takes a cell of gap, and the table a margin
        // cell at either end.
        let overhead = columns * GAP + 1;
        let room = width.saturating_sub(overhead);
        if narrowest.iter().sum::<usize>() > room {
            self.stacked(&grid, width, out);
            return;
        }
        let widths = share(&natural, &narrowest, room);
        let mut starts = Vec::new();
        let mut col = 1;
        for w in &widths {
            starts.push(col);
            col += w + GAP;
        }

        let table_width = col;
        let rows = grid.len();
        for (index, cells) in grid.iter().enumerate() {
            let wrapped: Vec<Vec<Vec<Run>>> = cells
                .iter()
                .zip(&widths)
                .map(|(pieces, &w)| wrap(self.font, pieces, w))
                .collect();
            let lines = wrapped.iter().map(Vec::len).max().unwrap_or(1).max(1);
            for line in 0..lines {
                let mut runs = Vec::new();
                for (cell, &col) in wrapped.iter().zip(&starts) {
                    if let Some(cell_runs) = cell.get(line) {
                        runs.extend(cell_runs.iter().cloned().map(|mut run| {
                            run.col += col;
                            run
                        }));
                    }
                }
                out.push(Row {
                    runs,
                    deco: Deco {
                        fill: (index == 0).then_some(BAR),
                        table: Some(TableLines {
                            width: table_width,
                            rule_above: line == 0,
                            outer_top: index == 0 && line == 0,
                            last: index + 1 == rows && line + 1 == lines,
                            header: index == 0,
                            dividers: starts[1..].to_vec(),
                        }),
                        ..Deco::default()
                    },
                    ..Row::default()
                });
            }
        }
    }

    /// A table too narrow for its columns side by side: each row shows its
    /// first cell as a title over its other cells.
    fn stacked(&mut self, grid: &[Vec<Vec<Piece>>], width: usize, out: &mut Vec<Row>) {
        let columns = grid.first().map_or(0, Vec::len);
        let inner = width.saturating_sub(2).max(1);
        let rows = grid.len();
        for (index, cells) in grid.iter().enumerate() {
            let first = out.len();
            for (c, pieces) in cells.iter().enumerate() {
                let mut pieces = pieces.clone();
                if c > 0
                    && columns > 2
                    && let Some(label) = grid[0].get(c)
                {
                    let mut prefixed = label.clone();
                    for piece in &mut prefixed {
                        piece.style = Style::plain(DIM);
                    }
                    prefixed.push(Piece {
                        text: ": ".to_owned(),
                        style: Style::plain(DIM),
                    });
                    prefixed.append(&mut pieces);
                    pieces = prefixed;
                }
                let lines = wrap(self.font, &pieces, inner);
                for mut runs in lines {
                    for run in &mut runs {
                        run.col += 1 + usize::from(c > 0);
                    }
                    let mut row = Row::line(runs);
                    if c == 0 {
                        row.deco.fill = Some(BAR);
                    }
                    out.push(row);
                }
            }
            let count = out.len() - first;
            for (offset, row) in out[first..].iter_mut().enumerate() {
                row.deco.table = Some(TableLines {
                    width,
                    rule_above: offset == 0,
                    outer_top: index == 0 && offset == 0,
                    last: index + 1 == rows && offset + 1 == count,
                    header: false,
                    dividers: Vec::new(),
                });
            }
        }
    }

    /// Cells the pieces take on one line.
    fn piece_cells(&self, pieces: &[Piece]) -> usize {
        pieces.iter().map(|piece| self.cells(&piece.text)).sum()
    }

    /// Cells of the longest word in `pieces`, a word being the characters
    /// between spaces.
    fn longest_word(&self, pieces: &[Piece]) -> usize {
        let text: String = pieces.iter().map(|piece| piece.text.as_str()).collect();
        text.split_whitespace()
            .map(|word| self.cells(word))
            .max()
            .unwrap_or(0)
    }

    /// Appends the pieces `inlines` stands for, starting from `base`.
    fn flatten(&mut self, inlines: &[Inline], base: Style, out: &mut Vec<Piece>) {
        for inline in inlines {
            match inline {
                Inline::Text(text) => out.push(Piece {
                    text: text.clone(),
                    style: base,
                }),
                Inline::Code(text) => out.push(Piece {
                    text: text.clone(),
                    style: Style {
                        ink: syntax_theme::color(SyntaxKind::Variable),
                        bg: Some(BAR),
                        ..base
                    },
                }),
                Inline::Strong(children) => {
                    let style = Style { ink: TEXT, ..base };
                    self.flatten(children, style, out);
                }
                Inline::Emph(children) => {
                    let style = Style {
                        ink: syntax_theme::color(SyntaxKind::Type),
                        ..base
                    };
                    self.flatten(children, style, out);
                }
                Inline::Link { target, children } => {
                    let index = self.links.len();
                    self.links.push(target.clone());
                    let style = Style {
                        ink: ACCENT,
                        underline: true,
                        link: Some(index),
                        ..base
                    };
                    self.flatten(children, style, out);
                }
                Inline::Break => out.push(Piece {
                    text: "\n".to_owned(),
                    style: base,
                }),
            }
        }
    }
}

/// Marks `rows` as one framed group.
fn frame(rows: &mut [Row], ink: u8, boxed: bool) {
    let count = rows.len();
    for (index, row) in rows.iter_mut().enumerate() {
        if row.gap.is_some() {
            // A gap inside the group stays blank but keeps the border.
            row.deco.frame = Some(Frame {
                ink,
                boxed,
                first: false,
                last: false,
            });
            continue;
        }
        row.deco.frame = Some(Frame {
            ink,
            boxed,
            first: index == 0,
            last: index + 1 == count,
        });
    }
    if boxed {
        if let Some(first) = rows.first_mut() {
            first.top += BOX_PAD;
        }
        if let Some(last) = rows.last_mut() {
            last.bottom += BOX_PAD;
        }
    }
}

/// Column widths for `room` cells, between each column's `narrowest` and
/// `natural` width, taking from the widest unmet need first.
fn share(natural: &[usize], narrowest: &[usize], room: usize) -> Vec<usize> {
    let mut widths = narrowest.to_vec();
    if natural.iter().sum::<usize>() <= room {
        return natural.to_vec();
    }
    let mut spare = room.saturating_sub(widths.iter().sum());
    while spare > 0 {
        let Some((column, _)) = widths
            .iter()
            .zip(natural)
            .map(|(width, natural)| natural.saturating_sub(*width))
            .enumerate()
            .filter(|(_, need)| *need > 0)
            .max_by_key(|(column, need)| (*need, std::cmp::Reverse(*column)))
        else {
            break;
        };
        widths[column] += 1;
        spare -= 1;
    }
    widths
}

/// A word: pieces with no space between them.
struct Word {
    pieces: Vec<Piece>,
    cells: usize,
}

/// Cuts `pieces` into lines of at most `width` cells, at spaces where
/// possible. Spaces keep the style of the text around them when both sides
/// agree, so an inline code band or a link underline runs unbroken across a
/// space.
fn wrap(font: &BitmapFont, pieces: &[Piece], width: usize) -> Vec<Vec<Run>> {
    let width = width.max(1);
    // Tokens: words, the style of the space after a word, and forced breaks.
    enum Token {
        Word(Word),
        Space(Style),
        Break,
    }
    let mut tokens = Vec::new();
    let mut word = Word {
        pieces: Vec::new(),
        cells: 0,
    };
    let flush = |word: &mut Word, tokens: &mut Vec<Token>| {
        if !word.pieces.is_empty() {
            tokens.push(Token::Word(std::mem::replace(
                word,
                Word {
                    pieces: Vec::new(),
                    cells: 0,
                },
            )));
        }
    };
    for piece in pieces {
        let mut current = String::new();
        let finish = |current: &mut String, word: &mut Word| {
            if !current.is_empty() {
                let text = std::mem::take(current);
                word.cells += font.text_cells(&text);
                word.pieces.push(Piece {
                    text,
                    style: piece.style,
                });
            }
        };
        for ch in piece.text.chars() {
            if ch == '\n' {
                finish(&mut current, &mut word);
                flush(&mut word, &mut tokens);
                tokens.push(Token::Break);
            } else if ch.is_whitespace() {
                finish(&mut current, &mut word);
                flush(&mut word, &mut tokens);
                tokens.push(Token::Space(piece.style));
            } else if ch.is_control() {
                // Drawn as nothing, so it takes no cell worth keeping.
            } else {
                current.push(ch);
            }
        }
        finish(&mut current, &mut word);
    }
    flush(&mut word, &mut tokens);

    let mut lines: Vec<Vec<Piece>> = Vec::new();
    let mut line: Vec<Piece> = Vec::new();
    let mut used = 0;
    let mut pending_space: Option<Style> = None;
    for token in tokens {
        match token {
            Token::Break => {
                lines.push(std::mem::take(&mut line));
                used = 0;
                pending_space = None;
            }
            Token::Space(style) => {
                if used > 0 {
                    pending_space.get_or_insert(style);
                }
            }
            Token::Word(word) => {
                let space = usize::from(pending_space.is_some());
                if used > 0 && used + space + word.cells > width {
                    lines.push(std::mem::take(&mut line));
                    used = 0;
                    pending_space = None;
                }
                if let Some(style) = pending_space.take() {
                    line.push(Piece {
                        text: " ".to_owned(),
                        style,
                    });
                    used += 1;
                }
                if word.cells <= width {
                    used += word.cells;
                    line.extend(word.pieces);
                    continue;
                }
                // Longer than a row: break it anywhere.
                for piece in word.pieces {
                    let mut chunk = String::new();
                    for ch in piece.text.chars() {
                        let cells = font.cells(ch);
                        if used + cells > width && used > 0 {
                            if !chunk.is_empty() {
                                line.push(Piece {
                                    text: std::mem::take(&mut chunk),
                                    style: piece.style,
                                });
                            }
                            lines.push(std::mem::take(&mut line));
                            used = 0;
                        }
                        chunk.push(ch);
                        used += cells;
                    }
                    if !chunk.is_empty() {
                        line.push(Piece {
                            text: chunk,
                            style: piece.style,
                        });
                    }
                }
            }
        }
    }
    if !line.is_empty() || lines.is_empty() {
        lines.push(line);
    }

    lines
        .into_iter()
        .map(|pieces| {
            let mut runs: Vec<Run> = Vec::new();
            let mut col = 0;
            for piece in pieces {
                let cells = font.text_cells(&piece.text);
                match runs.last_mut() {
                    Some(last) if last.style == piece.style => {
                        last.text.push_str(&piece.text);
                        last.cells += cells;
                    }
                    _ => runs.push(Run {
                        col,
                        cells,
                        text: piece.text,
                        style: piece.style,
                    }),
                }
                col += cells;
            }
            runs
        })
        .collect()
}

#[cfg(test)]
mod tests;
