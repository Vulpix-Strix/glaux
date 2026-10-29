//! The overlay that shows the [`LogStore`] over the lower half of the screen.
//!
//! The panel builds its rows from the store when it opens and holds none
//! while closed.

use std::cell::Cell;
use std::rc::Rc;

use glaux_i18n::Arg;
use glaux_input::{Button, InputEvent, Key};
use glaux_machine::font::BitmapFont;
use glaux_machine::framebuffer::{Framebuffer, HEIGHT, WIDTH};
use glaux_video::draw::{draw_text, fill_rect};

use super::{LogRecord, LogStore, Severity, Source, SourceLocation};
use crate::system::screen::{ACCENT, BACKGROUND, DIM, ERROR, PALE, TEXT, TRACE, WARN};
use crate::system::text::{clip, cut, wrap};
use crate::system::workspace::{take_cells, thumb};
use crate::ui_text;

/// Rows moved by a page key.
const PAGE_ROWS: usize = 6;
/// Rows of the selected record's full text under the list.
const DETAIL_ROWS: usize = 2;

/// Restricts the listing to one author.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum SourceFilter {
    #[default]
    All,
    Cartridge,
    System,
}

impl SourceFilter {
    fn next(self) -> Self {
        match self {
            Self::All => Self::Cartridge,
            Self::Cartridge => Self::System,
            Self::System => Self::All,
        }
    }

    fn admits(self, source: Source) -> bool {
        match self {
            Self::All => true,
            Self::Cartridge => source == Source::Cartridge,
            Self::System => source == Source::System,
        }
    }

    fn label(self) -> String {
        match self {
            Self::All => ui_text::get("logs-all"),
            Self::Cartridge => ui_text::get("logs-cartridge"),
            Self::System => ui_text::get("logs-system"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PanelAction {
    None,
    /// Show this line of the cartridge's scripts in the editor.
    Jump(SourceLocation),
}

/// The rows for one store revision and one pair of filters, rebuilt when
/// either changes.
#[derive(Debug, Default)]
pub(super) struct Listing {
    /// `None` until the first build.
    pub(super) revision: Option<u64>,
    min: Option<Severity>,
    source: SourceFilter,
    pub(super) rows: Vec<Rc<LogRecord>>,
}

#[derive(Debug)]
pub struct LogPanel {
    open: bool,
    /// Records below this severity are hidden.
    min: Severity,
    source: SourceFilter,
    /// The `seq` of the record the user moved to, or `None` while the
    /// panel follows the newest record.
    selected: Option<u64>,
    /// How far the selected record's text is scrolled, while the user reads
    /// it whole instead of moving between records.
    pub(super) reading: Option<usize>,
    /// The furthest `reading` can reach, as the last drawing measured it.
    reach: Cell<usize>,
    pub(super) listing: Listing,
}

impl Default for LogPanel {
    fn default() -> Self {
        Self {
            open: false,
            min: Severity::Trace,
            source: SourceFilter::All,
            selected: None,
            reading: None,
            reach: Cell::new(0),
            listing: Listing::default(),
        }
    }
}

impl LogPanel {
    pub fn is_open(&self) -> bool {
        self.open
    }

    pub fn toggle(&mut self) {
        self.open = !self.open;
        self.reading = None;
        if self.open {
            self.selected = None;
        }
    }

    pub fn close(&mut self) {
        self.open = false;
    }

    /// Rebuilds the rows if the store or a filter changed. Does nothing while
    /// the panel is closed.
    pub fn sync(&mut self, store: &LogStore) {
        if !self.open {
            return;
        }
        let listing = &self.listing;
        if listing.revision == Some(store.revision())
            && listing.min == Some(self.min)
            && listing.source == self.source
        {
            return;
        }
        self.listing = Listing {
            revision: Some(store.revision()),
            min: Some(self.min),
            source: self.source,
            rows: store
                .iter()
                .filter(|record| record.severity >= self.min && self.source.admits(record.source))
                .cloned()
                .collect(),
        };
    }

    /// Applies one input. `can_jump` is false when there is no editor to
    /// show a script in, which makes Enter do nothing.
    ///
    /// Over a running cartridge the host delivers keys as game button and
    /// key events, and over the editor as editing events, so both are read.
    pub fn input(&mut self, event: InputEvent, store: &LogStore, can_jump: bool) -> PanelAction {
        self.sync(store);
        if let Some(top) = self.reading {
            let top = match event {
                InputEvent::ToggleLogs => {
                    self.close();
                    return PanelAction::None;
                }
                InputEvent::Break | InputEvent::Tab { .. } | InputEvent::KeyDown(Key::Tab) => {
                    self.reading = None;
                    return PanelAction::None;
                }
                InputEvent::ButtonDown(Button::Up) | InputEvent::Up { .. } => top.saturating_sub(1),
                InputEvent::ButtonDown(Button::Down) | InputEvent::Down { .. } => top + 1,
                InputEvent::KeyDown(Key::PageUp) | InputEvent::PageUp { .. } => {
                    top.saturating_sub(PAGE_ROWS)
                }
                InputEvent::KeyDown(Key::PageDown) | InputEvent::PageDown { .. } => top + PAGE_ROWS,
                InputEvent::KeyDown(Key::Home) | InputEvent::Home { .. } => 0,
                InputEvent::KeyDown(Key::End) | InputEvent::End { .. } => usize::MAX,
                _ => top,
            };
            self.reading = Some(top.min(self.reach.get()));
            return PanelAction::None;
        }
        match event {
            InputEvent::Break | InputEvent::ToggleLogs => self.close(),
            InputEvent::KeyDown(Key::Tab) | InputEvent::Tab { .. } => {
                if self.selected_record().is_some() {
                    self.reading = Some(0);
                }
            }
            InputEvent::ButtonDown(Button::Up) | InputEvent::Up { .. } => self.step(-1),
            InputEvent::ButtonDown(Button::Down) | InputEvent::Down { .. } => self.step(1),
            InputEvent::KeyDown(Key::PageUp) | InputEvent::PageUp { .. } => {
                self.step(-(PAGE_ROWS as isize));
            }
            InputEvent::KeyDown(Key::PageDown) | InputEvent::PageDown { .. } => {
                self.step(PAGE_ROWS as isize);
            }
            InputEvent::KeyDown(Key::Home) | InputEvent::Home { .. } => self.select_at(0),
            InputEvent::KeyDown(Key::End) | InputEvent::End { .. } => self.selected = None,
            InputEvent::KeyDown(Key::L) | InputEvent::Text('l' | 'L') => {
                self.min = match self.min {
                    Severity::Trace => Severity::Debug,
                    Severity::Debug => Severity::Info,
                    Severity::Info => Severity::Warn,
                    Severity::Warn => Severity::Error,
                    Severity::Error => Severity::Trace,
                };
                self.refilter(store);
            }
            InputEvent::KeyDown(Key::S) | InputEvent::Text('s' | 'S') => {
                self.source = self.source.next();
                self.refilter(store);
            }
            InputEvent::KeyDown(Key::Enter) | InputEvent::Enter if can_jump => {
                if let Some(location) = self.selected_record().and_then(|r| r.location.clone()) {
                    return PanelAction::Jump(location);
                }
            }
            _ => {}
        }
        PanelAction::None
    }

    pub fn jump_target(&self) -> Option<&SourceLocation> {
        self.selected_record()
            .and_then(|record| record.location.as_ref())
    }

    fn refilter(&mut self, store: &LogStore) {
        self.listing.revision = None;
        self.sync(store);
        if self
            .selected
            .is_some_and(|seq| self.position(seq).is_none())
        {
            self.selected = None;
        }
    }

    fn position(&self, seq: u64) -> Option<usize> {
        let at = self.listing.rows.partition_point(|row| row.seq < seq);
        (self.listing.rows.get(at)?.seq == seq).then_some(at)
    }

    /// The highlighted row: the selected record, or the newest while
    /// following.
    fn highlighted(&self) -> Option<usize> {
        match self.selected {
            Some(seq) => self.position(seq),
            None => self.listing.rows.len().checked_sub(1),
        }
    }

    pub(super) fn selected_record(&self) -> Option<&Rc<LogRecord>> {
        self.listing.rows.get(self.highlighted()?)
    }

    fn select_at(&mut self, at: usize) {
        self.selected = self.listing.rows.get(at).map(|row| row.seq);
    }

    /// Moving past the newest record goes back to following.
    fn step(&mut self, by: isize) {
        let Some(current) = self.highlighted() else {
            return;
        };
        let last = self.listing.rows.len() - 1;
        let target = current.saturating_add_signed(by);
        if target >= last && by > 0 {
            self.selected = None;
        } else {
            self.select_at(target.min(last));
        }
    }

    /// Draws the panel over `framebuffer`. `color` maps a system palette
    /// index to the one the frame is presented with, which differs over a
    /// cartridge's own palette.
    pub fn render(
        &self,
        framebuffer: &mut Framebuffer,
        font: &BitmapFont,
        color: &dyn Fn(u8) -> u8,
        can_jump: bool,
    ) {
        let (cell_w, cell_h) = (
            i32::from(font.metrics().cell_width()),
            i32::from(font.metrics().cell_height()),
        );
        let top = HEIGHT as i32 / 2;
        let width = WIDTH as i32;
        fill_rect(
            framebuffer,
            0,
            top,
            width,
            HEIGHT as i32 - top,
            color(BACKGROUND),
        );
        fill_rect(framebuffer, 0, top, width, 1, color(DIM));
        let columns = usize::try_from((width - 2 * cell_w) / cell_w.max(1)).unwrap_or(1);
        let x = cell_w;
        let rows_total = usize::try_from((HEIGHT as i32 - top - 2) / cell_h.max(1)).unwrap_or(0);
        let list_rows = rows_total.saturating_sub(2 + DETAIL_ROWS);

        let highlighted = self.highlighted();
        let header = ui_text::get_with(
            "logs-header",
            &[
                ("level", Arg::Text(severity_name(self.min))),
                ("source", Arg::Text(&self.source.label())),
                (
                    "count",
                    Arg::Text(&format!(
                        "{}/{}",
                        highlighted.map_or(0, |at| at + 1),
                        self.listing.rows.len()
                    )),
                ),
            ],
        );
        let y0 = top + 2;
        let line = |n: usize| y0 + n as i32 * cell_h;
        draw_text(
            framebuffer,
            font,
            take_cells(font, &header, columns),
            x,
            line(0),
            color(ACCENT),
        );

        if let (Some(top), Some(record)) = (self.reading, self.selected_record()) {
            let rows = rows_total.saturating_sub(2);
            let lines = wrap(font, &record.text(), columns.saturating_sub(1));
            self.reach.set(lines.len().saturating_sub(rows));
            let top = top.min(self.reach.get());
            for (n, part) in lines.iter().skip(top).take(rows).enumerate() {
                let ink = color(severity_color(record.severity));
                draw_text(framebuffer, font, part, x, line(1 + n), ink);
            }
            let track = rows as i32 * cell_h;
            if let Some((start, length)) = thumb(top, rows, lines.len(), track) {
                fill_rect(framebuffer, width - 3, line(1), 2, track, color(DIM));
                fill_rect(
                    framebuffer,
                    width - 3,
                    line(1) + start,
                    2,
                    length,
                    color(PALE),
                );
            }
            let footer = ui_text::get("logs-up-down-scroll-tab-back");
            let footer = clip(font, &footer, columns);
            draw_text(
                framebuffer,
                font,
                &footer,
                x,
                line(rows_total - 1),
                color(DIM),
            );
            return;
        }

        // While following, the highlighted row is the newest and the list
        // ends there.
        let end = highlighted
            .map_or(0, |at| at + 1)
            .max(list_rows.min(self.listing.rows.len()));
        let start = end.saturating_sub(list_rows);
        for (n, row) in self.listing.rows[start..end].iter().enumerate() {
            let y = line(1 + n);
            let on = Some(start + n) == highlighted;
            if on {
                fill_rect(framebuffer, 0, y, width, cell_h, color(DIM));
            }
            let tag = match row.source {
                Source::Cartridge => "C",
                Source::System => "S",
            };
            let tag_color = match row.source {
                Source::Cartridge => PALE,
                Source::System => ACCENT,
            };
            draw_text(framebuffer, font, tag, x, y, color(tag_color));
            let text = row.text();
            let first = text.lines().next().unwrap_or_default();
            let shown = clip(font, first, columns.saturating_sub(2));
            draw_text(
                framebuffer,
                font,
                &shown,
                x + 2 * cell_w,
                y,
                color(severity_color(row.severity)),
            );
        }
        if self.listing.rows.is_empty() {
            draw_text(
                framebuffer,
                font,
                &ui_text::get("logs-nothing-logged"),
                x,
                line(1),
                color(DIM),
            );
        }

        if let Some(record) = self.selected_record() {
            let text = record.text();
            let lines = wrap(font, &text, columns);
            for (n, part) in lines.iter().take(DETAIL_ROWS).enumerate() {
                // The last row says there is more to read with Tab.
                let part = if n + 1 == DETAIL_ROWS && lines.len() > DETAIL_ROWS {
                    cut(font, part, columns)
                } else {
                    part.clone()
                };
                draw_text(
                    framebuffer,
                    font,
                    &part,
                    x,
                    line(1 + list_rows + n),
                    color(severity_color(record.severity)),
                );
            }
        }

        let footer = match self.jump_target().filter(|_| can_jump) {
            Some(location) => ui_text::get_with(
                "logs-enter-open",
                &[
                    ("path", Arg::Text(location.path.as_str())),
                    ("line", Arg::Text(&location.line.to_string())),
                ],
            ),
            None => ui_text::get("logs-tab-read-l-level-s-source-f12-close"),
        };
        draw_text(
            framebuffer,
            font,
            take_cells(font, &footer, columns),
            x,
            line(rows_total - 1),
            color(DIM),
        );
    }
}

fn severity_color(severity: Severity) -> u8 {
    match severity {
        Severity::Trace => TRACE,
        Severity::Debug => DIM,
        Severity::Info => TEXT,
        Severity::Warn => WARN,
        Severity::Error => ERROR,
    }
}

fn severity_name(severity: Severity) -> &'static str {
    match severity {
        Severity::Trace => "Trace",
        Severity::Debug => "Debug",
        Severity::Info => "Info",
        Severity::Warn => "Warn",
        Severity::Error => "Error",
    }
}
