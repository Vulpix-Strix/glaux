//! What part of the object-tile grid the sprite editor's sheet shows, and
//! at what size.

use glaux_machine::sprite::{OBJ_GRID_COLUMNS, OBJ_GRID_ROWS, OBJ_TILE_WIDTH, obj_grid_tile};

/// How much the sheet enlarges tiles.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Zoom {
    #[default]
    X1,
    X2,
}

impl Zoom {
    pub fn scale(self) -> i32 {
        match self {
            Self::X1 => 1,
            Self::X2 => 2,
        }
    }

    /// Side of one tile on screen, in pixels.
    pub fn tile_size(self) -> i32 {
        OBJ_TILE_WIDTH as i32 * self.scale()
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::X1 => "1x",
            Self::X2 => "2x",
        }
    }

    /// The next larger zoom, if any.
    pub fn zoomed_in(self) -> Option<Self> {
        match self {
            Self::X1 => Some(Self::X2),
            Self::X2 => None,
        }
    }

    /// The next smaller zoom, if any.
    pub fn zoomed_out(self) -> Option<Self> {
        match self {
            Self::X1 => None,
            Self::X2 => Some(Self::X1),
        }
    }

    /// Rows one wheel notch scrolls.
    pub fn wheel_rows(self) -> i32 {
        match self {
            Self::X1 => 2,
            Self::X2 => 1,
        }
    }
}

/// How many whole tiles the sheet shows across and down. Never more than
/// the grid has.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Visible {
    pub columns: usize,
    pub rows: usize,
}

impl Visible {
    /// `columns`×`rows`, limited to the grid.
    pub fn new(columns: usize, rows: usize) -> Self {
        Self {
            columns: columns.min(OBJ_GRID_COLUMNS),
            rows: rows.min(OBJ_GRID_ROWS),
        }
    }
}

/// The grid column and row of `tile`.
pub fn grid_position(tile: usize) -> (usize, usize) {
    (tile % OBJ_GRID_COLUMNS, tile / OBJ_GRID_COLUMNS)
}

/// The top-left grid cell the sheet shows and the zoom it shows it at.
///
/// Scrolling is always clamped so the sheet never shows past the grid.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SheetView {
    top_row: usize,
    left_column: usize,
    zoom: Zoom,
}

impl SheetView {
    pub fn top_row(&self) -> usize {
        self.top_row
    }

    pub fn left_column(&self) -> usize {
        self.left_column
    }

    pub fn zoom(&self) -> Zoom {
        self.zoom
    }

    /// Switches to `zoom`, which shows `visible` tiles. The top-left cell
    /// stays where it was if it can, then `selected` is brought into view.
    pub fn set_zoom(&mut self, zoom: Zoom, visible: Visible, selected: usize) {
        self.zoom = zoom;
        self.clamp(visible);
        self.reveal(selected, visible);
    }

    /// Pulls the top-left cell back so that `visible` tiles from it stay on
    /// the grid.
    pub fn clamp(&mut self, visible: Visible) {
        self.top_row = self.top_row.min(OBJ_GRID_ROWS - visible.rows);
        self.left_column = self.left_column.min(OBJ_GRID_COLUMNS - visible.columns);
    }

    /// Scrolls down by `delta` rows, or up when negative.
    pub fn scroll_rows(&mut self, delta: i32, visible: Visible) {
        self.top_row = offset(self.top_row, delta, OBJ_GRID_ROWS - visible.rows);
    }

    /// Scrolls right by `delta` columns, or left when negative.
    pub fn scroll_columns(&mut self, delta: i32, visible: Visible) {
        self.left_column = offset(self.left_column, delta, OBJ_GRID_COLUMNS - visible.columns);
    }

    /// Whether `tile` is on screen.
    pub fn is_visible(&self, tile: usize, visible: Visible) -> bool {
        let (column, row) = grid_position(tile);
        (self.left_column..self.left_column + visible.columns).contains(&column)
            && (self.top_row..self.top_row + visible.rows).contains(&row)
    }

    /// Scrolls as little as possible to put `tile` on screen. A tile
    /// already on screen does not scroll the sheet.
    pub fn reveal(&mut self, tile: usize, visible: Visible) {
        let (column, row) = grid_position(tile);
        self.top_row = reveal_axis(self.top_row, row, visible.rows);
        self.left_column = reveal_axis(self.left_column, column, visible.columns);
        self.clamp(visible);
    }

    /// The tile `column` tiles right of and `row` tiles below the sheet's
    /// top-left tile, if that is on screen.
    pub fn tile_in_view(&self, column: usize, row: usize, visible: Visible) -> Option<usize> {
        if column >= visible.columns || row >= visible.rows {
            return None;
        }
        obj_grid_tile(0, self.left_column + column, self.top_row + row)
    }
}

/// `start` moved by `delta`, kept in `0..=max`.
fn offset(start: usize, delta: i32, max: usize) -> usize {
    let moved = start as i64 + i64::from(delta);
    // `max` is at most the grid size, so the result fits in usize.
    moved.clamp(0, max as i64) as usize
}

/// The first shown index of an axis showing `shown` cells from `start`,
/// moved as little as possible to include `target`.
fn reveal_axis(start: usize, target: usize, shown: usize) -> usize {
    if target < start {
        target
    } else if target >= start + shown {
        target + 1 - shown.max(1)
    } else {
        start
    }
}

#[cfg(test)]
mod tests;
