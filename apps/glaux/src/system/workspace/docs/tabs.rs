//! The strip of page tabs above the options bar.

use glaux_input::Point;
use glaux_machine::font::BitmapFont;

use super::view::Geometry;
use crate::system::workspace::{ELLIPSIS, Rect, take_cells};

/// Space before the first tab and after the overflow button.
const STRIP_PAD: i32 = 4;
/// Gap between adjacent tabs.
const TAB_GAP: i32 = 1;
/// Space left of a tab's marker.
pub const LABEL_PAD: i32 = 4;
/// Space between a tab's label and its close glyph.
const CLOSE_GAP: i32 = 4;
/// Space right of a tab's close glyph.
const CLOSE_PAD: i32 = 3;
/// Side of the close glyph.
pub const CLOSE_SIZE: i32 = 5;
/// Most cells a tab label takes before it is shortened.
pub const MAX_LABEL_CELLS: usize = 14;
/// Width of the chevron glyph that opens the list of tabs.
pub const CHEVRON_WIDTH: i32 = 5;
/// Cells reserved left of the chevron for the count of hidden tabs.
const HIDDEN_COUNT_CELLS: i32 = 3;
/// What sits before a label to say the tab holds a page.
pub const MARKER: &str = "i";

/// One tab as the strip shows it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StripTab {
    /// Position in tab order.
    pub index: usize,
    pub rect: Rect,
    /// The glyph that closes the tab.
    pub close: Rect,
}

/// What a point in the strip is over.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StripHit {
    None,
    Tab(usize),
    Close(usize),
    More,
}

/// `title` kept to [`MAX_LABEL_CELLS`] of `font`'s cells.
pub fn label(font: &BitmapFont, title: &str) -> String {
    if font.text_cells(title) <= MAX_LABEL_CELLS {
        return title.to_owned();
    }
    let room = MAX_LABEL_CELLS.saturating_sub(font.text_cells(ELLIPSIS));
    format!("{}{ELLIPSIS}", take_cells(font, title, room))
}

impl Geometry {
    fn cells(font: &BitmapFont, text: &str) -> i32 {
        i32::try_from(font.text_cells(text)).unwrap_or(i32::MAX)
    }

    /// The button opening the list of tabs, at the strip's right end. It
    /// also holds the count of tabs the strip can't show.
    pub fn more_button(&self) -> Rect {
        let width = HIDDEN_COUNT_CELLS * self.metrics.cell_w + CHEVRON_WIDTH + 4;
        Rect {
            x: self.strip.right() - STRIP_PAD - width,
            y: self.strip.y,
            width,
            height: self.strip.height - 1,
        }
    }

    /// Width of the tab showing `label`: its marker, a space and the label.
    pub fn tab_width(&self, font: &BitmapFont, label: &str) -> i32 {
        let text = (2 + Self::cells(font, label)) * self.metrics.cell_w;
        LABEL_PAD + text + CLOSE_GAP + CLOSE_SIZE + CLOSE_PAD
    }

    /// The tabs showing `labels`, starting from the one at `scroll`, as many
    /// as fit left of the overflow button. Rendering and hit-testing both
    /// use this.
    pub fn strip_tabs(&self, font: &BitmapFont, labels: &[String], scroll: usize) -> Vec<StripTab> {
        let limit = self.more_button().x - TAB_GAP;
        let mut x = self.strip.x + STRIP_PAD;
        let mut tabs = Vec::new();
        for (index, label) in labels.iter().enumerate().skip(scroll) {
            let width = self.tab_width(font, label);
            if x + width > limit {
                break;
            }
            let rect = Rect {
                x,
                y: self.strip.y,
                width,
                height: self.strip.height - 1,
            };
            let close = Rect {
                x: rect.right() - CLOSE_PAD - CLOSE_SIZE,
                y: rect.y + (rect.height - CLOSE_SIZE) / 2,
                width: CLOSE_SIZE,
                height: CLOSE_SIZE,
            };
            tabs.push(StripTab { index, rect, close });
            x += width + TAB_GAP;
        }
        tabs
    }

    /// The scroll closest to `scroll` that shows the tab at `active` in
    /// full.
    pub fn reveal(
        &self,
        font: &BitmapFont,
        labels: &[String],
        scroll: usize,
        active: usize,
    ) -> usize {
        let mut scroll = scroll.min(active);
        while scroll < active
            && !self
                .strip_tabs(font, labels, scroll)
                .iter()
                .any(|tab| tab.index == active)
        {
            scroll += 1;
        }
        scroll
    }

    /// What `point` is over in the strip, for tabs laid out from `scroll`.
    pub fn strip_hit(
        &self,
        font: &BitmapFont,
        labels: &[String],
        scroll: usize,
        point: Point,
    ) -> StripHit {
        if !self.strip.contains(point) {
            return StripHit::None;
        }
        if self.more_button().contains(point) {
            return StripHit::More;
        }
        for tab in self.strip_tabs(font, labels, scroll) {
            if tab.close.inset(-1).contains(point) {
                return StripHit::Close(tab.index);
            }
            if tab.rect.contains(point) {
                return StripHit::Tab(tab.index);
            }
        }
        StripHit::None
    }

    /// The window listing every tab, dropped from the strip's right end,
    /// and the row of each tab it shows, `focus` kept in view.
    pub fn menu_rows(&self, count: usize, focus: usize) -> (Rect, Vec<(usize, Rect)>) {
        let cell_h = self.metrics.cell_h;
        let more = self.more_button();
        let fit = usize::try_from((self.status.y - more.bottom() - 4) / cell_h.max(1)).unwrap_or(1);
        let shown = count.min(fit.max(1));
        let first = (focus + 1).saturating_sub(shown);
        let width = (MAX_LABEL_CELLS as i32 + 2) * self.metrics.cell_w + 6;
        let window = Rect {
            x: more.right() - width,
            y: more.bottom() + 1,
            width,
            height: shown as i32 * cell_h + 4,
        };
        let rows = (0..shown)
            .map(|row| {
                let rect = Rect {
                    x: window.x + 1,
                    y: window.y + 2 + row as i32 * cell_h,
                    width: window.width - 2,
                    height: cell_h,
                };
                (first + row, rect)
            })
            .collect();
        (window, rows)
    }
}
