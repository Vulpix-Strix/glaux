use glaux_machine::font::BitmapFont;
use glaux_machine::framebuffer::{Framebuffer, WIDTH};
use glaux_video::draw::{draw_text, fill_rect};

use crate::system::{ACCENT, BACKGROUND, DIM, PALE, TEXT};

pub(super) const ROW: i32 = 12;
pub(super) const BAR: u8 = 12;
pub(super) const IDLE: u8 = 13;
const ELLIPSIS: &str = "..";
const CHECKER: i32 = 2;

pub(super) struct Painter<'a> {
    pub(super) fb: &'a mut Framebuffer,
    pub(super) font: &'a BitmapFont,
}

impl<'a> Painter<'a> {
    pub(super) fn new(fb: &'a mut Framebuffer, font: &'a BitmapFont) -> Self {
        Self { fb, font }
    }

    pub(super) fn cell_w(&self) -> i32 {
        i32::from(self.font.metrics().cell_width())
    }

    pub(super) fn width(&self, text: &str) -> i32 {
        i32::try_from(self.font.text_cells(text)).unwrap_or(i32::MAX) * self.cell_w()
    }

    pub(super) fn fill(&mut self, x: i32, y: i32, w: i32, h: i32, color: u8) {
        fill_rect(self.fb, x, y, w, h, color);
    }

    pub(super) fn clear(&mut self, color: u8) {
        self.fill(0, 0, WIDTH as i32, ROW * 20, color);
    }

    pub(super) fn text(&mut self, x: i32, y: i32, text: &str, color: u8) {
        draw_text(self.fb, self.font, text, x, y, color);
    }

    pub(super) fn text_right(&mut self, right: i32, y: i32, text: &str, color: u8) {
        let x = right - self.width(text);
        self.text(x, y, text, color);
    }

    pub(super) fn text_center(&mut self, y: i32, text: &str, color: u8) {
        let x = (WIDTH as i32 - self.width(text)) / 2;
        self.text(x, y, text, color);
    }

    /// Draws `text` cut to `cells` character cells, ending in `..` when
    /// something was left out.
    pub(super) fn text_clipped(&mut self, x: i32, y: i32, text: &str, cells: usize, color: u8) {
        let text = clip(self.font, text, cells);
        self.text(x, y, &text, color);
    }

    pub(super) fn checker(&mut self, x: i32, y: i32, w: i32, h: i32) {
        for row in 0..(h + CHECKER - 1) / CHECKER {
            for col in 0..(w + CHECKER - 1) / CHECKER {
                let color = if (row + col) % 2 == 0 { BAR } else { IDLE };
                self.fill(
                    x + col * CHECKER,
                    y + row * CHECKER,
                    CHECKER.min(w - col * CHECKER),
                    CHECKER.min(h - row * CHECKER),
                    color,
                );
            }
        }
    }

    pub(super) fn heading(&mut self, y: i32, title: &str) {
        let width = self.width(title);
        let band = 28;
        let gap = 6;
        let left = (WIDTH as i32 - width) / 2;
        self.checker(left - gap - band, y + 4, band, 4);
        self.checker(left + width + gap, y + 4, band, 4);
        self.text(left, y, title, PALE);
    }

    pub(super) fn tab_bar(&mut self, tabs: &[String], active: usize, position: &str) {
        self.fill(0, 0, WIDTH as i32, ROW, BAR);
        let cell = self.cell_w();
        let mut x = 0;
        for (index, label) in tabs.iter().enumerate() {
            let width = self.width(label) + cell;
            if index == active {
                self.fill(x, 0, width, ROW, BACKGROUND);
                self.fill(x, ROW - 1, width, 1, ACCENT);
                self.text(x + cell / 2, 0, label, TEXT);
            } else {
                self.text(x + cell / 2, 0, label, PALE);
            }
            x += width;
        }
        self.text_right(WIDTH as i32 - cell, 0, position, DIM);
    }

    pub(super) fn status_bar(&mut self, hint: &str, status: &str) {
        let y = 240 - ROW;
        self.fill(0, y, WIDTH as i32, ROW, BAR);
        let cell = self.cell_w();
        let status_width = self.width(status);
        let room = ((WIDTH as i32 - cell * 3 - status_width) / cell).max(0);
        self.text_clipped(cell, y, hint, room as usize, DIM);
        self.text_right(WIDTH as i32 - cell, y, status, TEXT);
    }

    pub(super) fn window(&mut self, (x, y, w, h): (i32, i32, i32, i32), title: &str) {
        self.fill(x + 2, y + 2, w, h, BAR);
        self.fill(x - 1, y - 1, w + 2, h + 2, BACKGROUND);
        self.fill(x, y, w, h, TEXT);
        self.fill(x, y, w, ROW, BAR);
        let left = x + (w - self.width(title)) / 2;
        self.text(left, y, title, TEXT);
    }

    pub(super) fn scrollbar(
        &mut self,
        (x, y, height): (i32, i32, i32),
        (offset, visible, total): (usize, usize, usize),
        (track, thumb_color): (u8, u8),
    ) {
        if let Some((start, length)) = thumb(offset, visible, total, height) {
            self.fill(x, y, 3, height, track);
            self.fill(x, y + start, 3, length, thumb_color);
        }
    }
}

/// Where a scroll thumb sits along a track `track` pixels long, as its start
/// and length, for a view showing `visible` of `total` rows from `offset`.
/// `None` when everything is visible.
fn thumb(offset: usize, visible: usize, total: usize, track: i32) -> Option<(i32, i32)> {
    if total <= visible || track <= 0 {
        return None;
    }
    let (offset, visible, total) = (offset as i64, visible as i64, total as i64);
    let track = i64::from(track);
    let length = (track * visible / total).clamp(4.min(track), track);
    let range = total - visible;
    let start = (track - length) * offset.min(range) / range;
    Some((start as i32, length as i32))
}

/// `text` cut to `cells` character cells, ending in `..` when something
/// was left out.
pub(super) fn clip(font: &BitmapFont, text: &str, cells: usize) -> String {
    if font.text_cells(text) <= cells {
        return text.to_owned();
    }
    let room = cells.saturating_sub(ELLIPSIS.len());
    let mut used = 0;
    let mut out = String::new();
    for ch in text.chars() {
        let width = font.cells(ch);
        if used + width > room {
            break;
        }
        used += width;
        out.push(ch);
    }
    if cells >= ELLIPSIS.len() {
        out.push_str(ELLIPSIS);
    }
    out
}

/// Breaks `text` into lines of at most `cells` character cells, at spaces
/// when it has any and inside words otherwise. Explicit newlines are kept.
pub(super) fn wrap(font: &BitmapFont, text: &str, cells: usize) -> Vec<String> {
    let cells = cells.max(1);
    let mut lines = Vec::new();
    for paragraph in text.split('\n') {
        let mut line = String::new();
        let mut used = 0;
        for word in paragraph.split(' ') {
            let width = font.text_cells(word);
            let space = usize::from(!line.is_empty());
            if used + space + width <= cells {
                if space == 1 {
                    line.push(' ');
                }
                line.push_str(word);
                used += space + width;
                continue;
            }
            if !line.is_empty() {
                lines.push(std::mem::take(&mut line));
                used = 0;
            }
            // A word longer than a line is split wherever it reaches the edge.
            for ch in word.chars() {
                let width = font.cells(ch);
                if used + width > cells && !line.is_empty() {
                    lines.push(std::mem::take(&mut line));
                    used = 0;
                }
                line.push(ch);
                used += width;
            }
        }
        lines.push(line);
    }
    lines
}
