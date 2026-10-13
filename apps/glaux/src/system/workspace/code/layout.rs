//! Where the Code view's parts sit, and how the document tabs share the
//! strip above the text.

use glaux_input::Point;
use glaux_machine::font::BitmapFont;
use glaux_machine::framebuffer::{HEIGHT, WIDTH};

use crate::cartridge::source::SourcePath;
use crate::rom::ICON_SIZE;
use crate::system::workspace::{ELLIPSIS, Rect};

/// Space before the first tab and after the overflow button.
const STRIP_PAD: i32 = 4;
/// Gap between adjacent tabs.
const TAB_GAP: i32 = 1;
/// Space left of a tab's label.
pub const LABEL_PAD: i32 = 4;
/// Space between a tab's label and its close glyph.
const CLOSE_GAP: i32 = 4;
/// Space right of a tab's close glyph.
const CLOSE_PAD: i32 = 3;
/// Side of the close glyph.
pub const CLOSE_SIZE: i32 = 5;
/// Most cells a tab label takes before it is shortened.
pub const MAX_LABEL_CELLS: usize = 16;
/// Width of the chevron glyph that opens the list of open tabs.
pub const CHEVRON_WIDTH: i32 = 5;
/// Cells reserved left of the chevron for the count of hidden tabs.
const HIDDEN_COUNT_CELLS: i32 = 3;
/// Side of the art of the settings icon, which sits centred in its
/// [`ICON_SIZE`] canvas.
pub const SETTINGS_ART: i32 = 12;
/// Transparent margin around the settings icon's art.
pub const SETTINGS_MARGIN: i32 = (ICON_SIZE as i32 - SETTINGS_ART) / 2;

/// Cell size of the system font and where the view starts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Metrics {
    pub cell_w: i32,
    pub cell_h: i32,
    /// First row below the workspace's tab bar.
    pub top: i32,
}

/// The Code view's rows, top to bottom, and the font that measures the
/// text laid out in them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeLayout {
    pub metrics: Metrics,
    pub font: BitmapFont,
    /// The document tabs.
    pub strip: Rect,
    /// The active document's language and path.
    pub options: Rect,
    /// The active document's text.
    pub text: Rect,
    pub status: Rect,
}

impl CodeLayout {
    pub fn new(metrics: Metrics, font: BitmapFont) -> Self {
        let bar = metrics.cell_h + 2;
        let width = WIDTH as i32;
        let strip = Rect {
            x: 0,
            y: metrics.top,
            width,
            height: bar,
        };
        // Room for the settings icon's art above the bar's rule.
        let options = Rect {
            y: strip.bottom(),
            height: bar.max(SETTINGS_ART + 1),
            ..strip
        };
        let status = Rect {
            y: HEIGHT as i32 - bar,
            ..strip
        };
        let text = Rect {
            x: 0,
            y: options.bottom(),
            width,
            height: status.y - options.bottom(),
        };
        Self {
            metrics,
            font,
            strip,
            options,
            text,
            status,
        }
    }

    /// Text columns that fit across the text area.
    pub fn text_columns(&self) -> usize {
        usize::try_from((self.text.width - 2 * STRIP_PAD) / self.metrics.cell_w.max(1)).unwrap_or(0)
    }

    /// The button opening EDITOR SETTINGS, at the options bar's right end:
    /// the visible art of its icon.
    pub fn settings_button(&self) -> Rect {
        Rect {
            x: self.options.right() - STRIP_PAD - SETTINGS_ART,
            y: self.options.y,
            width: SETTINGS_ART,
            height: SETTINGS_ART,
        }
    }

    /// The button opening the list of open tabs, at the strip's right end.
    /// It also holds the count of tabs the strip can't show.
    pub fn more_button(&self) -> Rect {
        let width = HIDDEN_COUNT_CELLS * self.metrics.cell_w + CHEVRON_WIDTH + 4;
        Rect {
            x: self.strip.right() - STRIP_PAD - width,
            y: self.strip.y,
            width,
            height: self.strip.height - 1,
        }
    }

    /// Cells `text` takes on one line.
    pub fn cells(&self, text: &str) -> i32 {
        i32::try_from(self.font.text_cells(text)).unwrap_or(i32::MAX)
    }

    /// Width of the tab showing `label`.
    pub fn tab_width(&self, label: &str) -> i32 {
        LABEL_PAD + self.cells(label) * self.metrics.cell_w + CLOSE_GAP + CLOSE_SIZE + CLOSE_PAD
    }

    /// The tabs showing `labels`, starting from the one at `scroll`, as
    /// many as fit left of the overflow button. Rendering and hit-testing
    /// both use this.
    pub fn strip_tabs(&self, labels: &[String], scroll: usize) -> Vec<StripTab> {
        let limit = self.more_button().x - TAB_GAP;
        let mut x = self.strip.x + STRIP_PAD;
        let mut tabs = Vec::new();
        for (index, label) in labels.iter().enumerate().skip(scroll) {
            let width = self.tab_width(label);
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
    pub fn reveal(&self, labels: &[String], scroll: usize, active: usize) -> usize {
        let mut scroll = scroll.min(active);
        while scroll < active
            && !self
                .strip_tabs(labels, scroll)
                .iter()
                .any(|tab| tab.index == active)
        {
            scroll += 1;
        }
        scroll
    }

    /// What `point` is over in the strip, for tabs laid out from `scroll`.
    pub fn strip_hit(&self, labels: &[String], scroll: usize, point: Point) -> StripHit {
        if !self.strip.contains(point) {
            return StripHit::None;
        }
        if self.more_button().contains(point) {
            return StripHit::More;
        }
        for tab in self.strip_tabs(labels, scroll) {
            if tab.close.inset(-1).contains(point) {
                return StripHit::Close(tab.index);
            }
            if tab.rect.contains(point) {
                return StripHit::Tab(tab.index);
            }
        }
        StripHit::None
    }

    /// The index of the shown tab whose columns hold `x`, for dragging a
    /// tab along the strip.
    pub fn tab_column(&self, labels: &[String], scroll: usize, x: i32) -> Option<usize> {
        self.strip_tabs(labels, scroll)
            .into_iter()
            .find(|tab| (tab.rect.x..tab.rect.right() + TAB_GAP).contains(&x))
            .map(|tab| tab.index)
    }
}

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

/// The label of each tab showing `paths`, in order: the file name, or the
/// whole path where another tab shows a file of the same name, kept to
/// [`MAX_LABEL_CELLS`] of `font`'s cells by cutting its start.
pub fn tab_labels(font: &BitmapFont, paths: &[&SourcePath]) -> Vec<String> {
    paths
        .iter()
        .map(|path| {
            let name = path.file_name();
            let shared = paths
                .iter()
                .filter(|other| other.file_name() == name)
                .count()
                > 1;
            let label = if shared { path.as_str() } else { name };
            shorten_start(font, label, MAX_LABEL_CELLS)
        })
        .collect()
}

/// `text` kept to `max` of `font`'s cells by cutting its start, where
/// paths differ least, and marking the cut with [`ELLIPSIS`].
pub fn shorten_start(font: &BitmapFont, text: &str, max: usize) -> String {
    if font.text_cells(text) <= max {
        return text.to_owned();
    }
    // Too little room for the mark and any text: show the text alone.
    let (mark, room) = match max.checked_sub(font.text_cells(ELLIPSIS)) {
        Some(room) if room > 0 => (ELLIPSIS, room),
        _ => ("", max),
    };
    let mut used = 0;
    let mut start = text.len();
    for (at, ch) in text.char_indices().rev() {
        used += font.cells(ch);
        if used > room {
            break;
        }
        start = at;
    }
    format!("{mark}{}", &text[start..])
}

#[cfg(test)]
mod tests;
