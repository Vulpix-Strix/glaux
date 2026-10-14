use proptest::prelude::*;

use super::*;
use crate::system::code_text::MetricsKey;
use crate::system::test_font::LAYOUT_FONT;
use pretty_assertions::assert_eq;

fn doc(text: &str) -> CodeDocument {
    CodeDocument::from_text("code/main.luau", text)
}

/// The positions `step` visits from `byte`, `count` times over.
fn walk(
    document: &CodeDocument,
    byte: usize,
    count: usize,
    step: impl Fn(Caret) -> Caret,
) -> Vec<usize> {
    let mut caret = Caret::at(document, byte);
    (0..count)
        .map(|_| {
            caret = step(caret);
            caret.byte()
        })
        .collect()
}

fn measure(document: &CodeDocument, wrap_columns: Option<usize>) -> TextMetrics {
    TextMetrics::measure(
        document,
        MetricsKey {
            tab_size: 4,
            wrap_columns,
        },
        &LAYOUT_FONT,
    )
}

#[test]
fn left_and_right_step_over_whole_characters_and_line_endings() {
    // a é 日 🦀 take 1, 2, 3 and 4 bytes. The CRLF is one step.
    let document = doc("aé日🦀\r\nb");
    assert_eq!(
        walk(&document, 0, 7, |caret| caret.right(&document)),
        [1, 3, 6, 10, 12, 13, 13]
    );
    assert_eq!(
        walk(&document, 13, 7, |caret| caret.left(&document)),
        [12, 10, 6, 3, 1, 0, 0]
    );
}

#[test]
fn the_caret_never_sits_inside_a_byte_order_mark_or_a_line_ending() {
    let document = doc("\u{feff}ab\r\nc");
    assert_eq!(Caret::default().reset(&document).byte(), 3);
    assert_eq!(Caret::at(&document, 1).byte(), 3);
    assert_eq!(Caret::document_start(&document).byte(), 3);
    assert_eq!(Caret::document_start(&document).left(&document).byte(), 3);
    // Between the CR and the LF is the end of the line's content.
    assert_eq!(Caret::at(&document, 6).byte(), 5);
    // Inside é: the start of the character.
    assert_eq!(Caret::at(&doc("é"), 1).byte(), 0);
}

#[test]
fn word_moves_pass_runs_of_word_characters_or_punctuation() {
    let document = doc("x\n  foo_bar.baz(qux) + 1\ny");
    // The middle line runs from byte 2 to 24.
    assert_eq!(
        walk(&document, 2, 9, |caret| caret.word_right(&document)),
        [11, 12, 15, 16, 19, 20, 22, 24, 25]
    );
    assert_eq!(
        walk(&document, 24, 10, |caret| caret.word_left(&document)),
        [23, 21, 19, 16, 15, 12, 11, 4, 2, 1]
    );
    let operators = doc("a == b");
    assert_eq!(
        walk(&operators, 1, 2, |caret| caret.word_right(&operators)),
        [4, 6]
    );
}

#[test]
fn word_moves_treat_multibyte_letters_as_word_characters() {
    let document = doc("año + ñu");
    assert_eq!(
        walk(&document, 0, 3, |caret| caret.word_right(&document)),
        [4, 6, 10]
    );
    assert_eq!(
        walk(&document, 10, 3, |caret| caret.word_left(&document)),
        [7, 5, 0]
    );
}

#[test]
fn home_alternates_between_the_indentation_and_the_line_start() {
    let document = doc("x\n \t local y\nz");
    assert_eq!(
        walk(&document, 9, 3, |caret| caret.home(&document)),
        [5, 2, 5]
    );
    assert_eq!(Caret::at(&document, 3).end(&document).byte(), 12);
    // End stops before the CRLF.
    let crlf = doc("ab\r\ncd");
    assert_eq!(Caret::at(&crlf, 0).end(&crlf).byte(), 2);
}

#[test]
fn document_ends_hold_for_empty_and_mark_only_files() {
    for (text, start, end) in [("", 0, 0), ("\u{feff}", 3, 3), ("a\r\n", 0, 3)] {
        let document = doc(text);
        assert_eq!(Caret::document_start(&document).byte(), start, "{text:?}");
        assert_eq!(Caret::document_end(&document).byte(), end, "{text:?}");
    }
    // Nothing moves anywhere in an empty file.
    let empty = doc("");
    let metrics = measure(&empty, None);
    let caret = Caret::default();
    for moved in [
        caret.left(&empty),
        caret.right(&empty),
        caret.word_left(&empty),
        caret.word_right(&empty),
        caret.home(&empty),
        caret.vertical(&empty, 1, &metrics, &LAYOUT_FONT),
        caret.vertical(&empty, -1, &metrics, &LAYOUT_FONT),
    ] {
        assert_eq!(moved.byte(), 0);
    }
}

#[test]
fn vertical_moves_keep_the_column_through_shorter_lines() {
    let document = doc("long line here\nab\nanother long");
    let metrics = measure(&document, None);
    let down = |caret: Caret| caret.vertical(&document, 1, &metrics, &LAYOUT_FONT);
    let up = |caret: Caret| caret.vertical(&document, -1, &metrics, &LAYOUT_FONT);
    let start = Caret::at(&document, 10);
    let moved = [down(start), down(down(start)), up(down(down(start)))];
    assert_eq!(moved.map(|caret| caret.byte()), [17, 28, 17]);
    assert_eq!(up(moved[2]).byte(), 10);
    assert_eq!(moved[2].preferred_column(), Some(10));
    // Any other move forgets the column.
    assert_eq!(moved[2].left(&document).preferred_column(), None);
}

#[test]
fn vertical_moves_count_columns_not_bytes() {
    let document = doc("日本語\nabc");
    let metrics = measure(&document, None);
    let caret = Caret::at(&document, 12).vertical(&document, -1, &metrics, &LAYOUT_FONT);
    assert_eq!(caret.byte(), 6);
}

#[test]
fn vertical_moves_cross_double_width_characters_by_their_columns() {
    // In the test font "✅" takes two columns and three bytes.
    let document = doc("abcd\n✅✅\nabcd");
    let metrics = measure(&document, None);
    let down = |caret: Caret| caret.vertical(&document, 1, &metrics, &LAYOUT_FONT);
    // Column 1 is the right half of the first "✅": its nearer edge is the
    // start of the second. Column 3 is the right half of the second, so the
    // line's end. The next line still gets column 3.
    assert_eq!(down(Caret::at(&document, 1)).byte(), 8);
    let middle = down(Caret::at(&document, 3));
    assert_eq!(middle.byte(), 11);
    assert_eq!(down(middle).byte(), 15);
}

#[test]
fn a_tab_gives_way_to_its_nearer_edge() {
    let document = doc("\tx\nabcdef");
    let metrics = measure(&document, None);
    let up_from = |byte| {
        Caret::at(&document, byte)
            .vertical(&document, -1, &metrics, &LAYOUT_FONT)
            .byte()
    };
    // The tab spans columns 0 to 4: column 1 is nearer its start, 3 its end.
    assert_eq!(up_from(4), 0);
    assert_eq!(up_from(6), 1);
}

#[test]
fn moving_past_the_first_or_last_row_reaches_the_document_ends() {
    let document = doc("abc\ndef\nghi");
    let metrics = measure(&document, None);
    let caret = Caret::at(&document, 5);
    assert_eq!(
        caret.vertical(&document, -1, &metrics, &LAYOUT_FONT).byte(),
        1
    );
    assert_eq!(
        caret
            .vertical(&document, -1, &metrics, &LAYOUT_FONT)
            .vertical(&document, -1, &metrics, &LAYOUT_FONT)
            .byte(),
        0
    );
    // A page down stops on the last row first, then reaches the end.
    let paged = caret.vertical(&document, 10, &metrics, &LAYOUT_FONT);
    assert_eq!(paged.byte(), 9);
    assert_eq!(
        paged.vertical(&document, 10, &metrics, &LAYOUT_FONT).byte(),
        11
    );
}

#[test]
fn vertical_moves_step_through_the_folds_of_a_wrapped_line() {
    // Folded at 5 columns: "aaaa ", "bbbb ", "cccc".
    let document = doc("aaaa bbbb cccc");
    let metrics = measure(&document, Some(5));
    assert_eq!(
        walk(&document, 2, 3, |caret| caret.vertical(
            &document,
            1,
            &metrics,
            &LAYOUT_FONT
        )),
        [7, 12, 14]
    );
    // Folded at 5 columns: "ab ", "cdefg", "h", then "xyzw". Past the end
    // of a fold the caret stays on that row, before its last character.
    let long = doc("ab cdefgh\nxyzw");
    let metrics = measure(&long, Some(5));
    assert_eq!(
        walk(&long, 14, 3, |caret| caret.vertical(
            &long,
            -1,
            &metrics,
            &LAYOUT_FONT
        )),
        [9, 7, 2]
    );
}

/// Characters chosen to hit every awkward case: line endings, a byte order
/// mark anywhere, tabs, multi-byte and unsupported glyphs.
fn awkward_text() -> impl Strategy<Value = String> {
    let pieces = prop::sample::select(vec![
        "a", "_", ".", " ", "\t", "\r", "\n", "\r\n", "é", "日", "🦀", "\u{feff}",
    ]);
    prop::collection::vec(pieces, 0..40).prop_map(|pieces| pieces.concat())
}

proptest! {
    #[test]
    fn every_move_lands_on_a_valid_position(
        text in awkward_text(),
        start in 0usize..200,
        moves in prop::collection::vec(0u8..10, 1..30),
        wrap in prop::option::of(1usize..8),
    ) {
        let document = doc(&text);
        let metrics = measure(&document, wrap);
        let mut caret = Caret::at(&document, start);
        for step in moves {
            caret = match step {
                0 => caret.left(&document),
                1 => caret.right(&document),
                2 => caret.word_left(&document),
                3 => caret.word_right(&document),
                4 => caret.home(&document),
                5 => caret.end(&document),
                6 => caret.vertical(&document, 1, &metrics, &LAYOUT_FONT),
                7 => caret.vertical(&document, -1, &metrics, &LAYOUT_FONT),
                8 => caret.vertical(&document, 3, &metrics, &LAYOUT_FONT),
                _ => caret.vertical(&document, -3, &metrics, &LAYOUT_FONT),
            };
            let byte = caret.byte();
            let range = document.line_range(document.line_of(byte)).unwrap();
            prop_assert!(text.is_char_boundary(byte));
            prop_assert!(range.contains(&byte) || byte == range.end, "{byte} outside {range:?}");
        }
    }
}

#[test]
fn a_word_is_the_run_of_one_kind_of_character_at_a_position() {
    let document = doc("  foo_bar.baz(qux) + 1\n");
    let word = |byte| word_at(&document, byte);
    assert_eq!(word(2), 2..9);
    assert_eq!(word(5), 2..9);
    // At a run's end, the character after the position decides.
    assert_eq!(word(9), 9..10);
    assert_eq!(word(0), 0..2);
    assert_eq!(word(18), 18..19);
    // At the line's end, the character before it.
    assert_eq!(word(22), 21..22);
    // An empty line has no word.
    assert_eq!(word(23), 23..23);
}

#[test]
fn words_hold_whole_multibyte_and_wide_characters() {
    // é is 2 bytes, 日本 are 3 each and two cells wide. ✅ is punctuation.
    let document = doc("é日本 ✅✅x");
    assert_eq!(word_at(&document, 2), 0..8);
    // Inside 日: the start of the character.
    assert_eq!(word_at(&document, 3), 0..8);
    assert_eq!(word_at(&document, 9), 9..15);
}

#[test]
fn a_line_runs_through_its_line_ending() {
    let document = doc("\u{feff}ab\r\ncd\nef");
    assert_eq!(line_at(&document, 4), 3..7);
    assert_eq!(line_at(&document, 8), 7..10);
    // The last line has no ending.
    assert_eq!(line_at(&document, 11), 10..12);
    let trailing = doc("ab\n");
    assert_eq!(line_at(&trailing, 1), 0..3);
    assert_eq!(line_at(&trailing, 3), 3..3);
}
