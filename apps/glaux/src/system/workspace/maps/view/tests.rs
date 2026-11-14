use super::*;
use pretty_assertions::assert_eq;

const CANVAS: Rect = Rect {
    x: 20,
    y: 50,
    width: 200,
    height: 100,
};
const AREA: (i32, i32) = (CANVAS.width, CANVAS.height);

#[test]
fn zoom_steps_between_the_three_levels_and_stops_at_the_ends() {
    assert_eq!(Zoom::default(), Zoom::X2);
    assert_eq!(Zoom::X1.zoomed_out(), None);
    assert_eq!(Zoom::X1.zoomed_in(), Some(Zoom::X2));
    assert_eq!(Zoom::X2.zoomed_in(), Some(Zoom::X4));
    assert_eq!(Zoom::X4.zoomed_in(), None);
    assert_eq!(Zoom::X4.zoomed_out(), Some(Zoom::X2));
    assert_eq!(
        [Zoom::X1, Zoom::X2, Zoom::X4].map(Zoom::tile_px),
        [8, 16, 32]
    );
}

#[test]
fn the_offset_stays_inside_the_map_at_the_largest_size() {
    let map = (256, 256);
    let mut view = Viewport::default();
    view.scroll_by(-50, -50, map, AREA);
    assert_eq!(view.offset(), (0, 0));
    view.scroll_by(1_000_000, 1_000_000, map, AREA);
    // 256 tiles of 16 pixels, less what the canvas shows.
    assert_eq!(view.offset(), (4096 - 200, 4096 - 100));
    view.scroll_by(i32::MAX, i32::MAX, map, AREA);
    assert_eq!(view.offset(), (4096 - 200, 4096 - 100), "no overflow");
}

#[test]
fn a_map_smaller_than_the_canvas_stays_in_the_corner() {
    let mut view = Viewport::default();
    view.scroll_by(500, 500, (4, 3), AREA);
    assert_eq!(view.offset(), (0, 0));
    view.scroll_tiles(3, 3, (4, 3), AREA);
    assert_eq!(view.offset(), (0, 0));
}

#[test]
fn scrolling_by_tiles_moves_whole_tiles_at_the_current_zoom() {
    let mut view = Viewport::default();
    view.scroll_tiles(2, 1, (64, 64), AREA);
    assert_eq!(view.offset(), (32, 16));
    view.set_zoom(Zoom::X4, (64, 64), AREA);
    let before = view.offset();
    view.scroll_tiles(1, 0, (64, 64), AREA);
    assert_eq!(view.offset().0 - before.0, 32);
}

#[test]
fn zooming_keeps_the_middle_of_the_canvas_where_it_was() {
    let map = (256, 256);
    let mut view = Viewport::default();
    view.scroll_by(1000, 800, map, AREA);
    let middle = |view: Viewport| {
        let tile = view.zoom().tile_px();
        (
            (view.offset().0 + AREA.0 / 2) * TILE / tile,
            (view.offset().1 + AREA.1 / 2) * TILE / tile,
        )
    };
    let before = middle(view);
    view.set_zoom(Zoom::X4, map, AREA);
    let after = middle(view);
    assert!((before.0 - after.0).abs() <= 1 && (before.1 - after.1).abs() <= 1);
    view.set_zoom(Zoom::X1, map, AREA);
    let back = middle(view);
    assert!((before.0 - back.0).abs() <= 1 && (before.1 - back.1).abs() <= 1);
}

#[test]
fn zooming_out_of_a_small_map_clamps_the_offset() {
    let mut view = Viewport::default();
    view.scroll_by(100, 100, (30, 30), AREA);
    view.set_zoom(Zoom::X1, (30, 30), AREA);
    // 30 tiles of 8 pixels are 240 wide and 240 high: room to scroll by 40
    // and 140.
    assert!(view.offset().0 <= 40 && view.offset().1 <= 140);
}

#[test]
fn a_screen_position_maps_to_the_cell_under_it_even_past_the_map() {
    let mut view = Viewport::default();
    view.scroll_by(40, 24, (64, 64), AREA);
    // The offset of 40 pixels puts the map's cell 2 at screen x 12 to 27.
    assert_eq!(view.cell_at(CANVAS, (20, 50)), (2, 1));
    assert_eq!(view.cell_at(CANVAS, (20 + 7, 50 + 7)), (2, 1));
    assert_eq!(view.cell_at(CANVAS, (20 + 8, 50)), (3, 1));
    assert_eq!(view.cell_at(CANVAS, (20 + 16, 50)), (3, 1));
    assert_eq!(view.cell_at(CANVAS, (0, 0)), (1, -2));
    let mut corner = Viewport::default();
    corner.clamp((64, 64), AREA);
    assert_eq!(corner.cell_at(CANVAS, (0, 0)), (-2, -4));
}

#[test]
fn a_cell_rect_is_where_cell_at_finds_the_cell() {
    let mut view = Viewport::default();
    view.scroll_by(37, 21, (64, 64), AREA);
    for cell in [(2, 1), (3, 2), (10, 5)] {
        let rect = view.cell_rect(CANVAS, cell);
        assert_eq!((rect.width, rect.height), (16, 16));
        assert_eq!(view.cell_at(CANVAS, (rect.x, rect.y)), cell);
        assert_eq!(
            view.cell_at(CANVAS, (rect.x + 15, rect.y + 15)),
            cell,
            "the far corner is inside too"
        );
    }
}

#[test]
fn the_visible_cells_cover_the_canvas_and_stay_in_the_map() {
    let mut view = Viewport::default();
    // 200 x 100 pixels hold 12.5 x 6.25 tiles of 16.
    assert_eq!(view.visible_cells((64, 64), AREA), ((0, 0), (13, 7)));
    view.scroll_by(40, 24, (64, 64), AREA);
    assert_eq!(view.visible_cells((64, 64), AREA), ((2, 1), (13, 7)));
    assert_eq!(
        Viewport::default().visible_cells((4, 3), AREA),
        ((0, 0), (4, 3))
    );
    let mut far = Viewport::default();
    far.scroll_by(10_000, 10_000, (64, 64), AREA);
    let (first, count) = far.visible_cells((64, 64), AREA);
    assert!(first.0 + count.0 <= 64 && first.1 + count.1 <= 64);
    assert_eq!(first.0 + count.0, 64);
}
