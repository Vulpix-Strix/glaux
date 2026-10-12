//! Where the active document's text, its line numbers and its scroll
//! indicators sit, and how far its view can scroll.

use super::layout::CodeLayout;
use crate::system::code_session::DocumentView;
use crate::system::code_text::{MetricsKey, TextMetrics};
use crate::system::editor_settings::EditorSettings;
use crate::system::workspace::Rect;

/// Space left of the line numbers.
const GUTTER_PAD_LEFT: i32 = 4;
/// Space between the line numbers and the rule right of them.
const GUTTER_PAD_RIGHT: i32 = 3;
/// Space between the gutter, or the view's left edge, and the text.
const TEXT_PAD: i32 = 4;
/// Fewest digits the gutter makes room for, so short files don't shift
/// their text as they pass ten lines.
const MIN_DIGITS: usize = 2;
/// Thickness of a scroll indicator.
pub const INDICATOR: i32 = 2;
/// Space between the text and an indicator.
const INDICATOR_GAP: i32 = 1;
/// Shortest an indicator's thumb gets, so it stays visible on long files.
const MIN_THUMB: i32 = 4;

/// The text view's parts for one document and set of settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextGeometry {
    /// The line numbers, the rule on its right edge included. `None` while
    /// line numbers are hidden.
    pub gutter: Option<Rect>,
    /// Where the text's cells go: exactly `columns` by `rows` cells.
    pub viewport: Rect,
    pub columns: usize,
    pub rows: usize,
    /// The vertical indicator's track, along the view's right edge.
    pub vertical: Rect,
    /// The horizontal indicator's track, along the view's bottom edge.
    pub horizontal: Rect,
    /// Whether lines fold at `columns`.
    pub wrap: bool,
    pub tab_size: usize,
}

impl TextGeometry {
    /// The text view of `layout` for a document of `line_count` lines shown
    /// with `settings`.
    pub fn new(layout: &CodeLayout, line_count: usize, settings: &EditorSettings) -> Self {
        let area = layout.text;
        let (cell_w, cell_h) = (layout.metrics.cell_w.max(1), layout.metrics.cell_h.max(1));
        let gutter = settings.line_numbers.then(|| {
            let digits = digits(line_count).max(MIN_DIGITS) as i32;
            Rect {
                width: GUTTER_PAD_LEFT + digits * cell_w + GUTTER_PAD_RIGHT + 1,
                ..area
            }
        });
        let text_x = gutter.map_or(area.x, Rect::right) + TEXT_PAD;
        let vertical = Rect {
            x: area.right() - INDICATOR,
            y: area.y,
            width: INDICATOR,
            height: area.height - INDICATOR - INDICATOR_GAP,
        };
        let horizontal = Rect {
            x: text_x,
            y: area.bottom() - INDICATOR,
            width: vertical.x - INDICATOR_GAP - text_x,
            height: INDICATOR,
        };
        let columns = ((vertical.x - INDICATOR_GAP - text_x) / cell_w).max(1);
        let rows = ((area.height - 1 - INDICATOR - INDICATOR_GAP) / cell_h).max(1);
        Self {
            gutter,
            viewport: Rect {
                x: text_x,
                y: area.y + 1,
                width: columns * cell_w,
                height: rows * cell_h,
            },
            columns: columns as usize,
            rows: rows as usize,
            vertical,
            horizontal,
            wrap: settings.word_wrap,
            tab_size: settings.tab_columns(),
        }
    }

    /// Where line numbers end, right-aligned in the gutter.
    pub fn number_right(&self) -> i32 {
        self.gutter.map_or(self.viewport.x, |gutter| {
            gutter.right() - 1 - GUTTER_PAD_RIGHT
        })
    }

    /// Columns lines fold at, or `None` when they don't fold.
    pub fn wrap_columns(&self) -> Option<usize> {
        self.wrap.then_some(self.columns)
    }

    /// What a document's metrics must have been measured with to describe
    /// this view.
    pub fn metrics_key(&self) -> MetricsKey {
        MetricsKey {
            tab_size: self.tab_size,
            wrap_columns: self.wrap_columns(),
        }
    }

    /// `view` moved to the nearest position this view can show: the last
    /// row no lower than the bottom of the view, and no column offset while
    /// lines fold.
    pub fn clamp(&self, view: DocumentView, metrics: &TextMetrics) -> DocumentView {
        let row = if self.wrap { view.top_row } else { 0 };
        self.at_row(
            metrics.row_index(view.top_line, row),
            view.left_column,
            metrics,
        )
    }

    /// `view` scrolled `step` rows down, or up when negative.
    pub fn scroll_rows(
        &self,
        view: DocumentView,
        step: isize,
        metrics: &TextMetrics,
    ) -> DocumentView {
        let view = self.clamp(view, metrics);
        let top = metrics.row_index(view.top_line, view.top_row);
        self.at_row(top.saturating_add_signed(step), view.left_column, metrics)
    }

    /// `view` scrolled `step` columns right, or left when negative. Folded
    /// lines have nothing to the side, so it then stays put.
    pub fn scroll_columns(
        &self,
        view: DocumentView,
        step: isize,
        metrics: &TextMetrics,
    ) -> DocumentView {
        let view = self.clamp(view, metrics);
        DocumentView {
            left_column: self.clamp_column(view.left_column.saturating_add_signed(step), metrics),
            ..view
        }
    }

    /// The view whose first row is document row `top`, kept in range.
    fn at_row(&self, top: usize, left_column: usize, metrics: &TextMetrics) -> DocumentView {
        let top = top.min(metrics.rows().saturating_sub(self.rows));
        let (top_line, top_row) = metrics.position(top);
        DocumentView {
            top_line,
            top_row,
            left_column: self.clamp_column(left_column, metrics),
        }
    }

    fn clamp_column(&self, column: usize, metrics: &TextMetrics) -> usize {
        if self.wrap {
            0
        } else {
            column.min(metrics.widest().saturating_sub(self.columns))
        }
    }
}

/// Decimal digits in `n`.
pub fn digits(n: usize) -> usize {
    n.checked_ilog10().map_or(1, |log| log as usize + 1)
}

/// Where an indicator's thumb sits along a track `track` pixels long, as
/// its start and length, for a view showing `visible` of `total` rows or
/// columns from `offset`. `None` when everything is visible.
pub fn thumb(offset: usize, visible: usize, total: usize, track: i32) -> Option<(i32, i32)> {
    if total <= visible || track <= 0 {
        return None;
    }
    // Row and column counts are far below i64::MAX.
    let (offset, visible, total) = (offset as i64, visible as i64, total as i64);
    let track = i64::from(track);
    let length = (track * visible / total).clamp(i64::from(MIN_THUMB).min(track), track);
    let range = total - visible;
    let start = (track - length) * offset.min(range) / range;
    Some((start as i32, length as i32))
}

#[cfg(test)]
mod tests;
