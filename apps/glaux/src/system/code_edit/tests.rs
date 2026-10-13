use proptest::prelude::*;

use super::*;
use crate::system::code_cursor::Caret;
use crate::system::code_document::EditError;
use crate::system::test_font::LAYOUT_FONT;
use pretty_assertions::assert_eq;

fn doc(text: &str) -> CodeDocument {
    CodeDocument::from_text("code/main.luau", text)
}

/// The caret at `byte` with nothing selected.
fn at(document: &CodeDocument, byte: usize) -> Selection {
    Selection::collapsed(Caret::at(document, byte))
}

/// `edit` applied to a copy of `document`'s text.
fn applied(document: &CodeDocument, edit: Option<Edit>) -> Option<String> {
    let mut document = document.clone();
    let Edit { range, text } = edit?;
    document.replace(range, &text).unwrap();
    Some(document.text().to_owned())
}

fn key(tab_size: usize) -> MetricsKey {
    MetricsKey {
        tab_size,
        wrap_columns: None,
    }
}

#[test]
fn backspace_and_delete_join_lines_across_whole_line_endings() {
    for (text, joined) in [("ab\ncd", "abcd"), ("ab\r\ncd", "abcd")] {
        let document = doc(text);
        let second = document.line_range(1).unwrap().start;
        let backspace = backspace(&document, at(&document, second));
        assert_eq!(applied(&document, backspace).as_deref(), Some(joined));
        let delete = delete(&document, at(&document, 2));
        assert_eq!(applied(&document, delete).as_deref(), Some(joined));
    }
}

#[test]
fn nothing_is_deleted_past_either_end() {
    let document = doc("\u{feff}ab");
    assert_eq!(backspace(&document, at(&document, 3)), None);
    assert_eq!(delete(&document, at(&document, 5)), None);
    let empty = doc("");
    assert_eq!(backspace(&empty, at(&empty, 0)), None);
    assert_eq!(delete(&empty, at(&empty, 0)), None);
}

#[test]
fn deleting_takes_whole_multibyte_and_double_width_characters() {
    // "é" is 2 bytes, "✅" 3 bytes and two cells wide, "🦀" 4 bytes.
    let document = doc("é✅🦀");
    let end = document.text().len();
    assert_eq!(
        applied(&document, backspace(&document, at(&document, end))).as_deref(),
        Some("é✅")
    );
    assert_eq!(
        applied(&document, backspace(&document, at(&document, 5))).as_deref(),
        Some("é🦀")
    );
    assert_eq!(
        applied(&document, delete(&document, at(&document, 0))).as_deref(),
        Some("✅🦀")
    );
}

#[test]
fn a_combining_sequence_takes_one_press_per_scalar_value() {
    let document = doc("e\u{301}x");
    let once = applied(&document, backspace(&document, at(&document, 3))).unwrap();
    assert_eq!(once, "ex");
    let document = doc(&once);
    assert_eq!(
        applied(&document, backspace(&document, at(&document, 1))).as_deref(),
        Some("x")
    );
}

#[test]
fn edits_replace_a_selection_made_in_either_direction() {
    let document = doc("one two three");
    let forward = Selection::between(&document, 4, Caret::at(&document, 7));
    let backward = Selection::between(&document, 7, Caret::at(&document, 4));
    for selection in [forward, backward] {
        let edits = [
            (typed(selection, 'X'), "one X three"),
            (Some(newline(&document, selection)), "one \n three"),
            (backspace(&document, selection), "one  three"),
            (delete(&document, selection), "one  three"),
            (
                Some(tab(&document, selection, key(4), &LAYOUT_FONT)),
                "one      three",
            ),
        ];
        for (edit, expected) in edits {
            assert_eq!(applied(&document, edit).as_deref(), Some(expected));
        }
    }
}

#[test]
fn enter_breaks_lines_with_the_documents_line_ending() {
    let lf = doc("ab\ncd");
    assert_eq!(newline(&lf, at(&lf, 1)).text, "\n");
    let crlf = doc("ab\r\ncd");
    assert_eq!(newline(&crlf, at(&crlf, 1)).text, "\r\n");
}

#[test]
fn control_characters_type_nothing() {
    let document = doc("");
    for ch in ['\n', '\r', '\t', '\u{1}', '\u{7f}', '\u{85}'] {
        assert_eq!(typed(at(&document, 0), ch), None, "{ch:?}");
    }
    assert_eq!(typed(at(&document, 0), '日').unwrap().text, "日");
}

#[test]
fn tab_fills_spaces_to_the_next_stop_counting_display_columns() {
    // "✅" takes columns 0-1, so the stop at 4 is 2 spaces away.
    let wide = doc("✅x");
    assert_eq!(tab(&wide, at(&wide, 3), key(4), &LAYOUT_FONT).text, "  ");
    // A tab in a space-indented file reaches column 4, so after "a" the
    // stop at 8 is 3 spaces away.
    let tabbed = doc("\ta\n  b\n  c");
    assert_eq!(
        tab(&tabbed, at(&tabbed, 2), key(4), &LAYOUT_FONT).text,
        "   "
    );
    // At a stop, a whole tab's width.
    let plain = doc("abcd");
    assert_eq!(tab(&plain, at(&plain, 4), key(2), &LAYOUT_FONT).text, "  ");
    // The column is that of the selection's start, whichever end the
    // caret is at.
    let selected = Selection::between(&plain, 4, Caret::at(&plain, 1));
    assert_eq!(tab(&plain, selected, key(4), &LAYOUT_FONT).text, "   ");
}

#[test]
fn tab_inserts_a_tab_in_documents_indented_with_tabs() {
    let document = doc("if x then\n\ty()\nend");
    let edit = tab(&document, at(&document, 3), key(4), &LAYOUT_FONT);
    assert_eq!(edit.text, "\t");
}

/// Characters chosen to hit every awkward case: line endings and lone
/// carriage returns, a byte order mark anywhere, tabs, combining marks,
/// multi-byte and double-width glyphs.
fn awkward_text() -> impl Strategy<Value = String> {
    let pieces = prop::sample::select(vec![
        "a", " ", "\t", "\r", "\n", "\r\n", "é", "e\u{301}", "日", "✅", "🦀", "\u{feff}",
    ]);
    prop::collection::vec(pieces, 0..40).prop_map(|pieces| pieces.concat())
}

/// A caret position never splits a character, the byte order mark or a
/// CRLF.
fn assert_valid_position(document: &CodeDocument, byte: usize) -> Result<(), TestCaseError> {
    let text = document.text();
    prop_assert!(text.is_char_boundary(byte), "{byte} splits a character");
    let bom = if text.starts_with('\u{feff}') { 3 } else { 0 };
    prop_assert!(byte >= bom, "{byte} inside the byte order mark");
    prop_assert!(
        !(text[..byte].ends_with('\r') && text[byte..].starts_with('\n')),
        "{byte} splits a CRLF"
    );
    Ok(())
}

proptest! {
    #[test]
    fn editing_keys_keep_the_document_and_caret_valid(
        text in awkward_text(),
        anchor in 0usize..200,
        caret in 0usize..200,
        steps in prop::collection::vec((0u8..9, 0usize..200), 1..30),
        tab_size in 1usize..8,
    ) {
        let mut document = doc(&text);
        let mut selection = Selection::between(&document, anchor, Caret::at(&document, caret));
        for (step, target) in steps {
            let edit = match step {
                0 => typed(selection, 'x'),
                1 => typed(selection, '日'),
                2 => typed(selection, '✅'),
                3 => Some(newline(&document, selection)),
                4 => Some(tab(&document, selection, key(tab_size), &LAYOUT_FONT)),
                5 => backspace(&document, selection),
                6 => delete(&document, selection),
                7 => {
                    selection = selection.moved(Caret::at(&document, target), true);
                    None
                }
                _ => {
                    selection = Selection::collapsed(Caret::at(&document, target));
                    None
                }
            };
            if let Some(Edit { range, text }) = edit {
                let before = document.clone();
                match document.replace(range.clone(), &text) {
                    Ok(replaced) => {
                        selection = Selection::collapsed(Caret::at(&document, replaced.range.end));
                    }
                    // Deleting what precedes a U+FEFF at the start would
                    // make it a byte order mark, which no edit can remove.
                    Err(EditError::CreatesBom) => prop_assert_eq!(&document, &before),
                    Err(err) => prop_assert!(false, "{range:?} {text:?}: {err}"),
                }
            }
            assert_valid_position(&document, selection.anchor())?;
            assert_valid_position(&document, selection.caret().byte())?;
            prop_assert_eq!(
                document.line_count(),
                document.text().matches('\n').count() + 1
            );
        }
    }
}
