//! OPEN FILE: every script of the cartridge in one list, narrowed by typing.

use glaux_input::{InputEvent, Point, PointerButton};

use crate::cartridge::source::{ScriptListing, SourcePath};
use crate::cartridge_fs::{FsError, WalkGap};
use crate::system::workspace::Rect;
use crate::system::workspace::dialog::{AREA_MARGIN, PAD};

/// Rows of files the window shows at once, where the area has room.
pub const LIST_ROWS: usize = 10;
/// Most filter characters kept.
const MAX_FILTER: usize = 32;
/// Most lines describing what the listing skipped.
pub const MAX_GAP_LINES: usize = 2;
/// Lines the window holds besides the files: the filter, a rule under it,
/// and the summary.
const FIXED_LINES: i32 = 3;
/// Width of the window.
const WINDOW_WIDTH: i32 = 260;

/// What the user did with the picker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PickerAction {
    Open(SourcePath),
    Close,
}

/// The script files of the cartridge, as listed when the picker opened.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FilePicker {
    listing: Result<ScriptListing, String>,
    filter: String,
    /// Indices into the listing's paths that match the filter, best first.
    matches: Vec<usize>,
    /// Position in `matches` of the highlighted file.
    focus: usize,
    /// Position in `matches` of the first row shown.
    first: usize,
}

impl FilePicker {
    pub fn new(listing: Result<ScriptListing, FsError>) -> Self {
        let mut picker = Self {
            listing: listing.map_err(|err| err.to_string()),
            filter: String::new(),
            matches: Vec::new(),
            focus: 0,
            first: 0,
        };
        picker.refilter();
        picker
    }

    pub fn filter(&self) -> &str {
        &self.filter
    }

    /// Why the cartridge could not be listed, if it could not.
    pub fn error(&self) -> Option<&str> {
        self.listing.as_ref().err().map(String::as_str)
    }

    /// Every script found, in listing order.
    pub fn paths(&self) -> &[SourcePath] {
        self.listing.as_ref().map_or(&[], |listing| &listing.paths)
    }

    /// What the listing did not search, as lines to show.
    pub fn gap_lines(&self) -> Vec<String> {
        let gaps = self
            .listing
            .as_ref()
            .map_or(&[][..], |listing| &listing.gaps);
        let mut lines: Vec<String> = gaps.iter().take(MAX_GAP_LINES).map(gap_line).collect();
        if gaps.len() > MAX_GAP_LINES
            && let Some(last) = lines.last_mut()
        {
            *last = format!("...and {} more places not searched", gaps.len() - 1);
        }
        lines
    }

    /// The files matching the filter, best first.
    pub fn matches(&self) -> impl Iterator<Item = &SourcePath> {
        self.matches.iter().map(|&index| &self.paths()[index])
    }

    pub fn match_count(&self) -> usize {
        self.matches.len()
    }

    /// Position among the matches of the highlighted file.
    pub fn focus(&self) -> usize {
        self.focus
    }

    /// Position among the matches of the first row shown.
    pub fn first(&self) -> usize {
        self.first
    }

    fn focused(&self) -> Option<&SourcePath> {
        self.matches
            .get(self.focus)
            .map(|&index| &self.paths()[index])
    }

    /// Applies `event`. Every event is taken while the picker is open.
    pub fn handle_input(
        &mut self,
        event: InputEvent,
        layout: &PickerLayout,
    ) -> Option<PickerAction> {
        match event {
            InputEvent::Break => return Some(PickerAction::Close),
            InputEvent::Enter => return self.focused().cloned().map(PickerAction::Open),
            InputEvent::Text(ch)
                if !ch.is_control() && self.filter.chars().count() < MAX_FILTER =>
            {
                self.filter.push(ch);
                self.refilter();
            }
            InputEvent::Backspace => {
                if self.filter.pop().is_some() {
                    self.refilter();
                }
            }
            InputEvent::Up { .. } => self.move_focus(-1, layout.rows),
            InputEvent::Down { .. } => self.move_focus(1, layout.rows),
            InputEvent::Wheel { delta } => self.move_focus(-isize::from(delta), layout.rows),
            InputEvent::PointerDown {
                button: PointerButton::Primary,
                at,
                ..
            } => {
                if !layout.window.contains(at) {
                    return Some(PickerAction::Close);
                }
                if let Some(row) = layout.row_at(at) {
                    let position = self.first + row;
                    if position < self.matches.len() {
                        self.focus = position;
                        return self.focused().cloned().map(PickerAction::Open);
                    }
                }
            }
            _ => {}
        }
        None
    }

    /// Whether a press at `point` opens a file: it lands on a row that
    /// lists one.
    pub fn opens_at(&self, point: Point, layout: &PickerLayout) -> bool {
        layout
            .row_at(point)
            .is_some_and(|row| self.first + row < self.matches.len())
    }

    /// Moves the highlight `step` matches on, scrolling a list of `rows`
    /// rows as little as it takes to keep it shown.
    fn move_focus(&mut self, step: isize, rows: usize) {
        let rows = rows.max(1);
        let Some(last) = self.matches.len().checked_sub(1) else {
            return;
        };
        self.focus = self.focus.saturating_add_signed(step).min(last);
        if self.focus < self.first {
            self.first = self.focus;
        } else if self.focus >= self.first + rows {
            self.first = self.focus + 1 - rows;
        }
    }

    /// Matches the filter again and highlights the best match.
    fn refilter(&mut self) {
        let filter = self.filter.to_lowercase();
        let mut ranked: Vec<(u8, usize)> = self
            .paths()
            .iter()
            .enumerate()
            .filter_map(|(index, path)| rank(path, &filter).map(|rank| (rank, index)))
            .collect();
        // Ties keep listing order, so the result never depends on anything
        // but the paths and the filter.
        ranked.sort_unstable();
        self.matches = ranked.into_iter().map(|(_, index)| index).collect();
        self.focus = 0;
        self.first = 0;
    }
}

/// How well `path` matches the lowercase `filter`, lower first: the file
/// name starts with it, the file name holds it, or the path holds it.
/// `None` when it doesn't match.
fn rank(path: &SourcePath, filter: &str) -> Option<u8> {
    let name = path.file_name().to_lowercase();
    if name.starts_with(filter) {
        Some(0)
    } else if name.contains(filter) {
        Some(1)
    } else if path.as_str().to_lowercase().contains(filter) {
        Some(2)
    } else {
        None
    }
}

/// A part of the cartridge the listing skipped, in words.
fn gap_line(gap: &WalkGap) -> String {
    let path = |path: &str| path.trim_start_matches('/').to_owned();
    match gap {
        WalkGap::FileLimit(limit) => format!("Listing stopped after {limit} files"),
        WalkGap::TooDeep(dir) => format!("Too deep, not searched: {}", path(dir)),
        WalkGap::Unreadable(dir) => format!("Cannot read folder {}", path(dir)),
        WalkGap::AlreadyWalked(dir) => format!("Skipped link to a listed folder: {}", path(dir)),
    }
}

/// Where the picker's parts sit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PickerLayout {
    pub window: Rect,
    pub title_bar: Rect,
    /// The filter line.
    pub filter: Rect,
    /// The first file row; the others follow one cell row apart.
    pub list: Rect,
    /// File rows shown: [`LIST_ROWS`], or fewer where the area is too short
    /// to hold them with every note.
    pub rows: usize,
    /// Top of the summary line, then of each gap line.
    pub summary_y: i32,
    pub text_x: i32,
    pub cell_h: i32,
}

impl PickerLayout {
    /// The window centred in `area`, tall enough for `gap_lines` notes.
    pub fn new(area: Rect, cell_h: i32, gap_lines: usize) -> Self {
        let title_h = cell_h + 2;
        let tallest = area.height - 2 * AREA_MARGIN;
        let room = (tallest - title_h - 2 * PAD) / cell_h.max(1) - FIXED_LINES - gap_lines as i32;
        let list_rows = usize::try_from(room).unwrap_or(0).clamp(1, LIST_ROWS);
        let lines = list_rows as i32 + FIXED_LINES + gap_lines as i32;
        let height = (title_h + PAD + lines * cell_h + PAD).min(tallest);
        let width = WINDOW_WIDTH.min(area.width - 2 * AREA_MARGIN);
        let window = Rect {
            x: area.x + (area.width - width) / 2,
            y: area.y + (area.height - height) / 2,
            width,
            height,
        };
        let title_bar = Rect {
            x: window.x,
            y: window.y,
            width,
            height: title_h,
        };
        let text_x = window.x + PAD;
        let inner = width - 2 * PAD;
        let filter = Rect {
            x: text_x,
            y: title_bar.bottom() + PAD,
            width: inner,
            height: cell_h,
        };
        let list = Rect {
            x: text_x,
            y: filter.bottom() + cell_h,
            width: inner,
            height: cell_h,
        };
        Self {
            window,
            title_bar,
            filter,
            list,
            rows: list_rows,
            summary_y: list.y + list_rows as i32 * cell_h,
            text_x,
            cell_h,
        }
    }

    /// The rectangle of file row `row`.
    pub fn row(&self, row: usize) -> Rect {
        Rect {
            y: self.list.y + row as i32 * self.cell_h,
            ..self.list
        }
    }

    /// The file row under `point`, if any.
    fn row_at(&self, point: Point) -> Option<usize> {
        (0..self.rows).find(|&row| self.row(row).contains(point))
    }
}

#[cfg(test)]
mod tests;
