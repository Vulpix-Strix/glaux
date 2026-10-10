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
