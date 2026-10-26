//! The palette indices of the system UI and the text lines drawn with them.

use glaux_machine::framebuffer::{HEIGHT, WIDTH};

// System UI colors, as indices into the system palette (Zughy 32).
pub(super) const BACKGROUND: u8 = 11;
pub(super) const TEXT: u8 = 16;
pub(super) const DIM: u8 = 14;
pub(super) const PALE: u8 = 15;
pub(super) const OK: u8 = 8;
pub(super) const ACCENT: u8 = 18;
pub(super) const ERROR: u8 = 28;
pub(super) const TRACE: u8 = 13;
pub(super) const WARN: u8 = 26;

/// Character cells that fit across the screen inside the margin left by
/// [`Line::origin`].
pub(super) fn text_columns(cell_w: i32) -> usize {
    let cells = WIDTH as i32 / cell_w.max(1);
    usize::try_from(cells - 2).unwrap_or(0)
}

/// Character rows that fit down the screen inside the margin left by
/// [`Line::origin`].
pub(super) fn text_rows(cell_h: i32) -> usize {
    let cells = HEIGHT as i32 / cell_h.max(1);
    usize::try_from(cells - 2).unwrap_or(0)
}

/// One line of system text, positioned in character cells.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Line {
    pub(super) col: i32,
    pub(super) row: i32,
    pub(super) text: String,
    pub(super) color: u8,
    /// Fills the cells covered by the text before drawing it.
    pub(super) background: Option<u8>,
}

impl Line {
    pub(super) fn new(col: i32, row: i32, text: impl Into<String>, color: u8) -> Self {
        Self {
            col,
            row,
            text: text.into(),
            color,
            background: None,
        }
    }

    pub(super) fn with_background(mut self, color: u8) -> Self {
        self.background = Some(color);
        self
    }

    /// Top-left pixel of the line, leaving a one-cell margin around the
    /// screen.
    pub(super) fn origin(&self, cell_w: i32, cell_h: i32) -> (i32, i32) {
        ((self.col + 1) * cell_w, (self.row + 1) * cell_h)
    }
}
