//! Where the browser's parts sit on the screen.

use glaux_input::Point;
use glaux_machine::font::BitmapFont;
use glaux_machine::framebuffer::{HEIGHT, WIDTH};

use super::LanguageFilter;
use crate::system::workspace::Rect;
use crate::system::workspace::code::text_view::INDICATOR;
use crate::ui_text;

/// Cells the side panel's text takes.
const NAV_COLS: i32 = 13;
/// Space left of the side panel's text and right of it.
const NAV_PAD: i32 = 3;
/// Space left of the page text.
const TEXT_PAD: i32 = 6;
/// Space between the page text and the scroll indicator.
const TRACK_GAP: i32 = 2;
/// Space above and below the page text.
const PAGE_PAD: i32 = 2;
/// Space between two buttons of the options bar.
const BUTTON_GAP: i32 = 3;
/// Space left and right of a button's label.
const BUTTON_PAD: i32 = 2;

/// Cell size of the system font and where the view starts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Metrics {
    pub cell_w: i32,
    pub cell_h: i32,
    /// First row below the workspace's tab bar.
    pub top: i32,
}

/// The buttons of the options bar and the text between them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OptionsBar {
    pub back: Rect,
    pub forward: Rect,
    /// Where the page's path and title go.
    pub crumb: Rect,
    pub filter: Rect,
    pub nav: Rect,
    pub search: Rect,
}

/// The browser's rows, top to bottom.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Geometry {
    pub metrics: Metrics,
    /// The page tabs.
    pub strip: Rect,
    pub options: Rect,
    /// The side panel and the page.
    pub body: Rect,
    pub status: Rect,
}

impl Geometry {
    pub fn new(metrics: Metrics) -> Self {
        let bar = metrics.cell_h + 2;
        let width = WIDTH as i32;
        let strip = Rect {
            x: 0,
            y: metrics.top,
            width,
            height: bar,
        };
        let options = Rect {
            y: strip.bottom(),
            ..strip
        };
        let status = Rect {
            y: HEIGHT as i32 - bar,
            ..strip
        };
        let body = Rect {
            x: 0,
            y: options.bottom(),
            width,
            height: status.y - options.bottom(),
        };
        Self {
            metrics,
            strip,
            options,
            body,
            status,
        }
    }

    /// The side panel, including the rule on its right edge, or `None`
    /// while it is hidden.
    pub fn nav(&self, collapsed: bool) -> Option<Rect> {
        (!collapsed).then(|| Rect {
            width: NAV_COLS * self.metrics.cell_w + 2 * NAV_PAD + 1,
            ..self.body
        })
    }

    /// Cells of text that fit in the side panel.
    pub fn nav_cols(&self) -> usize {
        NAV_COLS as usize
    }

    /// Where the side panel's text starts.
    pub fn nav_text_x(&self) -> i32 {
        NAV_PAD
    }

    /// Where the page's text goes: exactly [`cols`](Self::cols) cells wide.
    pub fn text(&self, collapsed: bool) -> Rect {
        let left = self.nav(collapsed).map_or(0, |nav| nav.right()) + TEXT_PAD;
        let height = self.body.height - 2 * PAGE_PAD;
        Rect {
            x: left,
            y: self.body.y + PAGE_PAD,
            width: self.cols(collapsed) as i32 * self.metrics.cell_w,
            height,
        }
    }

    /// Cells of text that fit across the page.
    pub fn cols(&self, collapsed: bool) -> usize {
        let left = self.nav(collapsed).map_or(0, |nav| nav.right()) + TEXT_PAD;
        let room = self.body.right() - INDICATOR - TRACK_GAP - 1 - left;
        usize::try_from(room / self.metrics.cell_w.max(1)).unwrap_or(0)
    }

    /// The scroll indicator's track, along the body's right edge.
    pub fn track(&self) -> Rect {
        Rect {
            x: self.body.right() - INDICATOR - 1,
            y: self.body.y + PAGE_PAD,
            width: INDICATOR,
            height: self.body.height - 2 * PAGE_PAD,
        }
    }

    /// The buttons of the options bar for a language filter and the
    /// panel/search labels `font` measures.
    pub fn options_bar(&self, font: &BitmapFont, filter: LanguageFilter) -> OptionsBar {
        let cell_w = self.metrics.cell_w;
        let bar = self.options;
        let height = bar.height - 3;
        let y = bar.y + 1;
        let width = |label: &str| -> i32 {
            i32::try_from(font.text_cells(label)).unwrap_or(0) * cell_w + 2 * BUTTON_PAD
        };
        let mut right = bar.right() - 4;
        let mut button = |label: &str| {
            let w = width(label);
            right -= w;
            let rect = Rect {
                x: right,
                y,
                width: w,
                height,
            };
            right -= BUTTON_GAP;
            rect
        };
        let search = button(&search_label());
        let nav = button(&nav_label());
        let filter = button(&filter.label());
        let left_edge = 4;
        let back = Rect {
            x: left_edge,
            y,
            width: width("<"),
            height,
        };
        let forward = Rect {
            x: back.right() + BUTTON_GAP,
            width: width(">"),
            ..back
        };
        let crumb_x = forward.right() + BUTTON_GAP + cell_w / 2;
        let crumb = Rect {
            x: crumb_x,
            y,
            width: (right - crumb_x).max(0),
            height,
        };
        OptionsBar {
            back,
            forward,
            crumb,
            filter,
            nav,
            search,
        }
    }

    /// Offset of a button's label from its left edge.
    pub const fn button_pad() -> i32 {
        BUTTON_PAD
    }

    /// The text row index under `at` in the page, and the cell column.
    pub fn text_hit(&self, collapsed: bool, at: Point) -> Option<(i32, usize)> {
        let text = self.text(collapsed);
        let (x, y) = (i32::from(at.x), i32::from(at.y));
        if !(text.x..text.right()).contains(&x) || !(text.y..text.bottom()).contains(&y) {
            return None;
        }
        Some((
            y - text.y,
            usize::try_from((x - text.x) / self.metrics.cell_w.max(1)).ok()?,
        ))
    }
}

/// The options bar's label for the side panel's button.
pub fn nav_label() -> String {
    ui_text::get("docs-nav")
}
/// The options bar's label for the search button.
pub fn search_label() -> String {
    ui_text::get("docs-search")
}
