//! Drawing the Code view.

use std::ops::Range;

use glaux_i18n::Arg;
use glaux_input::Point;
use glaux_machine::font::BitmapFont;
use glaux_machine::framebuffer::Framebuffer;
use glaux_machine::system_rom::{SystemIcon, SystemUi};
use glaux_syntax::{Span, SymbolKind, SyntaxKind};
use glaux_video::draw::{draw_glyph, draw_ink_image, draw_text, fill_rect, pixel};

use super::layout::{
    CHEVRON_WIDTH, CLOSE_SIZE, CodeLayout, LABEL_PAD, SETTINGS_MARGIN, StripHit, shorten_start,
};
mod debug_panel;

use super::picker::FilePicker;
use super::symbols::{PAD, PanelLayout, SymbolPanel, clip_name, kind_letter};
use super::text_view::{TextGeometry, thumb};
use super::{CodeEditor, labels, picker_layout, settings_layout};
use crate::cartridge::Language;
use crate::system::editing::document::{Breakpoints, CodeDocument};
use crate::system::editing::metrics::{Spot, TextMetrics, spot, visual_rows};
use crate::system::editing::session::{CodeSession, DocumentView};
use crate::system::editor_settings::EditorSettings;
use crate::system::screen::{ACCENT, BACKGROUND, DIM, ERROR, OK, PALE, TEXT, WARN};
use crate::system::syntax_theme;
use crate::system::workspace::dialog::{BODY, INK, draw_window};
use crate::system::workspace::{BAR, HOVER_SHADE, IDLE_SHADE, OUTLINE, Rect, take_cells, wrap};
use crate::ui_text;

/// Side of the dot marking a tab whose document has unsaved edits.
const DIRTY_DOT: i32 = 3;
/// Marks a file OPEN FILE lists that already has a tab.
fn open_tag() -> String {
    ui_text::get("code-open-file-tag")
}
/// The ground of text the find bar matches, outside the selection.
const MATCH: u8 = HOVER_SHADE;

impl CodeEditor {
    /// The line of `document`, counted from 0, the selected frame of a
    /// stopped cartridge is at.
    fn stopped_line(&self, document: &CodeDocument) -> Option<usize> {
        let (path, line) = self.debug.as_ref()?.place()?;
        (path == document.path().as_str()).then(|| line.saturating_sub(1) as usize)
    }

    /// Draws the view for `session` below the workspace's tab bar, with
    /// what `pointer` is over highlighted. `notice`, when given, takes the
    /// status bar in its color.
    #[hotpath::measure(label = "code_editor::render", impl_type = "CodeEditor")]
    pub fn render(
        &self,
        framebuffer: &mut Framebuffer,
        font: &BitmapFont,
        ui: &SystemUi,
        session: Option<&CodeSession>,
        pointer: Option<Point>,
        notice: Option<(&str, u8)>,
    ) {
        let mut painter = Painter {
            framebuffer,
            font,
            layout: &self.layout,
            stopped: self.debug.is_some(),
        };
        // Nothing under an overlay reacts to the pointer.
        let hover_pointer = pointer.filter(|_| !self.has_overlay());
        let settings_lit = self.settings_dialog.is_some()
            || hover_pointer.is_some_and(|at| self.layout.settings_button().contains(at));
        let run_lit = hover_pointer.is_some_and(|at| self.layout.run_button().contains(at));
        let debug_lit = hover_pointer.is_some_and(|at| self.layout.debug_button().contains(at));
        match session {
            Some(session) => {
                painter.strip(session, hover_pointer, self.menu.is_some());
                match session.active_document() {
                    Some(document) => {
                        let id = document.id();
                        let view = session.tabs().view(id).copied().unwrap_or_default();
                        let metrics = session.tabs().metrics(id);
                        let selection = session
                            .tabs()
                            .selection(id)
                            .unwrap_or_default()
                            .reset(document);
                        let geometry = self.geometry(document);
                        let key = geometry.metrics_key();
                        let caret = spot(document, selection.caret().byte(), key, font);
                        let caret_byte = selection.caret().byte();
                        painter.options_bar(document.language(), document.path().as_str(), &caret);
                        // The caret is hidden while something else has the
                        // keys.
                        let lit = self.caret_lit() && !self.has_overlay() && !self.find_focused();
                        let matches = self
                            .find
                            .as_ref()
                            .map_or(&[][..], |bar| bar.matches_in(document));
                        painter.text_view(
                            document,
                            &geometry,
                            view,
                            metrics,
                            &self.settings,
                            lit.then_some((&caret, caret_byte)),
                            selection.range(),
                            matches,
                            session.syntax_spans(id),
                            Marks {
                                breakpoints: document.breakpoints(),
                                stopped_at: self.stopped_line(document),
                            },
                        );
                        if let (Some(panel), Some(layout)) = (&self.symbols, self.panel_layout()) {
                            painter.symbol_panel(panel, &layout, hover_pointer);
                        }
                        if let (Some(view), Some(layout)) = (&self.debug, self.debug_layout()) {
                            let grip = self.resizing
                                || hover_pointer.is_some_and(|at| layout.handle.contains(at));
                            painter.debug_panel(view, &layout, hover_pointer, grip);
                        }
                        if let (Some(popup), Some(layout)) =
                            (&self.signature, self.signature_layout(session))
                            && !self.has_overlay()
                        {
                            popup.render(
                                painter.framebuffer,
                                font,
                                self.layout.metrics.cell_w,
                                self.layout.metrics.cell_h,
                                &layout,
                            );
                        }
                        if let (Some(popup), Some(layout)) =
                            (&self.completion, self.completion_layout(session))
                            && !self.has_overlay()
                        {
                            popup.render(
                                painter.framebuffer,
                                font,
                                self.layout.metrics.cell_w,
                                &layout,
                                hover_pointer,
                            );
                        }
                        if let Some((layout, _)) = self.hover_layout(session)
                            && !self.has_overlay()
                        {
                            layout.render(painter.framebuffer, font, self.layout.metrics.cell_w);
                        }
                        if let (Some(bar), Some(layout)) = (&self.find, self.find_layout()) {
                            let current =
                                matches.iter().position(|found| *found == selection.range());
                            let lit = self.caret_lit() && !self.has_overlay();
                            bar.render(
                                painter.framebuffer,
                                font,
                                &layout,
                                matches.len(),
                                current,
                                lit,
                            );
                        }
                    }
                    None => {
                        painter.options_bar_empty();
                        painter.empty_state(self.entry_error.as_deref());
                        for (rect, label, _) in self.empty_actions() {
                            let lit = hover_pointer.is_some_and(|at| rect.contains(at));
                            painter.text(&label, rect.x, rect.y, if lit { TEXT } else { ACCENT });
                        }
                    }
                }
            }
            None => {
                painter.strip_background();
                painter.options_bar_empty();
                painter.empty_state(self.entry_error.as_deref());
            }
        }
        painter.run_buttons(run_lit, debug_lit, hover_pointer);
        painter.settings_button(ui, settings_lit);
        let hint = self.hint();
        let (status, color) = notice.unwrap_or((&hint, PALE));
        painter.status_bar(status, color);

        if let (Some(menu), Some(session)) = (self.menu, session) {
            let (window, rows) = self.menu_rows(session.tabs().len(), menu.focus);
            painter.menu(session, window, &rows, menu.focus);
        }
        if let Some(picker) = &self.picker {
            painter.picker(picker, session);
        }
        if let Some(menu) = &self.context_menu {
            menu.render(painter.framebuffer, font, &self.menu_layout(menu));
        }
        let lit = self.caret_lit();
        if let Some(dialog) = &self.new_file {
            let layout = self.prompt_layout(dialog.prompt());
            dialog.render(painter.framebuffer, font, &layout, lit);
        }
        if let Some(dialog) = &self.rename {
            let layout = self.prompt_layout(dialog.prompt());
            dialog.render(painter.framebuffer, font, &layout, lit);
        }
        if let Some(dialog) = &self.go_to {
            let layout = self.prompt_layout(dialog.prompt());
            dialog.render(painter.framebuffer, font, &layout, lit);
        }
        if let Some(dialog) = &self.settings_dialog {
            let layout = settings_layout(&self.layout);
            dialog.render(painter.framebuffer, font, &layout, &self.settings);
        }
    }
}

/// Draws into one frame with one layout.
/// What the margin marks beside the lines of the document drawn.
struct Marks<'a> {
    breakpoints: &'a Breakpoints,
    /// The line, counted from 0, a stopped cartridge is at in this document.
    stopped_at: Option<usize>,
}

struct Painter<'a> {
    framebuffer: &'a mut Framebuffer,
    font: &'a BitmapFont,
    layout: &'a CodeLayout,
    /// Whether a debugged cartridge is stopped, which replaces the run
    /// plates with the ones that go on with it.
    stopped: bool,
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
        let labels = labels(self.font, session);
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
        let ids: Vec<_> = session.tabs().ids().collect();
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
            let modified = ids
                .get(tab.index)
                .and_then(|&id| session.document(id))
                .is_some_and(CodeDocument::is_modified);
            // A modified tab shows a dot in the close glyph's place until
            // the pointer comes over the tab, so it can still be closed.
            if modified && !hovered {
                let inset = (CLOSE_SIZE - DIRTY_DOT) / 2;
                self.fill(
                    Rect {
                        x: tab.close.x + inset,
                        y: tab.close.y + inset,
                        width: DIRTY_DOT,
                        height: DIRTY_DOT,
                    },
                    ACCENT,
                );
            } else {
                self.close_glyph(tab.close, close_ink);
            }
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
            let x = chevron_x - 2 - self.layout.cells(&count) * self.cell_w();
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

    /// The active document's language and path, and the line and column
    /// of its caret at `caret`.
    fn options_bar(&mut self, language: Language, path: &str, caret: &Spot) {
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
            width: self.layout.cells(tag) * self.cell_w() + 4,
            height: bar.height - 3,
        };
        self.fill(tag_rect, BAR);
        self.text(tag, tag_rect.x + 2, y, PALE);

        let position = ui_text::get_with(
            "code-caret-position",
            &[
                ("line", Arg::Text(&(caret.line + 1).to_string())),
                ("column", Arg::Text(&(caret.column + 1).to_string())),
            ],
        );
        let controls_x = if self.stopped {
            self.layout.step_buttons()[0].x
        } else {
            self.layout.run_button().x
        };
        let position_x = controls_x - self.cell_w() - self.layout.cells(&position) * self.cell_w();
        let path_x = tag_rect.right() + self.cell_w();
        let room = (position_x - self.cell_w() - path_x) / self.cell_w().max(1);
        let path = shorten_start(self.font, path, usize::try_from(room).unwrap_or(0));
        self.text(&path, path_x, y, PALE);
        self.text(&position, position_x, y, PALE);
    }

    fn options_bar_empty(&mut self) {
        self.options_rule();
    }

    /// The plates that run and stop the cartridge, `run_lit` and
    /// `debug_lit` while the pointer is over them. Stop is drawn disabled:
    /// nothing runs while the editor is shown, unless a debugged cartridge
    /// is stopped, and then the plates that go on with it take their place.
    fn run_buttons(&mut self, run_lit: bool, debug_lit: bool, pointer: Option<Point>) {
        if self.stopped {
            self.step_buttons(pointer);
            return;
        }
        let run = self.layout.run_button();
        let stop = self.layout.stop_button();
        self.fill(run, if run_lit { HOVER_SHADE } else { BAR });
        self.fill(stop, BAR);
        self.play(run);
        self.bug(self.layout.debug_button(), debug_lit);
        fill_rect(self.framebuffer, stop.x + 2, stop.y + 2, 5, 5, IDLE_SHADE);
    }

    /// The cogwheel opening EDITOR SETTINGS, `lit` while hovered or open.
    fn settings_button(&mut self, ui: &SystemUi, lit: bool) {
        let button = self.layout.settings_button();
        let inks = if lit {
            [OUTLINE, HOVER_SHADE, PALE]
        } else {
            [OUTLINE, IDLE_SHADE, DIM]
        };
        // The icon's transparent margin runs past the bar and draws nothing.
        draw_ink_image(
            self.framebuffer,
            ui.icon(SystemIcon::Settings),
            button.x - SETTINGS_MARGIN,
            button.y - SETTINGS_MARGIN,
            &inks,
        );
    }

    fn options_rule(&mut self) {
        let bar = self.layout.options;
        fill_rect(self.framebuffer, bar.x, bar.bottom() - 1, bar.width, 1, BAR);
    }

    /// The rows of `document` that fit from `view`, with line numbers,
    /// scroll indicators, the caret at `caret` (its spot and byte) and the
    /// `selected` bytes as `settings` and `metrics` allow.
    ///
    /// Selected characters show inverted, each in its own cells however
    /// shaping joins it with its neighbours, and a selected line ending
    /// takes one cell after its line's last.
    ///
    /// Only the rows shown are laid out. `metrics` that don't describe this
    /// view leave the indicators and the caret out rather than being
    /// measured again here.
    /// The text of `document` in `geometry`, with `selected` inverted and
    /// each of `matches`, sorted and apart, on a highlight.
    #[expect(
        clippy::too_many_arguments,
        reason = "each is one part of the view drawn"
    )]
    fn text_view(
        &mut self,
        document: &CodeDocument,
        geometry: &TextGeometry,
        view: DocumentView,
        metrics: Option<&TextMetrics>,
        settings: &EditorSettings,
        caret: Option<(&Spot, usize)>,
        selected: Range<usize>,
        matches: &[Range<usize>],
        syntax: &[Span],
        marks: Marks<'_>,
    ) {
        let geometry = *geometry;
        let found = |byte: usize| {
            let at = matches.partition_point(|found| found.end <= byte);
            matches.get(at).is_some_and(|found| found.contains(&byte))
        };
        let syntax_color = |byte: usize| {
            let at = syntax.partition_point(|span| span.range.end <= byte);
            syntax
                .get(at)
                .filter(|span| span.range.contains(&byte))
                .map(|span| syntax_theme::color(span.kind))
        };
        let metrics = metrics.filter(|metrics| metrics.describes(document, geometry.metrics_key()));
        let view = metrics.map_or(view, |metrics| geometry.clamp(view, metrics));
        let (cell_w, cell_h) = (self.cell_w(), self.cell_h());
        if let Some(gutter) = geometry.gutter {
            fill_rect(
                self.framebuffer,
                gutter.right() - 1,
                gutter.y,
                1,
                gutter.height,
                BAR,
            );
        }
        let top_row = if geometry.wrap { view.top_row } else { 0 };
        let columns = geometry.metrics_key().columns(self.font);
        let rows = visual_rows(
            document,
            view.top_line,
            top_row,
            geometry.wrap_columns(),
            columns,
        );
        for (index, row) in rows.take(geometry.rows).enumerate() {
            let y = geometry.viewport.y + index as i32 * cell_h;
            // Folds of a wrapped line leave the gutter blank, so each
            // number marks where its source line starts.
            if marks.stopped_at == Some(row.line) {
                // The line the cartridge stopped on, across the whole row.
                let gutter_right = geometry.gutter.map_or(geometry.viewport.x, Rect::right);
                fill_rect(
                    self.framebuffer,
                    gutter_right,
                    y,
                    geometry.vertical.x - gutter_right,
                    cell_h,
                    BAR,
                );
            }
            if row.row_in_line == 0
                && let Some(gutter) = geometry.gutter
            {
                let color = self.line_number(row.line, y, &gutter, &marks);
                self.number(row.line + 1, geometry.number_right(), y, color);
            }
            let first = if geometry.wrap {
                row.start.column
            } else {
                view.left_column
            };
            let end = first + geometry.columns;
            let cell_rect = |column: usize, width: usize| -> Option<Rect> {
                // Columns past the viewport's edges are cut off.
                let left = column.max(first);
                let right = (column + width).min(end);
                (left < right).then(|| Rect {
                    x: geometry.viewport.x + (left - first) as i32 * cell_w,
                    y,
                    width: (right - left) as i32 * cell_w,
                    height: cell_h,
                })
            };
            // Shaping never changes which cells characters take, so each
            // glyph lands in its character's cell.
            let glyphs = self.font.shape(row.text, settings.features());
            let mut row_end = row.start.column;
            for (cell, glyph) in row.cells(columns).zip(glyphs) {
                row_end = cell.column + cell.width;
                if cell.column >= end {
                    continue;
                }
                let is_selected = selected.contains(&cell.byte);
                let ground = if is_selected {
                    Some(TEXT)
                } else if found(cell.byte) {
                    Some(MATCH)
                } else {
                    None
                };
                if let (Some(ground), Some(rect)) = (ground, cell_rect(cell.column, cell.width)) {
                    self.fill(rect, ground);
                }
                // Glyphs cut by either edge of the viewport are left out.
                if cell.column < first || cell.column + cell.width > end {
                    continue;
                }
                // Characters the font lacks show its fallback, dimmed, and so
                // do control characters other than tabs, so none of them
                // goes unseen.
                let own = self.font.glyph_id(cell.ch).is_some();
                let (ink, missing) = if is_selected {
                    (BACKGROUND, BACKGROUND)
                } else if found(cell.byte) {
                    // Syntax colors are not meant for the match highlight.
                    (TEXT, DIM)
                } else {
                    (syntax_color(cell.byte).unwrap_or(TEXT), DIM)
                };
                let glyph = match glyph {
                    Some(glyph) if own => Some((glyph, ink)),
                    _ if cell.ch == '\t' => None,
                    _ => self.font.fallback().map(|glyph| (glyph, missing)),
                };
                if let Some((glyph, color)) = glyph {
                    let x = geometry.viewport.x + (cell.column - first) as i32 * cell_w;
                    draw_glyph(self.framebuffer, self.font, glyph, x, y, color);
                }
            }
            let line_end = row.start.byte + row.text.len();
            if !row.folds
                && selected.contains(&line_end)
                && line_end < document.text().len()
                && let Some(rect) = cell_rect(row_end, 1)
            {
                self.fill(rect, TEXT);
            }
        }
        if let (Some(metrics), Some((caret, byte))) = (metrics, caret)
            && let Some((x, y)) = geometry.caret_point(view, caret, metrics)
        {
            // A line on the left edge of the cell the next character
            // takes, inverted where that cell is selected.
            let color = if selected.contains(&byte) {
                BACKGROUND
            } else {
                TEXT
            };
            self.fill(
                Rect {
                    x,
                    y,
                    width: 1,
                    height: cell_h,
                },
                color,
            );
        }
        if let Some(metrics) = metrics {
            let top = metrics.row_index(view.top_line, top_row);
            let track = geometry.vertical;
            if let Some((start, length)) = thumb(top, geometry.rows, metrics.rows(), track.height) {
                self.fill(track, BAR);
                self.fill(
                    Rect {
                        y: track.y + start,
                        height: length,
                        ..track
                    },
                    DIM,
                );
            }
            let track = geometry.horizontal;
            let sideways = thumb(
                view.left_column,
                geometry.columns,
                geometry.scroll_width(metrics),
                track.width,
            );
            if let Some((start, length)) = sideways.filter(|_| !geometry.wrap) {
                self.fill(track, BAR);
                self.fill(
                    Rect {
                        x: track.x + start,
                        width: length,
                        ..track
                    },
                    DIM,
                );
            }
        }
    }

    /// `n` in decimal, its last digit's cell ending at `right`.
    /// The symbol panel: its title, then the declarations from the one at `scroll`,
    /// the selected one on a bar, with the pointer's row lit.
    fn symbol_panel(&mut self, panel: &SymbolPanel, layout: &PanelLayout, pointer: Option<Point>) {
        let (cell_w, area) = (self.cell_w(), layout.area);
        self.fill(area, BACKGROUND);
        self.fill(Rect { width: 1, ..area }, BAR);
        let title = ui_text::get("code-symbols");
        let title_color = if panel.focused() { ACCENT } else { PALE };
        let y = self.bar_text_y(layout.title.y, layout.title.height - 1);
        self.text(&title, area.x + PAD, y, title_color);
        self.fill(
            Rect {
                y: layout.title.bottom() - 1,
                height: 1,
                ..area
            },
            BAR,
        );
        if panel.rows().is_empty() {
            let empty = ui_text::get("code-no-symbols");
            self.text(&empty, area.x + PAD, layout.first_row.y + 2, DIM);
            return;
        }
        let rows = panel.rows().iter().enumerate().skip(panel.scroll());
        for (n, (index, entry)) in rows.take(layout.visible).enumerate() {
            let symbol = &entry.symbol;
            let row = layout.row(n);
            let selected = index == panel.selected();
            let lit = pointer.is_some_and(|at| row.contains(at));
            if selected {
                self.fill(
                    Rect {
                        x: row.x + 1,
                        width: row.width - 1,
                        ..row
                    },
                    BAR,
                );
            } else if lit {
                self.fill(
                    Rect {
                        x: row.x + 1,
                        width: row.width - 1,
                        ..row
                    },
                    IDLE_SHADE,
                );
            }
            let text_y = row.y + (row.height - self.cell_h()) / 2;
            let kind_color = syntax_theme::color(match symbol.kind {
                SymbolKind::Function => SyntaxKind::Function,
                SymbolKind::Variable => SyntaxKind::Variable,
                SymbolKind::Constant => SyntaxKind::Constant,
            });
            let line = (entry.line + 1).to_string();
            let x = area.x + PAD;
            self.text(kind_letter(symbol.kind), x, text_y, kind_color);
            let line_x = area.right() - PAD - self.layout.cells(&line) * cell_w;
            self.text(&line, line_x, text_y, DIM);
            let room = (line_x - x) / cell_w.max(1) - 3;
            let name = clip_name(self.font, &symbol.name, usize::try_from(room).unwrap_or(0));
            let color = if selected { TEXT } else { PALE };
            self.text(&name, x + 2 * cell_w, text_y, color);
        }
    }

    /// Draws what the margin shows of `line` besides its number, and gives
    /// the color the number takes: a breakpoint turns it into white on red,
    /// and the line a cartridge stopped on gets an arrow.
    fn line_number(&mut self, line: usize, y: i32, gutter: &Rect, marks: &Marks<'_>) -> u8 {
        let cell_h = self.cell_h();
        if marks.stopped_at == Some(line) {
            // A right-pointing triangle in the space left of the numbers.
            for column in 0..3 {
                fill_rect(
                    self.framebuffer,
                    gutter.x + column,
                    y + (cell_h - 5) / 2 + column,
                    1,
                    5 - 2 * column,
                    WARN,
                );
            }
        }
        if marks.breakpoints.contains(line) {
            fill_rect(
                self.framebuffer,
                gutter.x + 1,
                y,
                gutter.width - 3,
                cell_h,
                ERROR,
            );
            return TEXT;
        }
        IDLE_SHADE
    }

    fn number(&mut self, n: usize, right: i32, y: i32, color: u8) {
        let mut digits = [0; 20];
        let mut start = digits.len();
        let mut rest = n;
        loop {
            start -= 1;
            digits[start] = b'0' + (rest % 10) as u8;
            rest /= 10;
            if rest == 0 {
                break;
            }
        }
        let text = std::str::from_utf8(&digits[start..]).unwrap_or_default();
        self.text(
            text,
            right - self.layout.cells(text) * self.cell_w(),
            y,
            color,
        );
    }

    /// What the view shows without an open document.
    fn empty_state(&mut self, entry_error: Option<&str>) {
        let area = self.layout.text;
        let x = area.x + self.cell_w();
        let mut y = area.y + self.cell_h();
        self.text(&ui_text::get("code-no-file-open"), x, y, TEXT);
        // The links to create and open files take the next two rows. See
        // `CodeEditor::empty_actions`.
        y += 2 * self.cell_h();
        if let Some(error) = entry_error {
            y += 2 * self.cell_h();
            self.text(
                &ui_text::get("code-the-entry-script-could-not-be-opened"),
                x,
                y,
                TEXT,
            );
            for line in wrap(
                self.font,
                error,
                self.layout.text_columns().saturating_sub(2),
            ) {
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
        let text = take_cells(self.font, text, columns);
        self.text(text, 4, y, color);
    }

    /// The list of every open tab, `focus` highlighted.
    fn menu(&mut self, session: &CodeSession, window: Rect, rows: &[(usize, Rect)], focus: usize) {
        let labels = labels(self.font, session);
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
            &ui_text::get("code-open-file-2"),
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

        let tag_width = self.layout.cells(&open_tag()) * cell_w;
        let marker = 2 * cell_w;
        for (row, path) in picker
            .matches()
            .skip(picker.first())
            .take(layout.rows)
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
                let marker_y = rect.y + (rect.height - 3) / 2;
                fill_rect(self.framebuffer, rect.x + 2, marker_y, 3, 3, ink);
                let tag = Rect {
                    x: rect.right() - 4 - tag_width,
                    y: rect.y,
                    width: tag_width + 4,
                    height: rect.height,
                };
                self.fill(tag, ink);
                self.text(&open_tag(), tag.x + 2, rect.y + 1, ground);
            }
            let room = (rect.width - marker - tag_width - 4 - cell_w) / cell_w.max(1);
            let label = shorten_start(self.font, path.as_str(), usize::try_from(room).unwrap_or(0));
            self.text(&label, rect.x + marker, rect.y + 1, ink);
        }

        let total = picker.paths().len();
        let summary = match picker.error() {
            Some(error) => {
                ui_text::get_with("code-cannot-list-scripts", &[("error", Arg::Text(error))])
            }
            None if total == 0 => ui_text::get("code-no-luau-or-rhai-files-in-this-cartridge"),
            None if picker.match_count() == 0 => ui_text::get("code-no-file-matches"),
            None => ui_text::get_with(
                "code-picker-file-count",
                &[
                    ("matches", Arg::Text(&(picker.match_count().to_string()))),
                    ("total", Arg::Text(&total.to_string())),
                ],
            ),
        };
        let columns = usize::try_from(layout.list.width / cell_w.max(1)).unwrap_or(0);
        let mut y = layout.summary_y;
        let font = self.font;
        let clip = |text: &str| take_cells(font, text, columns).to_owned();
        self.text(&clip(&summary), layout.text_x, y, INK);
        for line in picker.gap_lines() {
            y += self.cell_h();
            self.text(&clip(&format!("! {line}")), layout.text_x, y, ERROR);
        }
    }
}

#[cfg(test)]
mod tests;
