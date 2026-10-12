use super::*;
use crate::system::code_document::CodeDocument;
use crate::system::workspace::code::layout::Metrics;
use pretty_assertions::assert_eq;

fn layout() -> CodeLayout {
    CodeLayout::new(Metrics {
        cell_w: 6,
        cell_h: 10,
        top: 16,
    })
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
    let metrics = TextMetrics::measure(document, geometry.metrics_key());
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
    assert_eq!(geometry.rows, 18);
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
    assert_eq!(down, view(30 - 18, 0, 0));
    assert_eq!(geometry.scroll_rows(down, -3, &metrics), view(9, 0, 0));
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
fn horizontal_scrolling_stops_when_the_widest_line_ends_at_the_edge() {
    let document = document(3, 100);
    let (geometry, metrics) = measured(&document, &settings(false, true));
    let right = geometry.scroll_columns(view(0, 0, 0), 500, &metrics);
    assert_eq!(right.left_column, 100 - geometry.columns);
    assert_eq!(geometry.scroll_columns(right, -6, &metrics).left_column, 46);
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
    // The bottom stops at row 30 - 18 = 12, the first row of line 4.
    assert_eq!(geometry.scroll_rows(moved, 100, &metrics), view(4, 0, 0));
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
