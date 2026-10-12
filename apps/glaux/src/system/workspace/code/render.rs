//! Drawing the Code view.

use glaux_input::Point;
use glaux_machine::font::BitmapFont;
use glaux_machine::framebuffer::Framebuffer;
use glaux_video::draw::{draw_text, fill_rect, pixel};

use super::layout::{
    CHEVRON_WIDTH, CLOSE_SIZE, CodeLayout, LABEL_PAD, StripHit, cells, shorten_start,
};
use super::picker::{FilePicker, LIST_ROWS};
use super::{CodeEditor, labels, picker_layout};
use crate::cartridge::Language;
use crate::system::code_session::CodeSession;
use crate::system::workspace::dialog::{BODY, INK, draw_window};
use crate::system::workspace::{BAR, HOVER_SHADE, IDLE_SHADE, Rect, wrap};
use crate::system::{ACCENT, BACKGROUND, DIM, ERROR, PALE, TEXT};

/// Columns a tab character advances to the next multiple of.
const TAB_STOP: usize = 4;
/// Shown right of the path of every document, none being editable yet.
const READ_ONLY: &str = "Read-only";
/// Marks a file OPEN FILE lists that already has a tab.
const OPEN_TAG: &str = "open";

impl CodeEditor {
    /// Draws the view for `session` below the workspace's tab bar, with
    /// what `pointer` is over highlighted. `notice`, when given, takes the
    /// status bar in its color.
    pub fn render(
        &self,
        framebuffer: &mut Framebuffer,
        font: &BitmapFont,
        session: Option<&CodeSession>,
        pointer: Option<Point>,
        notice: Option<(&str, u8)>,
    ) {
        let mut painter = Painter {
            framebuffer,
            font,
            layout: &self.layout,
        };
        // Nothing under an overlay reacts to the pointer.
        let hover_pointer = pointer.filter(|_| !self.has_overlay());
        match session {
            Some(session) => {
                painter.strip(session, hover_pointer, self.menu.is_some());
                match session.active_document() {
                    Some(document) => {
                        painter.options_bar(document.language(), document.path().as_str());
                        let top = session
                            .tabs()
                            .view(document.id())
                            .map_or(0, |view| view.top_line);
                        painter.text_view(document.text(), top);
                    }
                    None => {
                        painter.options_bar_empty();
                        painter.empty_state(self.entry_error.as_deref());
                    }
                }
            }
            None => {
                painter.strip_background();
                painter.options_bar_empty();
                painter.empty_state(self.entry_error.as_deref());
            }
        }
        let (status, color) = notice.unwrap_or((self.hint(), PALE));
        painter.status_bar(status, color);

        if let (Some(menu), Some(session)) = (self.menu, session) {
            let (window, rows) = self.menu_rows(session.tabs().len(), menu.focus);
            painter.menu(session, window, &rows, menu.focus);
        }
        if let Some(picker) = &self.picker {
            painter.picker(picker, session);
        }
    }
}

/// Draws into one frame with one layout.
struct Painter<'a> {
    framebuffer: &'a mut Framebuffer,
    font: &'a BitmapFont,
    layout: &'a CodeLayout,
}

impl Painter<'_> {
    fn text(&mut self, text: &str, x: i32, y: i32, color: u8) {
        draw_text(self.framebuffer, self.font, text, x, y, color);
    }

    fn fill(&mut self, rect: Rect, color: u8) {
        fill_rect(
            self.framebuffer,
            rect.x,
            rect.y,
            rect.width,
            rect.height,
            color,
        );
    }

    fn cell_w(&self) -> i32 {
        self.layout.metrics.cell_w
    }

    fn cell_h(&self) -> i32 {
        self.layout.metrics.cell_h
    }

    /// Top of a text row centred in a bar that starts at `y`.
    fn bar_text_y(&self, y: i32, height: i32) -> i32 {
        y + (height - self.cell_h()) / 2 + 1
    }

    /// The strip's ground and the rule under it.
    fn strip_background(&mut self) {
        let strip = self.layout.strip;
        self.fill(strip, BACKGROUND);
        fill_rect(
            self.framebuffer,
            strip.x,
            strip.bottom() - 1,
            strip.width,
            1,
            BAR,
        );
    }

    /// The document tabs and the button listing them all.
    fn strip(&mut self, session: &CodeSession, pointer: Option<Point>, menu_open: bool) {
        self.strip_background();
        let labels = labels(session);
        if labels.is_empty() {
            return;
        }
        let scroll = session.tabs().scroll();
        let hit = pointer.map_or(StripHit::None, |at| {
            self.layout.strip_hit(&labels, scroll, at)
        });
        let active = session
            .tabs()
            .active()
            .and_then(|id| session.tabs().index_of(id));
        let tabs = self.layout.strip_tabs(&labels, scroll);
        for tab in &tabs {
            let is_active = active == Some(tab.index);
            let hovered = matches!(hit, StripHit::Tab(i) | StripHit::Close(i) if i == tab.index);
            self.fill(tab.rect, BAR);
            let ink = match (is_active, hovered) {
                (true, _) => TEXT,
                (false, true) => PALE,
                (false, false) => DIM,
            };
            let y = tab.rect.y + 1;
            self.text(&labels[tab.index], tab.rect.x + LABEL_PAD, y, ink);
            let close_ink = match (hit, is_active || hovered) {
                (StripHit::Close(i), _) if i == tab.index => ERROR,
                (_, true) => DIM,
                (_, false) => IDLE_SHADE,
            };
            self.close_glyph(tab.close, close_ink);
            if is_active {
                fill_rect(
                    self.framebuffer,
                    tab.rect.x,
                    tab.rect.bottom() - 1,
                    tab.rect.width,
                    1,
                    ACCENT,
                );
            }
        }

        let more = self.layout.more_button();
        let hovered = hit == StripHit::More;
        if menu_open || hovered {
            self.fill(more, BAR);
        }
        let ink = if menu_open || hovered { PALE } else { DIM };
        let chevron_x = more.right() - 2 - CHEVRON_WIDTH;
        let hidden = labels.len() - tabs.len();
        if hidden > 0 {
            let count = format!("+{hidden}");
            let x = chevron_x - 2 - cells(&count) * self.cell_w();
            self.text(&count, x, more.y + 1, ink);
        }
        self.chevron(chevron_x, more.y + (more.height - 3) / 2, ink);
    }

    /// A diagonal cross filling `rect`.
    fn close_glyph(&mut self, rect: Rect, color: u8) {
        for i in 0..CLOSE_SIZE {
            pixel(self.framebuffer, rect.x + i, rect.y + i, color);
            pixel(
                self.framebuffer,
                rect.x + CLOSE_SIZE - 1 - i,
                rect.y + i,
                color,
            );
        }
    }

    /// A downward chevron three pixels tall with its top-left at `(x, y)`.
    fn chevron(&mut self, x: i32, y: i32, color: u8) {
        for row in 0..3 {
            let width = CHEVRON_WIDTH - 2 * row;
            fill_rect(self.framebuffer, x + row, y + row, width, 1, color);
        }
    }

    /// The active document's language and path.
    fn options_bar(&mut self, language: Language, path: &str) {
        let bar = self.layout.options;
        self.options_rule();
        let y = self.bar_text_y(bar.y, bar.height - 1);
        let tag = match language {
            Language::Luau => "Luau",
            Language::Rhai => "Rhai",
        };
        let tag_rect = Rect {
            x: 4,
            y: bar.y + 1,
            width: cells(tag) * self.cell_w() + 4,
            height: bar.height - 3,
        };
        self.fill(tag_rect, BAR);
        self.text(tag, tag_rect.x + 2, y, PALE);

        let read_only_x = bar.right() - 4 - cells(READ_ONLY) * self.cell_w();
        let path_x = tag_rect.right() + self.cell_w();
        let room = (read_only_x - self.cell_w() - path_x) / self.cell_w().max(1);
        let path = shorten_start(path, usize::try_from(room).unwrap_or(0));
        self.text(&path, path_x, y, PALE);
        self.text(READ_ONLY, read_only_x, y, DIM);
    }

    fn options_bar_empty(&mut self) {
        self.options_rule();
    }

    fn options_rule(&mut self) {
        let bar = self.layout.options;
        fill_rect(self.framebuffer, bar.x, bar.bottom() - 1, bar.width, 1, BAR);
    }

    /// `text` from line `top`, as many lines and columns as fit.
    fn text_view(&mut self, text: &str, top: usize) {
        let area = self.layout.text;
        let columns = self.layout.text_columns();
        let rows = self.layout.text_rows();
        for (row, line) in text.split('\n').skip(top).take(rows).enumerate() {
            let line = expand_tabs(line.strip_suffix('\r').unwrap_or(line), columns);
            let y = area.y + 1 + row as i32 * self.cell_h();
            self.text(&line, area.x + 4, y, TEXT);
        }
    }

    /// What the view shows without an open document.
    fn empty_state(&mut self, entry_error: Option<&str>) {
        let area = self.layout.text;
        let x = area.x + self.cell_w();
        let mut y = area.y + self.cell_h();
        self.text("No file open.", x, y, TEXT);
        y += self.cell_h();
        self.text("Ctrl+P: open a script", x, y, DIM);
        if let Some(error) = entry_error {
            y += 2 * self.cell_h();
            self.text("The entry script could not be opened:", x, y, TEXT);
            for line in wrap(error, self.layout.text_columns().saturating_sub(2)) {
                y += self.cell_h();
                self.text(&line, x, y, ERROR);
            }
        }
    }

    fn status_bar(&mut self, text: &str, color: u8) {
        let bar = self.layout.status;
        self.fill(bar, BAR);
        let y = self.bar_text_y(bar.y, bar.height);
        let columns = usize::try_from((bar.width - 8) / self.cell_w().max(1)).unwrap_or(0);
        let text: String = text.chars().take(columns).collect();
        self.text(&text, 4, y, color);
    }

    /// The list of every open tab, `focus` highlighted.
    fn menu(&mut self, session: &CodeSession, window: Rect, rows: &[(usize, Rect)], focus: usize) {
        let labels = labels(session);
        let active = session
            .tabs()
            .active()
            .and_then(|id| session.tabs().index_of(id));
        self.fill(window, DIM);
        self.fill(window.inset(1), BAR);
        for &(index, rect) in rows {
            let focused = index == focus;
            if focused {
                self.fill(rect, HOVER_SHADE);
            }
            if active == Some(index) {
                self.text(">", rect.x + 2, rect.y + 1, ACCENT);
            }
            let ink = if focused { TEXT } else { PALE };
            self.text(
                &labels[index],
                rect.x + 2 + 2 * self.cell_w(),
                rect.y + 1,
                ink,
            );
        }
    }

    /// OPEN FILE, marking files that already have a tab.
    fn picker(&mut self, picker: &FilePicker, session: Option<&CodeSession>) {
        let layout = picker_layout(self.layout, picker);
        let cell_w = self.cell_w();
        draw_window(
            self.framebuffer,
            self.font,
            layout.window,
            layout.title_bar,
            "OPEN FILE",
            cell_w,
        );
        let filter = format!("> {}_", picker.filter());
        self.text(&filter, layout.filter.x, layout.filter.y, INK);
        let rule_y = layout.filter.bottom() + self.cell_h() / 2;
        fill_rect(
            self.framebuffer,
            layout.filter.x,
            rule_y,
            layout.filter.width,
            1,
            INK,
        );

        let tag_width = cells(OPEN_TAG) * cell_w;
        let marker = 2 * cell_w;
        for (row, path) in picker
            .matches()
            .skip(picker.first())
            .take(LIST_ROWS)
            .enumerate()
        {
            let rect = layout.row(row);
            let focused = picker.first() + row == picker.focus();
            let (ground, ink) = if focused { (INK, BODY) } else { (BODY, INK) };
            self.fill(rect, ground);
            let open = session.is_some_and(|session| session.is_open(path));
            if open {
                // Inverted against the row, so it reads on the light body
                // and on the highlight alike.
                fill_rect(self.framebuffer, rect.x + 2, rect.y + 3, 3, 3, ink);
                let tag = Rect {
                    x: rect.right() - 4 - tag_width,
                    y: rect.y,
                    width: tag_width + 4,
                    height: rect.height,
                };
                self.fill(tag, ink);
                self.text(OPEN_TAG, tag.x + 2, rect.y + 1, ground);
            }
            let room = (rect.width - marker - tag_width - 4 - cell_w) / cell_w.max(1);
            let label = shorten_start(path.as_str(), usize::try_from(room).unwrap_or(0));
            self.text(&label, rect.x + marker, rect.y + 1, ink);
        }

        let total = picker.paths().len();
        let summary = match picker.error() {
            Some(error) => format!("Cannot list scripts: {error}"),
            None if total == 0 => "No .luau or .rhai files in this cartridge.".to_owned(),
            None if picker.match_count() == 0 => "No file matches.".to_owned(),
            None => format!("{} of {total} files", picker.match_count()),
        };
        let columns = usize::try_from(layout.list.width / cell_w.max(1)).unwrap_or(0);
        let mut y = layout.summary_y;
        let clip = |text: &str| text.chars().take(columns).collect::<String>();
        self.text(&clip(&summary), layout.text_x, y, INK);
        for line in picker.gap_lines() {
            y += self.cell_h();
            self.text(&clip(&format!("! {line}")), layout.text_x, y, ERROR);
        }
    }
}

/// `line` with tabs expanded to [`TAB_STOP`] columns, cut to `columns`.
fn expand_tabs(line: &str, columns: usize) -> String {
    let mut out = String::new();
    let mut column = 0;
    for ch in line.chars() {
        if column >= columns {
            break;
        }
        if ch == '\t' {
            let next = (column / TAB_STOP + 1) * TAB_STOP;
            while column < next.min(columns) {
                out.push(' ');
                column += 1;
            }
        } else {
            out.push(ch);
            column += 1;
        }
    }
    out
}
