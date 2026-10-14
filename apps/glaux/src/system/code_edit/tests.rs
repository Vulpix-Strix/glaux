use proptest::prelude::*;

use super::*;
use crate::system::code_cursor::Caret;
use crate::system::code_document::EditError;
use crate::system::editor_settings::IndentStyle;
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
                Some(tab(
                    &document,
                    selection,
                    IndentUnit::Spaces(4),
                    key(4),
                    &LAYOUT_FONT,
                )),
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
    let spaces = |width| IndentUnit::Spaces(width);
    // "✅" takes columns 0-1, so the stop at 4 is 2 spaces away.
    let wide = doc("✅x");
    assert_eq!(
        tab(&wide, at(&wide, 3), spaces(4), key(4), &LAYOUT_FONT).text,
        "  "
    );
    // A tab reaches column 4 when tabs are 4 wide, so after "a" the next
    // 2-column stop is 1 space away.
    let tabbed = doc("\ta\n  b\n  c");
    assert_eq!(
        tab(&tabbed, at(&tabbed, 2), spaces(2), key(4), &LAYOUT_FONT).text,
        " "
    );
    // At a stop, a whole step's width, whatever the tab size.
    let plain = doc("abcd");
    assert_eq!(
        tab(&plain, at(&plain, 4), spaces(2), key(8), &LAYOUT_FONT).text,
        "  "
    );
    // The column is that of the selection's start, whichever end the
    // caret is at.
    let selected = Selection::between(&plain, 4, Caret::at(&plain, 1));
    assert_eq!(
        tab(&plain, selected, spaces(4), key(4), &LAYOUT_FONT).text,
        "   "
    );
    let tab_unit = tab(&plain, selected, IndentUnit::Tab, key(4), &LAYOUT_FONT);
    assert_eq!(tab_unit.text, "\t");
}

#[test]
fn the_indent_unit_follows_the_setting_or_else_the_file() {
    let tabs = doc("if x then\n\ty()\nend");
    let two = doc("if x then\n  y()\nend");
    let unknown = doc("x = 1");
    let of = |document, style| IndentUnit::of(document, style, 4);
    assert_eq!(of(&tabs, IndentStyle::Detect), IndentUnit::Tab);
    assert_eq!(of(&two, IndentStyle::Detect), IndentUnit::Spaces(2));
    assert_eq!(of(&unknown, IndentStyle::Detect), IndentUnit::Spaces(4));
    assert_eq!(of(&tabs, IndentStyle::Spaces), IndentUnit::Spaces(4));
    assert_eq!(of(&two, IndentStyle::Tabs), IndentUnit::Tab);
}

#[test]
fn enter_keeps_the_indentation_of_the_line_it_breaks() {
    let spaces = doc("    local x = 1");
    assert_eq!(newline(&spaces, at(&spaces, 15)).text, "\n    ");
    let tabs = doc("\t\tif x then\r\nend");
    assert_eq!(newline(&tabs, at(&tabs, 11)).text, "\r\n\t\t");
    // Inside the indentation only what is left of the caret carries over,
    // so the line keeps its own text where it was.
    assert_eq!(newline(&spaces, at(&spaces, 2)).text, "\n  ");
    // Over a selection, the indentation is that of the line it starts on.
    let lines = doc("  a\n\tb");
    let selection = Selection::between(&lines, 6, Caret::at(&lines, 3));
    let edit = newline(&lines, selection);
    assert_eq!((edit.range, edit.text.as_str()), (3..6, "\n  "));
    // No indentation, just the line break; a byte order mark is not one.
    let bom = doc("\u{feff}x");
    assert_eq!(newline(&bom, at(&bom, 4)).text, "\n");
}

#[test]
fn pasted_line_breaks_take_the_documents_line_ending() {
    let crlf = doc("a\r\nb");
    let edit = paste(&crlf, at(&crlf, 1), "x\ny\r\nz\rw").unwrap();
    assert_eq!(edit.text, "x\r\ny\r\nz\r\nw");
    let lf = doc("ab");
    let selection = Selection::between(&lf, 0, Caret::at(&lf, 2));
    assert_eq!(
        applied(&lf, paste(&lf, selection, "1\r\n\t2")).as_deref(),
        Some("1\n\t2")
    );
    assert_eq!(paste(&lf, at(&lf, 0), ""), None);
}

/// `edit`'s result and the selection it leaves, as `(anchor, caret)`.
fn reselected(document: &CodeDocument, edit: Option<Reselecting>) -> (String, usize, usize) {
    let Reselecting {
        edit,
        anchor,
        caret,
    } = edit.expect("an edit");
    (applied(document, Some(edit)).unwrap(), anchor, caret)
}

#[test]
fn tab_over_lines_indents_each_non_blank_one_and_keeps_them_selected() {
    let document = doc("a\n\n  b\nc");
    // From the start of "a" to inside "b".
    let selection = Selection::between(&document, 0, Caret::at(&document, 5));
    assert!(spans_lines(&document, selection));
    let result = indent_lines(&document, selection, IndentUnit::Spaces(2));
    assert_eq!(
        reselected(&document, result),
        ("  a\n\n    b\nc".into(), 0, 9)
    );
    let result = indent_lines(&document, selection, IndentUnit::Tab);
    assert_eq!(
        reselected(&document, result),
        ("\ta\n\n\t  b\nc".into(), 0, 7)
    );
}

#[test]
fn a_selection_ending_at_a_lines_start_leaves_that_line() {
    // A triple click selects "a" and its line break, caret on the next line.
    let document = doc("a\r\nb");
    let selection = Selection::between(&document, 0, Caret::at(&document, 3));
    let result = indent_lines(&document, selection, IndentUnit::Spaces(4));
    assert_eq!(reselected(&document, result), ("    a\r\nb".into(), 0, 7));
}

#[test]
fn indenting_only_blank_lines_changes_nothing() {
    let document = doc("\n  \n\t");
    let selection = Selection::all(&document);
    assert_eq!(indent_lines(&document, selection, IndentUnit::Tab), None);
}

#[test]
fn shift_tab_removes_up_to_one_step_from_each_line() {
    let document = doc("    a\n  b\nc\n\t\td\n      e");
    let selection = Selection::all(&document);
    let result = outdent_lines(&document, selection, IndentUnit::Spaces(4), 4);
    assert_eq!(
        reselected(&document, result).0,
        "a\nb\nc\n\td\n  e",
        "partial steps go whole, a tab counts as a step"
    );
    // With a 2-column tab a step of 4 takes both tabs.
    let result = outdent_lines(&document, selection, IndentUnit::Spaces(4), 2);
    assert_eq!(reselected(&document, result).0, "a\nb\nc\nd\n  e");
    let flat = doc("a\nb");
    let all = Selection::all(&flat);
    assert_eq!(outdent_lines(&flat, all, IndentUnit::Tab, 4), None);
}

#[test]
fn shift_tab_without_a_selection_outdents_the_caret_line_keeping_its_place() {
    let document = doc("x\n    foo()");
    // Caret before "oo".
    let result = outdent_lines(&document, at(&document, 7), IndentUnit::Spaces(4), 4);
    assert_eq!(reselected(&document, result), ("x\nfoo()".into(), 3, 3));
    // A caret inside the removed blanks goes to the line's start.
    let result = outdent_lines(&document, at(&document, 4), IndentUnit::Spaces(4), 4);
    assert_eq!(reselected(&document, result), ("x\nfoo()".into(), 2, 2));
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
        steps in prop::collection::vec((0u8..13, 0usize..200), 1..30),
        tab_size in 1usize..8,
    ) {
        let mut document = doc(&text);
        let mut selection = Selection::between(&document, anchor, Caret::at(&document, caret));
        let unit = IndentUnit::Spaces(tab_size);
        for (step, target) in steps {
            let reselecting = match step {
                9 => indent_lines(&document, selection, unit),
                10 => indent_lines(&document, selection, IndentUnit::Tab),
                11 => outdent_lines(&document, selection, unit, tab_size),
                12 => outdent_lines(&document, selection, IndentUnit::Tab, tab_size),
                _ => None,
            };
            if let Some(Reselecting { edit, anchor, caret }) = reselecting {
                let before = document.clone();
                match document.replace(edit.range.clone(), &edit.text) {
                    Ok(_) => {
                        // The ends are exact positions, never moved to fit.
                        assert_valid_position(&document, anchor)?;
                        assert_valid_position(&document, caret)?;
                        selection = Selection::between(&document, anchor, Caret::at(&document, caret));
                        prop_assert_eq!(selection.anchor(), anchor);
                        prop_assert_eq!(selection.caret().byte(), caret);
                    }
                    Err(EditError::CreatesBom) => prop_assert_eq!(&document, &before),
                    Err(err) => prop_assert!(false, "{:?}: {err}", edit.range),
                }
                continue;
            }
            let edit = match step {
                0 => typed(selection, 'x'),
                1 => typed(selection, '日'),
                2 => typed(selection, '✅'),
                3 => Some(newline(&document, selection)),
                4 => Some(tab(&document, selection, unit, key(tab_size), &LAYOUT_FONT)),
                5 => backspace(&document, selection),
                6 => delete(&document, selection),
                7 => {
                    selection = selection.moved(Caret::at(&document, target), true);
                    None
                }
                8 => {
                    selection = Selection::collapsed(Caret::at(&document, target));
                    None
                }
                _ => paste(&document, selection, "p\r\nq\rr"),
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
