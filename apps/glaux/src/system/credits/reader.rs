//! Lays embedded license text out for reading without altering it. Wrapping
//! and reflowing only change where lines break on screen. Layout counts one
//! cell per character, which is exact for the ASCII the legal texts use.

use glaux_input::InputEvent;
use glaux_machine::framebuffer::WIDTH;

use super::catalog::Catalog;
use super::paint::{BAR, Painter, ROW};
use crate::system::screen::{BACKGROUND, TEXT};
use crate::ui_text;
use glaux_i18n::Arg;

const COLUMNS: usize = 50;
pub(super) const VISIBLE: usize = 16;
const TEXT_TOP: i32 = 26;
const TAB_WIDTH: usize = 8;
const INK: u8 = BACKGROUND;
const PAPER: u8 = TEXT;
const PAPER_MUTE: u8 = BAR;
const PAPER_HOVER: u8 = 15;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Mode {
    Wrap,
    Reflow,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Line {
    pub(super) text: String,
    pub(super) continued: bool,
}

/// Lays `content` out in `columns` columns.
pub(super) fn layout(content: &str, mode: Mode, columns: usize) -> Vec<Line> {
    let columns = columns.max(2);
    let source: Vec<String> = content.lines().map(expand_tabs).collect();
    let mut out = Vec::new();
    match mode {
        Mode::Wrap => source
            .iter()
            .for_each(|line| wrap_line(line, columns, &mut out)),
        Mode::Reflow => {
            let mut paragraph: Vec<&str> = Vec::new();
            for line in &source {
                if line.trim().is_empty() {
                    flush_paragraph(&mut paragraph, columns, &mut out);
                    out.push(Line {
                        text: String::new(),
                        continued: false,
                    });
                } else {
                    paragraph.push(line);
                }
            }
            flush_paragraph(&mut paragraph, columns, &mut out);
        }
    }
    out
}

fn expand_tabs(line: &str) -> String {
    let mut out = String::new();
    let mut column = 0;
    for ch in line.chars() {
        if ch == '\t' {
            let fill = TAB_WIDTH - column % TAB_WIDTH;
            out.extend(std::iter::repeat_n(' ', fill));
            column += fill;
        } else {
            out.push(ch);
            column += 1;
        }
    }
    out
}

fn flush_paragraph(paragraph: &mut Vec<&str>, columns: usize, out: &mut Vec<Line>) {
    let Some(first) = paragraph.first() else {
        return;
    };
    let indent = first.len() - first.trim_start().len();
    let joined = paragraph
        .iter()
        .map(|line| line.trim())
        .collect::<Vec<_>>()
        .join(" ");
    wrap_line(&format!("{}{joined}", " ".repeat(indent)), columns, out);
    paragraph.clear();
}

/// Wraps one line, keeping its indentation on the lines that carry on.
fn wrap_line(line: &str, columns: usize, out: &mut Vec<Line>) {
    let chars: Vec<char> = line.chars().collect();
    if chars.len() <= columns {
        out.push(Line {
            text: line.to_owned(),
            continued: false,
        });
        return;
    }
    let lead = chars.iter().take_while(|&&ch| ch == ' ').count();
    // A deep indent would leave no room, so it is not carried over.
    let indent = if lead * 2 < columns { lead } else { 0 };
    let mut start = 0;
    let mut first = true;
    while start < chars.len() {
        let room = if first { columns } else { columns - indent };
        if chars.len() - start <= room {
            push(&chars[start..], first, indent, out);
            break;
        }
        let window = &chars[start..start + room];
        // Break after the last space that leaves something on this line.
        let cut = window
            .iter()
            .rposition(|&ch| ch == ' ')
            .filter(|&at| at > if first { lead } else { 0 });
        match cut {
            Some(at) => {
                push(&chars[start..start + at], first, indent, out);
                start += at;
                while chars.get(start) == Some(&' ') {
                    start += 1;
                }
            }
            None => {
                push(window, first, indent, out);
                start += room;
            }
        }
        first = false;
    }
}

fn push(chars: &[char], first: bool, indent: usize, out: &mut Vec<Line>) {
    let mut text = String::new();
    if !first {
        text.extend(std::iter::repeat_n(' ', indent));
    }
    text.extend(chars);
    out.push(Line {
        text,
        continued: !first,
    });
}

#[derive(Debug, Clone)]
pub(super) struct Reader {
    entry: usize,
    text: usize,
    mode: Mode,
    scroll: usize,
    lines: Vec<Line>,
}

impl Reader {
    pub(super) fn open(catalog: &Catalog, entry: usize, text: usize) -> Option<Self> {
        let mut reader = Self {
            entry,
            text,
            mode: Mode::Wrap,
            scroll: 0,
            lines: Vec::new(),
        };
        reader.relayout(catalog)?;
        Some(reader)
    }

    fn relayout(&mut self, catalog: &Catalog) -> Option<()> {
        let texts = catalog.texts_of(self.entry);
        let text = texts.get(self.text)?;
        self.lines = layout(&text.content, self.mode, COLUMNS);
        self.scroll = self.scroll.min(self.max_scroll());
        Some(())
    }

    fn max_scroll(&self) -> usize {
        self.lines.len().saturating_sub(VISIBLE)
    }

    pub(super) fn handle_input(&mut self, event: InputEvent, catalog: &Catalog) {
        let max = self.max_scroll();
        match event {
            InputEvent::Up { .. } => self.scroll = self.scroll.saturating_sub(1),
            InputEvent::Down { .. } => self.scroll = (self.scroll + 1).min(max),
            InputEvent::PageUp { .. } => self.scroll = self.scroll.saturating_sub(VISIBLE),
            InputEvent::PageDown { .. } => self.scroll = (self.scroll + VISIBLE).min(max),
            InputEvent::Home { .. } | InputEvent::DocumentStart { .. } => self.scroll = 0,
            InputEvent::End { .. } | InputEvent::DocumentEnd { .. } => self.scroll = max,
            InputEvent::Tab { reverse } => self.step_text(catalog, reverse),
            InputEvent::Text('r' | 'R') => self.toggle_mode(catalog),
            _ => {}
        }
    }

    fn step_text(&mut self, catalog: &Catalog, reverse: bool) {
        let count = catalog.texts_of(self.entry).len();
        if count < 2 {
            return;
        }
        self.text = if reverse {
            (self.text + count - 1) % count
        } else {
            (self.text + 1) % count
        };
        self.scroll = 0;
        self.relayout(catalog);
    }

    /// Switches between wrapping and reflowing, keeping the reader at the
    /// same relative place in the text.
    fn toggle_mode(&mut self, catalog: &Catalog) {
        let before = (self.scroll, self.lines.len().max(1));
        self.mode = match self.mode {
            Mode::Wrap => Mode::Reflow,
            Mode::Reflow => Mode::Wrap,
        };
        self.scroll = 0;
        self.relayout(catalog);
        self.scroll = (before.0 * self.lines.len() / before.1).min(self.max_scroll());
    }

    pub(super) fn render(&self, p: &mut Painter<'_>, catalog: &Catalog) {
        let entry = &catalog.entries()[self.entry];
        let texts = catalog.texts_of(self.entry);
        let width = WIDTH as i32;
        let cell = p.cell_w();
        p.fill(0, 0, width, ROW * 20, PAPER);
        p.fill(0, 0, width, ROW, INK);
        let heading = if entry.version.is_empty() {
            entry.name.clone()
        } else {
            format!("{} {}", entry.name, entry.version)
        };
        let mode = match self.mode {
            Mode::Wrap => ui_text::get("credits-reader-wrap"),
            Mode::Reflow => ui_text::get("credits-reader-reflow"),
        };
        let mode_width = p.width(&mode);
        let room = (width - cell * 3 - mode_width) / cell;
        p.text_clipped(cell, 0, &heading, room.max(0) as usize, PAPER);
        p.text_right(width - cell, 0, &mode, PAPER);

        p.fill(0, ROW + ROW - 2, width, 2, PAPER_HOVER);
        let total = self.lines.len().max(1);
        let seen = (self.scroll + VISIBLE).min(total);
        p.fill(0, ROW + ROW - 2, width * seen as i32 / total as i32, 2, INK);
        let pages = total.div_ceil(VISIBLE);
        let page = (self.scroll / VISIBLE + 1).min(pages);
        let position = ui_text::get_with(
            "credits-reader-position",
            &[
                ("page", Arg::Number(page as f64)),
                ("pages", Arg::Number(pages as f64)),
                ("percent", Arg::Number((seen * 100 / total) as f64)),
            ],
        );
        let title = texts.get(self.text).map_or("", |text| text.title.as_str());
        let label = ui_text::get_with(
            "credits-reader-text",
            &[
                ("index", Arg::Number((self.text + 1) as f64)),
                ("count", Arg::Number(texts.len() as f64)),
                ("title", Arg::Text(title)),
            ],
        );
        let room = (width - cell * 3 - p.width(&position)) / cell;
        p.text_clipped(cell, ROW, &label, room.max(0) as usize, INK);
        p.text_right(width - cell, ROW, &position, INK);

        for (row, line) in self
            .lines
            .iter()
            .skip(self.scroll)
            .take(VISIBLE)
            .enumerate()
        {
            let y = TEXT_TOP + row as i32 * ROW;
            if line.continued {
                p.fill(4, y + 3, 1, 4, PAPER_MUTE);
                p.fill(4, y + 6, 3, 1, PAPER_MUTE);
            }
            p.text(10, y, &line.text, INK);
        }
        p.scrollbar(
            (316, TEXT_TOP, ROW * VISIBLE as i32),
            (self.scroll, VISIBLE, self.lines.len()),
            (PAPER_HOVER, INK),
        );
        let hint = ui_text::get("credits-reader-hint");
        p.status_bar(&hint, "");
    }
}

#[cfg(test)]
mod tests;
