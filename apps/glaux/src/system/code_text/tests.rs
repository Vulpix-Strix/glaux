use super::*;
use pretty_assertions::assert_eq;

fn document(text: &str) -> CodeDocument {
    CodeDocument::from_text("code/main.luau", text)
}

/// The text of each row `document` folds into at `columns`.
fn folded(text: &str, columns: usize) -> Vec<String> {
    let document = document(text);
    visual_rows(&document, 0, 0, Some(columns), 4)
        .map(|row| row.text.to_owned())
        .collect()
}

#[test]
fn tabs_advance_to_the_next_stop() {
    let columns = |text: &str, tab: usize| -> Vec<usize> {
        cells(text, 0, 0, tab).map(|cell| cell.column).collect()
    };
    assert_eq!(columns("\ta\tb", 4), [0, 4, 5, 8]);
    assert_eq!(columns("\ta\tb", 2), [0, 2, 3, 4]);
    assert_eq!(columns("ab\tc", 8), [0, 1, 2, 8]);
    assert_eq!(line_width("ab\t", 8), 8);
    assert_eq!(line_width("", 4), 0);
}

#[test]
fn every_character_takes_one_cell_and_keeps_its_bytes() {
    let text = "añ€\u{301}x";
    let cells: Vec<_> = cells(text, 10, 0, 4).collect();
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
    let rows: Vec<_> = visual_rows(&document, 1, 1, Some(5), 4)
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
    let rows: Vec<_> = visual_rows(&document, 0, 0, None, 4)
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
    );
    assert_eq!((flat.widest(), flat.rows()), (13, 3));

    let wrapped = TextMetrics::measure(
        &document,
        MetricsKey {
            tab_size: 4,
            wrap_columns: Some(4),
        },
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
