//! Where the Map view's empty state sits.

use glaux_input::Point;
use glaux_machine::framebuffer::{HEIGHT, WIDTH};

use super::Metrics;
use super::frame::BAR;
use crate::system::workspace::Rect;

/// Space between the view's edge and its controls.
const MARGIN: i32 = 4;

/// An operation the selector's menu offers after the maps.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Action {
    New,
    Rename,
    Delete,
}

impl Action {
    pub(super) const ALL: [Self; 3] = [Self::New, Self::Rename, Self::Delete];

    pub(super) fn message_id(self) -> &'static str {
        match self {
            Self::New => "maps-new",
            Self::Rename => "maps-rename",
            Self::Delete => "maps-delete",
        }
    }
}

/// What a point of the view is over.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Hit {
    /// The CREATE MAP button of the empty state.
    CreateMap,
    Nothing,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Layout {
    metrics: Metrics,
}

impl Layout {
    pub(super) const fn new(metrics: Metrics) -> Self {
        Self { metrics }
    }

    /// The bar along the bottom of the view.
    pub(super) fn status(&self) -> Rect {
        Rect {
            x: 0,
            y: HEIGHT as i32 - BAR,
            width: WIDTH as i32,
            height: BAR,
        }
    }

    /// The part of the view under the options bar, where the empty state
    /// goes.
    pub(super) fn body(&self) -> Rect {
        let top = self.metrics.top + BAR + 2;
        Rect {
            x: MARGIN,
            y: top,
            width: WIDTH as i32 - 2 * MARGIN,
            height: self.status().y - MARGIN - top,
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

    /// Finds what is at `at`. `create_cells` is the width of the CREATE MAP
    /// label and is given only while there are no maps.
    pub(super) fn hit(&self, at: Point, create_cells: Option<i32>) -> Hit {
        if create_cells.is_some_and(|cells| self.create_map(cells).contains(at)) {
            return Hit::CreateMap;
        }
        Hit::Nothing
    }
}
