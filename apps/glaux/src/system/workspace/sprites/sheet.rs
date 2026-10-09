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
mod tests {
    use super::*;
    use glaux_machine::sprite::OBJ_TILE_CAPACITY;
    use pretty_assertions::assert_eq;

    const ONE_X: Visible = Visible {
        columns: 16,
        rows: 22,
    };
    const TWO_X: Visible = Visible {
        columns: 11,
        rows: 11,
    };

    fn view(top_row: usize, left_column: usize, zoom: Zoom) -> SheetView {
        SheetView {
            top_row,
            left_column,
            zoom,
        }
    }

    #[test]
    fn tiles_sit_on_the_sixteen_by_sixty_four_grid() {
        assert_eq!(grid_position(0), (0, 0));
        assert_eq!(grid_position(15), (15, 0));
        assert_eq!(grid_position(16), (0, 1));
        assert_eq!(grid_position(12), (12, 0));
        assert_eq!(grid_position(OBJ_TILE_CAPACITY - 1), (15, 63));

        let sheet = view(0, 0, Zoom::X1);
        assert_eq!(sheet.tile_in_view(15, 0, ONE_X), Some(15));
        assert_eq!(sheet.tile_in_view(0, 1, ONE_X), Some(16));
        let bottom = view(42, 0, Zoom::X1);
        assert_eq!(bottom.tile_in_view(15, 21, ONE_X), Some(1023));
    }

    #[test]
    fn cells_outside_the_viewport_hold_no_tile() {
        let sheet = view(0, 0, Zoom::X2);
        assert_eq!(sheet.tile_in_view(11, 0, TWO_X), None);
        assert_eq!(sheet.tile_in_view(0, 11, TWO_X), None);
        assert_eq!(view(0, 5, Zoom::X2).tile_in_view(10, 0, TWO_X), Some(15));
    }

    #[test]
    fn row_scrolling_stops_at_the_top_and_bottom() {
        let mut sheet = SheetView::default();
        sheet.scroll_rows(-3, ONE_X);
        assert_eq!(sheet.top_row(), 0);
        sheet.scroll_rows(1000, ONE_X);
        assert_eq!(sheet.top_row(), 64 - 22);
        sheet.scroll_rows(-2, ONE_X);
        assert_eq!(sheet.top_row(), 40);

        let mut sheet = view(0, 0, Zoom::X2);
        sheet.scroll_rows(i32::MAX, TWO_X);
        assert_eq!(sheet.top_row(), 64 - 11);
        sheet.scroll_rows(i32::MIN, TWO_X);
        assert_eq!(sheet.top_row(), 0);
    }

    #[test]
    fn panning_is_clamped_at_two_x_and_does_nothing_at_one_x() {
        let mut sheet = view(0, 0, Zoom::X1);
        sheet.scroll_columns(3, ONE_X);
        assert_eq!(sheet.left_column(), 0);

        let mut sheet = view(0, 0, Zoom::X2);
        sheet.scroll_columns(3, TWO_X);
        assert_eq!(sheet.left_column(), 3);
        sheet.scroll_columns(100, TWO_X);
        assert_eq!(sheet.left_column(), 16 - 11);
        sheet.scroll_columns(-100, TWO_X);
        assert_eq!(sheet.left_column(), 0);
    }

    #[test]
    fn revealing_a_visible_tile_does_not_scroll() {
        let mut sheet = view(10, 2, Zoom::X2);
        for tile in [10 * 16 + 2, 20 * 16 + 12, 15 * 16 + 7] {
            sheet.reveal(tile, TWO_X);
            assert_eq!(sheet, view(10, 2, Zoom::X2), "tile {tile}");
        }
    }

    #[test]
    fn revealing_an_offscreen_tile_scrolls_the_least_needed() {
        let mut sheet = view(10, 2, Zoom::X2);
        sheet.reveal(21 * 16 + 2, TWO_X);
        assert_eq!((sheet.top_row(), sheet.left_column()), (11, 2));
        sheet.reveal(5 * 16 + 15, TWO_X);
        assert_eq!((sheet.top_row(), sheet.left_column()), (5, 5));
        sheet.reveal(5 * 16, TWO_X);
        assert_eq!((sheet.top_row(), sheet.left_column()), (5, 0));
        sheet.reveal(1023, TWO_X);
        assert_eq!((sheet.top_row(), sheet.left_column()), (53, 5));
    }

    #[test]
    fn zooming_keeps_the_corner_when_it_can_and_shows_the_selection() {
        let mut sheet = view(30, 0, Zoom::X1);
        sheet.set_zoom(Zoom::X2, TWO_X, 31 * 16 + 3);
        assert_eq!(sheet, view(30, 0, Zoom::X2));

        sheet.set_zoom(Zoom::X2, TWO_X, 30 * 16 + 14);
        assert_eq!(sheet, view(30, 4, Zoom::X2));

        let mut sheet = view(53, 5, Zoom::X2);
        sheet.set_zoom(Zoom::X1, ONE_X, 1023);
        assert_eq!(sheet, view(42, 0, Zoom::X1));
    }

    #[test]
    fn a_new_viewport_size_only_clamps_the_corner() {
        let mut sheet = view(40, 0, Zoom::X1);
        sheet.clamp(Visible::new(16, 18));
        assert_eq!(sheet, view(40, 0, Zoom::X1));
        let mut sheet = view(42, 0, Zoom::X1);
        sheet.clamp(Visible::new(16, 18));
        assert_eq!(sheet, view(42, 0, Zoom::X1));
        sheet.clamp(Visible::new(16, 30));
        assert_eq!(sheet, view(34, 0, Zoom::X1));
    }

    #[test]
    fn visible_tiles_never_exceed_the_grid() {
        assert_eq!(Visible::new(40, 100), Visible::new(16, 64));
    }

    #[test]
    fn zoom_steps_between_one_x_and_two_x() {
        assert_eq!(Zoom::X1.zoomed_in(), Some(Zoom::X2));
        assert_eq!(Zoom::X2.zoomed_in(), None);
        assert_eq!(Zoom::X2.zoomed_out(), Some(Zoom::X1));
        assert_eq!(Zoom::X1.zoomed_out(), None);
        assert_eq!((Zoom::X1.tile_size(), Zoom::X2.tile_size()), (8, 16));
    }
}
