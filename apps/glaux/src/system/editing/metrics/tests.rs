use super::*;
use crate::system::test_font::{LAYOUT_FONT, columns as tabs};
use pretty_assertions::assert_eq;

fn document(text: &str) -> CodeDocument {
    CodeDocument::from_text("code/main.luau", text)
}

/// The text of each row `document` folds into at `columns`.
fn folded(text: &str, columns: usize) -> Vec<String> {
    let document = document(text);
    visual_rows(&document, 0, 0, Some(columns), tabs(4))
        .map(|row| row.text.to_owned())
        .collect()
}

#[test]
fn tabs_advance_to_the_next_stop() {
    let columns = |text: &str, tab: usize| -> Vec<usize> {
        cells(text, 0, 0, tabs(tab))
            .map(|cell| cell.column)
            .collect()
    };
    assert_eq!(columns("\ta\tb", 4), [0, 4, 5, 8]);
    assert_eq!(columns("\ta\tb", 2), [0, 2, 3, 4]);
    assert_eq!(columns("ab\tc", 8), [0, 1, 2, 8]);
    assert_eq!(line_width("ab\t", tabs(8)), 8);
    assert_eq!(line_width("", tabs(4)), 0);
}

#[test]
fn double_width_characters_take_two_columns() {
    // In the test font "✅" and "⭕" are double-width; the tab after "✅"
    // still stops at column 4.
    let text = "a✅\tb⭕";
    let got: Vec<_> = cells(text, 0, 0, tabs(4))
        .map(|c| (c.ch, c.byte, c.column, c.width))
        .collect();
    assert_eq!(
        got,
        [
            ('a', 0, 0, 1),
            ('✅', 1, 1, 2),
            ('\t', 4, 3, 1),
            ('b', 5, 4, 1),
            ('⭕', 6, 5, 2)
        ]
    );
    assert_eq!(line_width(text, tabs(4)), 7);
}

#[test]
fn a_double_width_character_that_would_cross_the_fold_starts_the_next_row() {
    assert_eq!(folded("ab✅", 3), ["ab", "✅"]);
    assert_eq!(folded("✅✅✅", 4), ["✅✅", "✅"]);
    // A row always holds its first character, even one wider than it.
    assert_eq!(folded("✅✅", 1), ["✅", "✅"]);
}

#[test]
fn positions_around_a_double_width_character_snap_to_its_edges() {
    let document = document("a✅b");
    let key = MetricsKey {
        tab_size: 4,
        wrap_columns: None,
    };
    let columns: Vec<_> = [0, 1, 4, 5]
        .map(|byte| spot(&document, byte, key, &LAYOUT_FONT).column)
        .into();
    assert_eq!(columns, [0, 1, 3, 4]);

    let row = visual_rows(&document, 0, 0, None, tabs(4)).next().unwrap();
    // Column 2 is the right half of "✅": its nearer edge is after it.
    let bytes: Vec<_> = [0, 1, 2, 3, 4]
        .map(|column| row.byte_near_column(column, tabs(4)))
        .into();
    assert_eq!(bytes, [0, 1, 4, 4, 5]);
}

#[test]
fn narrow_characters_take_one_cell_and_keep_their_bytes() {
    let text = "añ€\u{301}x";
    let cells: Vec<_> = cells(text, 10, 0, tabs(4)).collect();
    let got: Vec<_> = cells.iter().map(|c| (c.ch, c.byte, c.column)).collect();
    assert_eq!(
        got,
        [
            ('a', 10, 0),
            ('ñ', 11, 1),
            ('€', 13, 2),
            ('\u{301}', 16, 3),
            ('x', 18, 4)
        ]
    );
}

#[test]
fn wrapping_folds_after_the_last_blank_that_fits() {
    assert_eq!(folded("one two three", 9), ["one two ", "three"]);
    // Blanks at a fold stay on the row they follow.
    assert_eq!(folded("one   two", 4), ["one   ", "two"]);
}

#[test]
fn a_word_wider_than_the_row_is_cut_at_its_edge() {
    assert_eq!(folded("abcdefghij", 4), ["abcd", "efgh", "ij"]);
    // Indentation alone never makes a row.
    assert_eq!(folded("    abcdefgh", 6), ["    ab", "cdefgh"]);
}

#[test]
fn lines_that_fit_take_one_row() {
    assert_eq!(folded("abcd", 4), ["abcd"]);
    assert_eq!(folded("", 4), [""]);
    assert_eq!(folded("ab\n\ncd", 4), ["ab", "", "cd"]);
}

#[test]
fn folded_rows_keep_their_line_and_columns() {
    let document = document("x\none two three\ny");
    let rows: Vec<_> = visual_rows(&document, 1, 1, Some(5), tabs(4))
        .map(|row| (row.line, row.row_in_line, row.text, row.start.column))
        .collect();
    assert_eq!(
        rows,
        [(1, 1, "two ", 4), (1, 2, "three", 8), (2, 0, "y", 0)]
    );
}

#[test]
fn unfolded_rows_are_whole_lines_without_their_endings() {
    let document = document("\u{feff}a\r\nb\n");
    let rows: Vec<_> = visual_rows(&document, 0, 0, None, tabs(4))
        .map(|row| (row.text, row.start.byte))
        .collect();
    assert_eq!(rows, [("a", 3), ("b", 6), ("", 8)]);
}

#[test]
fn metrics_measure_the_widest_line_and_the_folded_rows() {
    let document = document("ab\none two three\n\tx");
    let flat = TextMetrics::measure(
        &document,
        MetricsKey {
            tab_size: 8,
            wrap_columns: None,
        },
        &LAYOUT_FONT,
    );
    assert_eq!((flat.widest(), flat.rows()), (13, 3));

    let wrapped = TextMetrics::measure(
        &document,
        MetricsKey {
            tab_size: 4,
            wrap_columns: Some(4),
        },
        &LAYOUT_FONT,
    );
    // "ab" | "one " "two " "thre" "e" | "\t" "x"
    assert_eq!(wrapped.rows(), 7);
    assert_eq!(wrapped.row_index(1, 2), 3);
    assert_eq!(wrapped.position(3), (1, 2));
    assert_eq!(wrapped.position(5), (2, 0));
    assert_eq!(wrapped.position(99), (2, 1));
    assert_eq!(
        wrapped.row_index(1, 99),
        4,
        "rows past a line's end stop at its last"
    );
}

#[test]
fn a_spot_counts_columns_with_tabs_expanded() {
    let document = document("x\n\tab\u{feff}c");
    let key = MetricsKey {
        tab_size: 4,
        wrap_columns: None,
    };
    let columns: Vec<_> = [2, 3, 4, 5, 8]
        .map(|byte| {
            let spot = spot(&document, byte, key, &LAYOUT_FONT);
            (spot.line, spot.column)
        })
        .into();
    // The mark inside the line is a character like any other.
    assert_eq!(columns, [(1, 0), (1, 4), (1, 5), (1, 6), (1, 7)]);
}

#[test]
fn a_position_where_a_line_folds_starts_the_next_row() {
    // Folded at 5 columns: "aaaa ", "bbbb".
    let document = document("aaaa bbbb");
    let key = MetricsKey {
        tab_size: 4,
        wrap_columns: Some(5),
    };
    let at = |byte| {
        let spot = spot(&document, byte, key, &LAYOUT_FONT);
        (spot.row_in_line, spot.row_start.byte, spot.column)
    };
    assert_eq!(at(4), (0, 0, 4));
    assert_eq!(at(5), (1, 5, 5));
    assert_eq!(at(9), (1, 5, 9));
}

#[test]
fn the_nearest_position_on_a_folded_row_stays_on_that_row() {
    let document = document("aaaa bbbb");
    let rows: Vec<_> = visual_rows(&document, 0, 0, Some(5), tabs(4)).collect();
    assert_eq!((rows[0].folds, rows[1].folds), (true, false));
    // Past the end of the first row: before its blank, not after it.
    assert_eq!(rows[0].byte_near_column(40, tabs(4)), 4);
    assert_eq!(rows[1].byte_near_column(40, tabs(4)), 9);
    assert_eq!(rows[1].byte_near_column(6, tabs(4)), 6);
}
