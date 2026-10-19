//! Where the suggestion list and its documentation panel sit.

use glaux_machine::font::BitmapFont;
use glaux_sense::Candidate;

use super::panel::DocPanel;
use crate::system::workspace::code::text_view::thumb;
use crate::system::workspace::{Rect, wrap};

/// Rows the list shows before it scrolls.
pub const VISIBLE_ROWS: usize = 8;
/// Narrowest the list gets, borders included.
const MIN_LIST_WIDTH: i32 = 96;
/// Space the hard shadow takes right of and below a box.
pub const SHADOW: i32 = 2;
/// Space between the list and its panel, which the list's shadow fills.
const GAP: i32 = 2;
/// Thickness of the list's scroll indicator.
const SCROLL_WIDTH: i32 = 2;
/// Space left of a row's marker and right of its detail.
pub const ROW_PAD_LEFT: i32 = 2;
pub const ROW_PAD_RIGHT: i32 = 4;
/// Space between a row's marker and its name.
pub const MARKER_GAP: i32 = 4;
/// Least space between a row's name and its detail.
const DETAIL_GAP: i32 = 8;
/// Narrowest the panel gets, borders included.
const MIN_PANEL_WIDTH: i32 = 146;
/// Most and fewest text columns the panel wraps its text at. With fewer
/// room than the minimum it is left out.
const MAX_PANEL_COLUMNS: usize = 30;
const MIN_PANEL_COLUMNS: usize = 14;
/// Space around the panel's text.
pub const PANEL_PAD_X: i32 = 4;
const PANEL_PAD_Y: i32 = 3;
/// Lines of summary and of parameters the panel shows.
const SUMMARY_LINES: usize = 4;
const PARAM_LINES: usize = 3;

/// What the layout is drawn with and where it may go.
#[derive(Clone, Copy)]
pub struct Place<'a> {
    pub font: &'a BitmapFont,
    /// Everything the list and panel may cover.
    pub area: Rect,
    pub cell_w: i32,
    pub cell_h: i32,
}

/// One line of the panel and where its top sits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PanelLine {
    Signature { y: i32 },
    Rule { y: i32 },
    Summary { y: i32, text: String },
    Params { y: i32, text: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PanelLayout {
    /// The border included.
    pub window: Rect,
    pub lines: Vec<PanelLine>,
    /// The bar along the panel's bottom that names the keys.
    pub foot: Rect,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompletionLayout {
    /// The border included.
    pub list: Rect,
    /// Each row shown: the position of its candidate and its rectangle.
    pub rows: Vec<(usize, Rect)>,
    pub thumb: Option<Rect>,
    pub track: Option<Rect>,
    pub panel: Option<PanelLayout>,
}

impl CompletionLayout {
    /// Lays out `items` from `first`, with `selected` marked and `panel`
    /// beside it. `anchor` is the top-left of the line the suggestions are
    /// for: the list goes below it, or above when there is no room.
    pub fn new(
        place: Place<'_>,
        items: &[Candidate],
        first: usize,
        selected: usize,
        panel: Option<&DocPanel>,
        anchor: (i32, i32),
    ) -> Self {
        let Place {
            font,
            area,
            cell_w,
            cell_h,
        } = place;
        let cells = |text: &str| i32::try_from(font.text_cells(text)).unwrap_or(i32::MAX);
        let natural = items
            .iter()
            .map(|item| {
                ROW_PAD_LEFT
                    + cell_w
                    + MARKER_GAP
                    + cells(&item.label) * cell_w
                    + DETAIL_GAP
                    + cells(&item.detail) * cell_w
                    + ROW_PAD_RIGHT
            })
            .max()
            .unwrap_or(0);
        let shown = items.len().min(VISIBLE_ROWS);
        let scrolls = items.len() > shown;
        let row_width = natural.max(MIN_LIST_WIDTH - 2);
        let width = 2 + row_width + if scrolls { SCROLL_WIDTH + 1 } else { 0 };
        let height = 2 + 2 + shown as i32 * cell_h;

        let x = anchor
            .0
            .clamp(area.x, (area.right() - width - SHADOW).max(area.x));
        let below = anchor.1 + cell_h;
        let above = anchor.1 - height - SHADOW;
        let y = if below + height + SHADOW <= area.bottom() {
            below
        } else if above >= area.y {
            above
        } else {
            (area.bottom() - height - SHADOW).max(area.y)
        };
        let list = Rect {
            x,
            y,
            width,
            height,
        };

        let rows = (0..shown)
            .map(|row| {
                let rect = Rect {
                    x: list.x + 1,
                    y: list.y + 2 + row as i32 * cell_h,
                    width: row_width,
                    height: cell_h,
                };
                (first + row, rect)
            })
            .collect();
        let track = scrolls.then(|| Rect {
            x: list.x + 1 + row_width,
            y: list.y + 2,
            width: SCROLL_WIDTH,
            height: shown as i32 * cell_h,
        });
        let thumb = track.and_then(|track| {
            let (start, length) = thumb(first, shown, items.len(), track.height)?;
            Some(Rect {
                y: track.y + start,
                height: length,
                ..track
            })
        });
        let selected_y = list.y + 2 + selected.saturating_sub(first) as i32 * cell_h;
        let panel = panel.and_then(|panel| panel_layout(place, panel, list, selected_y));
        Self {
            list,
            rows,
            thumb,
            track,
            panel,
        }
    }

    /// The candidate whose row holds `at`.
    pub fn row_at(&self, at: glaux_input::Point) -> Option<usize> {
        self.rows
            .iter()
            .find(|(_, rect)| rect.contains(at))
            .map(|&(index, _)| index)
    }

    /// Whether `at` is over the list.
    pub fn list_contains(&self, at: glaux_input::Point) -> bool {
        self.list.contains(at)
    }
}

/// The panel for the row at `selected_y`, to the right of `list` or else
/// to its left, or `None` when neither side has room for readable text.
fn panel_layout(
    place: Place<'_>,
    panel: &DocPanel,
    list: Rect,
    selected_y: i32,
) -> Option<PanelLayout> {
    let Place {
        font,
        area,
        cell_w,
        cell_h,
    } = place;
    let chrome = 2 + 2 * PANEL_PAD_X;
    let columns_in = |room: i32| {
        usize::try_from((room - chrome) / cell_w)
            .unwrap_or(0)
            .min(MAX_PANEL_COLUMNS)
    };
    let right_room = area.right() - (list.right() + GAP) - SHADOW;
    let left_room = list.x - GAP - area.x;
    let (on_right, room) = if columns_in(right_room) >= MIN_PANEL_COLUMNS {
        (true, right_room)
    } else if columns_in(left_room) >= MIN_PANEL_COLUMNS {
        (false, left_room)
    } else {
        return None;
    };
    let columns = columns_in(room);

    let summary: Vec<String> = panel
        .summary
        .as_deref()
        .map(|text| wrap(font, text, columns))
        .unwrap_or_default()
        .into_iter()
        .take(SUMMARY_LINES)
        .collect();
    let params: Vec<String> = panel
        .params
        .as_deref()
        .map(|text| wrap(font, text, columns))
        .unwrap_or_default()
        .into_iter()
        .take(PARAM_LINES)
        .collect();

    let signature_cells: usize = panel
        .signature
        .iter()
        .map(|(piece, _)| font.text_cells(piece))
        .sum();
    let widest = summary
        .iter()
        .chain(&params)
        .map(|line| font.text_cells(line))
        .max()
        .unwrap_or(0)
        .max(signature_cells.min(columns));
    let width = (widest as i32 * cell_w + chrome)
        .max(MIN_PANEL_WIDTH)
        .min(room);

    let foot_height = cell_h + 2;
    let mut lines = Vec::new();
    let mut y = 1;
    lines.push(PanelLine::Signature { y });
    y += cell_h + 2 * PANEL_PAD_Y;
    if !summary.is_empty() || !params.is_empty() {
        lines.push(PanelLine::Rule { y });
        y += 1;
    }
    for (block, make) in [
        (summary, summary_line as fn(i32, String) -> PanelLine),
        (params, params_line),
    ] {
        if block.is_empty() {
            continue;
        }
        y += PANEL_PAD_Y;
        for text in block {
            lines.push(make(y, text));
            y += cell_h;
        }
        y += PANEL_PAD_Y;
    }
    let height = y + foot_height + 1;

    let x = if on_right {
        list.right() + GAP
    } else {
        list.x - GAP - width
    };
    let top = (selected_y - 1)
        .min(area.bottom() - height - SHADOW)
        .max(area.y);
    let window = Rect {
        x,
        y: top,
        width,
        height,
    };
    // Line positions were counted from the window's inner top.
    let lines = lines
        .into_iter()
        .map(|line| match line {
            PanelLine::Signature { y } => PanelLine::Signature { y: top + y },
            PanelLine::Rule { y } => PanelLine::Rule { y: top + y },
            PanelLine::Summary { y, text } => PanelLine::Summary { y: top + y, text },
            PanelLine::Params { y, text } => PanelLine::Params { y: top + y, text },
        })
        .collect();
    let foot = Rect {
        x: window.x + 1,
        y: window.bottom() - 1 - foot_height,
        width: window.width - 2,
        height: foot_height,
    };
    Some(PanelLayout {
        window,
        lines,
        foot,
    })
}

fn summary_line(y: i32, text: String) -> PanelLine {
    PanelLine::Summary { y, text }
}

fn params_line(y: i32, text: String) -> PanelLine {
    PanelLine::Params { y, text }
}

#[cfg(test)]
mod tests;
