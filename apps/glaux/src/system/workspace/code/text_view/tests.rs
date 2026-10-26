use super::*;
use crate::system::editing::document::CodeDocument;
use crate::system::test_font::LAYOUT_FONT;
use pretty_assertions::assert_eq;

fn layout() -> CodeLayout {
    CodeLayout::new(
        crate::system::workspace::tests_support::CODE_METRICS,
        LAYOUT_FONT.clone(),
    )
}

fn settings(word_wrap: bool, line_numbers: bool) -> EditorSettings {
    EditorSettings {
        word_wrap,
        line_numbers,
        ..EditorSettings::default()
    }
}

/// A document of `lines` lines, each `width` columns of `x`.
fn document(lines: usize, width: usize) -> CodeDocument {
    let line = "x".repeat(width);
    let text = vec![line; lines].join("\n");
    CodeDocument::from_text("code/main.luau", &text)
}

fn view(top_line: usize, top_row: usize, left_column: usize) -> DocumentView {
    DocumentView {
        top_line,
        top_row,
        left_column,
    }
}

/// The geometry and metrics for `document` shown with `settings`.
fn measured(document: &CodeDocument, settings: &EditorSettings) -> (TextGeometry, TextMetrics) {
    let geometry = TextGeometry::new(&layout(), document.line_count(), settings);
    let metrics = TextMetrics::measure(document, geometry.metrics_key(), &LAYOUT_FONT);
    (geometry, metrics)
}

#[test]
fn the_gutter_widens_with_the_digits_of_the_last_line_number() {
    let shown = settings(false, true);
    let at = |lines| TextGeometry::new(&layout(), lines, &shown);
    let widths: Vec<_> = [1, 99, 100, 1000]
        .map(|lines| (at(lines).gutter.unwrap().width, at(lines).columns))
        .into();
    assert_eq!(widths, [(20, 48), (20, 48), (26, 47), (32, 46)]);
    // The text starts after the gutter and its rule, on whole cells.
    let geometry = at(100);
    assert_eq!(geometry.viewport.x, geometry.gutter.unwrap().right() + 4);
    assert_eq!(geometry.viewport.width, 47 * 6);
}

#[test]
fn hidden_line_numbers_give_their_room_to_the_text() {
    let geometry = TextGeometry::new(&layout(), 1000, &settings(false, false));
    assert_eq!(geometry.gutter, None);
    assert_eq!((geometry.viewport.x, geometry.columns), (4, 52));
}

#[test]
fn text_and_indicators_never_overlap() {
    let geometry = TextGeometry::new(&layout(), 10, &settings(false, true));
    let area = layout().text;
    // 182 pixels between the options and status bars, less the top pixel
    // and the horizontal indicator, hold 14 rows of 12.
    assert_eq!(geometry.rows, 14);
    assert!(geometry.viewport.right() < geometry.vertical.x);
    assert!(geometry.viewport.bottom() < geometry.horizontal.y);
    assert!(geometry.vertical.bottom() < geometry.horizontal.y);
    assert_eq!(geometry.vertical.right(), area.right());
    assert_eq!(geometry.horizontal.bottom(), area.bottom());
}

#[test]
fn vertical_scrolling_stops_when_the_last_line_reaches_the_bottom() {
    let document = document(30, 10);
    let (geometry, metrics) = measured(&document, &settings(false, true));
    let down = geometry.scroll_rows(view(0, 0, 0), 100, &metrics);
    assert_eq!(down, view(30 - 14, 0, 0));
    assert_eq!(geometry.scroll_rows(down, -3, &metrics), view(13, 0, 0));
    assert_eq!(geometry.scroll_rows(down, -100, &metrics), view(0, 0, 0));
}

#[test]
fn a_document_shorter_than_the_view_does_not_scroll() {
    let document = document(5, 10);
    let (geometry, metrics) = measured(&document, &settings(false, true));
    assert_eq!(
        geometry.scroll_rows(view(0, 0, 0), 3, &metrics),
        view(0, 0, 0)
    );
    assert_eq!(
        geometry.scroll_columns(view(0, 0, 0), 6, &metrics),
        view(0, 0, 0)
    );
}

#[test]
fn horizontal_scrolling_stops_one_column_past_the_widest_line() {
    // The column after the widest line holds the caret at its end.
    let document = document(3, 100);
    let (geometry, metrics) = measured(&document, &settings(false, true));
    let right = geometry.scroll_columns(view(0, 0, 0), 500, &metrics);
    assert_eq!(right.left_column, 101 - geometry.columns);
    assert_eq!(geometry.scroll_columns(right, -6, &metrics).left_column, 47);
    assert_eq!(
        geometry.scroll_columns(right, -500, &metrics).left_column,
        0
    );
}

#[test]
fn folded_lines_scroll_by_rows_across_lines_and_never_sideways() {
    // Ten lines of 100 columns, three rows each at 48 columns.
    let document = document(10, 100);
    let (geometry, metrics) = measured(&document, &settings(true, true));
    assert_eq!(geometry.columns, 48);
    let moved = geometry.scroll_rows(view(0, 0, 0), 4, &metrics);
    assert_eq!(moved, view(1, 1, 0));
    assert_eq!(geometry.scroll_rows(moved, -2, &metrics), view(0, 2, 0));
    // The bottom stops at row 30 - 14 = 16, the second row of line 5.
    assert_eq!(geometry.scroll_rows(moved, 100, &metrics), view(5, 1, 0));
    assert_eq!(geometry.scroll_columns(moved, 6, &metrics), moved);
}

#[test]
fn switching_wrapping_keeps_the_top_line() {
    let document = document(40, 100);
    let unfolded = settings(false, true);
    let folded = settings(true, true);

    let (geometry, metrics) = measured(&document, &folded);
    assert_eq!(geometry.clamp(view(5, 0, 30), &metrics), view(5, 0, 0));

    let (geometry, metrics) = measured(&document, &unfolded);
    assert_eq!(geometry.clamp(view(5, 2, 0), &metrics), view(5, 0, 0));
}

#[test]
fn no_thumb_while_everything_is_visible() {
    assert_eq!(thumb(0, 18, 18, 100), None);
    assert_eq!(thumb(0, 18, 5, 100), None);
}

#[test]
fn the_thumb_spans_the_visible_share_and_meets_both_track_ends() {
    assert_eq!(thumb(0, 25, 100, 100), Some((0, 25)));
    assert_eq!(thumb(75, 25, 100, 100), Some((75, 25)));
    assert_eq!(
        thumb(500, 25, 100, 100),
        Some((75, 25)),
        "offsets past the end"
    );
}

#[test]
fn the_thumb_never_shrinks_out_of_sight() {
    assert_eq!(thumb(0, 18, 100_000, 100), Some((0, 4)));
    assert_eq!(thumb(100_000 - 18, 18, 100_000, 100), Some((96, 4)));
}

/// Where byte `byte` of `document` shows in `geometry`.
fn spot_of(document: &CodeDocument, geometry: &TextGeometry, byte: usize) -> Spot {
    crate::system::editing::metrics::spot(document, byte, geometry.metrics_key(), &LAYOUT_FONT)
}

/// Whether the 1×`cell_h` caret drawn from `point` stays in the viewport.
fn caret_inside(geometry: &TextGeometry, (x, y): (i32, i32)) -> bool {
    geometry.viewport.contains_xy(x, y) && geometry.viewport.contains_xy(x, y + geometry.cell_h - 1)
}

#[test]
fn revealing_the_caret_scrolls_only_as_far_as_it_must() {
    // 100 lines of 100 columns: byte `l * 101 + c` is line l, column c.
    let document = document(100, 100);
    let (geometry, metrics) = measured(&document, &settings(false, false));
    let rows = geometry.rows;
    let reveal = |view, byte| geometry.reveal(view, &spot_of(&document, &geometry, byte), &metrics);

    let below = reveal(view(0, 0, 0), 30 * 101);
    assert_eq!(below, view(30 + 1 - rows, 0, 0));
    assert_eq!(reveal(below, 5 * 101), view(5, 0, 0));
    // Already in sight: nothing moves.
    assert_eq!(reveal(view(5, 0, 0), 6 * 101 + 3), view(5, 0, 0));

    // After the last character of a line wider than the view, with room
    // for the caret itself.
    let right = reveal(view(0, 0, 0), 100);
    assert_eq!(right, view(0, 0, 101 - geometry.columns));
    let point = geometry.caret_point(right, &spot_of(&document, &geometry, 100), &metrics);
    assert!(caret_inside(&geometry, point.unwrap()), "{point:?}");
    assert_eq!(reveal(right, 3), view(0, 0, 3));
}

#[test]
fn a_caret_scrolled_out_of_sight_is_not_drawn() {
    let document = document(100, 100);
    let (geometry, metrics) = measured(&document, &settings(false, false));
    let at =
        |view, byte| geometry.caret_point(view, &spot_of(&document, &geometry, byte), &metrics);
    assert_eq!(at(view(10, 0, 0), 0), None);
    assert_eq!(at(view(0, 0, 10), 5), None);
    assert_eq!(at(view(0, 0, 0), geometry.columns), None);
    assert_eq!(
        at(view(0, 0, 0), 101 + 2),
        Some((
            geometry.viewport.x + 2 * geometry.cell_w,
            geometry.viewport.y + geometry.cell_h
        ))
    );
}

#[test]
fn a_caret_after_a_full_folded_row_stays_inside_the_viewport() {
    let shown = settings(true, true);
    let geometry = TextGeometry::new(&layout(), 1, &shown);
    // Exactly as wide as the view: one row, with the caret after it.
    let document = document(1, geometry.columns);
    let (geometry, metrics) = measured(&document, &shown);
    let end = spot_of(&document, &geometry, geometry.columns);
    assert_eq!(
        geometry.reveal(view(0, 0, 0), &end, &metrics),
        view(0, 0, 0)
    );
    let point = geometry.caret_point(view(0, 0, 0), &end, &metrics).unwrap();
    assert_eq!(point.0, geometry.viewport.right() - 1);
    assert!(caret_inside(&geometry, point));
}

#[test]
fn a_click_picks_the_nearest_character_edge_on_its_row() {
    let document = CodeDocument::from_text("code/main.luau", "abcdef\n\tx\nlast");
    let (geometry, metrics) = measured(&document, &settings(false, true));
    let (x0, y0) = (geometry.viewport.x, geometry.viewport.y);
    let click = |x: i32, y: i32| {
        geometry.hit(
            view(0, 0, 0),
            Point::new(x as u16, y as u16),
            &document,
            &metrics,
            &LAYOUT_FONT,
        )
    };
    // Left of a cell's middle is before it; right of it, after.
    assert_eq!(click(x0 + 2 * 6 + 2, y0 + 5), Some(2));
    assert_eq!(click(x0 + 2 * 6 + 3, y0 + 5), Some(3));
    // Past the end, over the gutter, and below the last line.
    assert_eq!(click(x0 + 40 * 6, y0 + 5), Some(6));
    assert_eq!(click(2, y0 + 15), Some(7));
    assert_eq!(click(x0, y0 + 15 * 10), Some(document.text().len()));
    // The tab spans columns 0 to 4: a click at column 1 is before it, at
    // column 3 after it.
    assert_eq!(click(x0 + 6, y0 + 15), Some(7));
    assert_eq!(click(x0 + 3 * 6, y0 + 15), Some(8));
}

#[test]
fn double_width_characters_place_the_caret_and_clicks_on_their_edges() {
    // In the test font "✅" takes two cells and three bytes.
    let document = CodeDocument::from_text("code/main.luau", "✅x");
    let (geometry, metrics) = measured(&document, &settings(false, false));
    let (x0, y0) = (geometry.viewport.x, geometry.viewport.y);
    let caret = |byte| {
        geometry.caret_point(
            view(0, 0, 0),
            &spot_of(&document, &geometry, byte),
            &metrics,
        )
    };
    assert_eq!(caret(3), Some((x0 + 2 * 6, y0)));
    assert_eq!(caret(4), Some((x0 + 3 * 6, y0)));

    let click = |x: i32| {
        let at = Point::new(x as u16, (y0 + 5) as u16);
        geometry.hit(view(0, 0, 0), at, &document, &metrics, &LAYOUT_FONT)
    };
    // Either half of the glyph gives the edge nearer the click.
    assert_eq!(click(x0 + 4), Some(0));
    assert_eq!(click(x0 + 7), Some(3));
    assert_eq!(click(x0 + 2 * 6 + 1), Some(3));
}

#[test]
fn a_click_follows_the_scrolled_view() {
    let document = document(100, 100);
    let (geometry, metrics) = measured(&document, &settings(true, false));
    let point = Point::new(geometry.viewport.x as u16, geometry.viewport.y as u16 + 5);
    // Each line folds into two rows: row 3 of the view is line 41's
    // second fold.
    let top = view(40, 0, 0);
    let at = Point::new(point.x, point.y + 3 * geometry.cell_h as u16);
    let hit = geometry.hit(top, at, &document, &metrics, &LAYOUT_FONT);
    assert_eq!(hit, Some(41 * 101 + geometry.columns));
}
