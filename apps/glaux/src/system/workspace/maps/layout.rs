//! Where the Map view's controls sit. Rendering and hit-testing both read
//! it.

use glaux_input::Point;
use glaux_machine::framebuffer::{HEIGHT, WIDTH};

use super::Metrics;
use crate::system::workspace::Rect;

/// Space between the view's edge and its controls.
const MARGIN: i32 = 4;
/// Space between neighbouring controls.
const GAP: i32 = 3;

/// A button of the header.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Action {
    New,
    Rename,
    Delete,
    Save,
}

impl Action {
    pub(super) const ALL: [Self; 4] = [Self::New, Self::Rename, Self::Delete, Self::Save];

    fn index(self) -> usize {
        self as usize
    }

    pub(super) fn message_id(self) -> &'static str {
        match self {
            Self::New => "maps-new",
            Self::Rename => "maps-rename",
            Self::Delete => "maps-delete",
            Self::Save => "maps-save",
        }
    }
}

/// What a point of the view is over.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Hit {
    Selector,
    Action(Action),
    /// A row of the open list, by index into the maps.
    Row(usize),
    /// The CREATE MAP button of the empty state.
    CreateMap,
    Nothing,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Layout {
    metrics: Metrics,
    /// Width of each header button, in [`Action::ALL`] order.
    button_w: [i32; 4],
}

impl Layout {
    pub(super) const fn new(metrics: Metrics, button_w: [i32; 4]) -> Self {
        Self { metrics, button_w }
    }

    /// The row holding the selector and the buttons.
    pub(super) fn header(&self) -> Rect {
        Rect {
            x: MARGIN,
            y: self.metrics.top + MARGIN,
            width: WIDTH as i32 - 2 * MARGIN,
            height: self.metrics.cell_h + 6,
        }
    }

    pub(super) fn button(&self, action: Action) -> Rect {
        let header = self.header();
        // Buttons stack from the right edge, the last action outermost.
        let right_of: i32 = Action::ALL[action.index() + 1..]
            .iter()
            .map(|other| self.button_w[other.index()] + GAP)
            .sum();
        let width = self.button_w[action.index()];
        Rect {
            x: header.right() - right_of - width,
            y: header.y,
            width,
            height: header.height,
        }
    }

    /// The control showing the open map, which opens the list.
    pub(super) fn selector(&self) -> Rect {
        let header = self.header();
        let first = self.button(Action::New);
        Rect {
            x: header.x,
            y: header.y,
            width: first.x - GAP - header.x,
            height: header.height,
        }
    }

    pub(super) fn row_height(&self) -> i32 {
        self.metrics.cell_h + 2
    }

    /// Rows of the list that fit below the selector.
    pub(super) fn visible_rows(&self) -> usize {
        let room = HEIGHT as i32 - self.selector().bottom() - MARGIN;
        usize::try_from(room / self.row_height()).unwrap_or(0)
    }

    /// The row `slot` places below the top of the list.
    pub(super) fn row(&self, slot: usize) -> Rect {
        let selector = self.selector();
        Rect {
            x: selector.x,
            y: selector.bottom() + slot as i32 * self.row_height(),
            // Wide enough for a name, an id and a size.
            width: self.header().width,
            height: self.row_height(),
        }
    }

    /// The part of the view under the header.
    pub(super) fn body(&self) -> Rect {
        let header = self.header();
        Rect {
            x: header.x,
            y: header.bottom() + MARGIN,
            width: header.width,
            height: HEIGHT as i32 - header.bottom() - 2 * MARGIN,
        }
    }

    /// The CREATE MAP button shown while there are no maps.
    pub(super) fn create_map(&self, label_cells: i32) -> Rect {
        let body = self.body();
        let width = (label_cells + 2) * self.metrics.cell_w;
        let height = self.metrics.cell_h + 6;
        // Two rows below the line that says there are no maps.
        Rect {
            x: body.x,
            y: body.y + 3 * self.metrics.cell_h,
            width,
            height,
        }
    }

    /// Finds what is at `at`. While the list is open, `list` holds the number
    /// of maps and the index of the first row shown, and otherwise it is
    /// `None`. `create_cells` is the width of the CREATE MAP label and is
    /// given only while there are no maps.
    pub(super) fn hit(
        &self,
        at: Point,
        list: Option<(usize, usize)>,
        create_cells: Option<i32>,
    ) -> Hit {
        if let Some((count, first)) = list {
            let shown = count.saturating_sub(first).min(self.visible_rows());
            for slot in 0..shown {
                if self.row(slot).contains(at) {
                    return Hit::Row(first + slot);
                }
            }
        }
        if self.selector().contains(at) {
            return Hit::Selector;
        }
        for action in Action::ALL {
            if self.button(action).contains(at) {
                return Hit::Action(action);
            }
        }
        if create_cells.is_some_and(|cells| self.create_map(cells).contains(at)) {
            return Hit::CreateMap;
        }
        Hit::Nothing
    }
}

/// The first row the list shows so that `highlighted` stays in view with
/// `visible` rows.
pub(super) fn first_visible(highlighted: usize, visible: usize) -> usize {
    (highlighted + 1).saturating_sub(visible.max(1))
}
