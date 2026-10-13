//! Selecting text in the Code view with the keyboard and the pointer.

use std::ops::Range;

use super::*;
use pretty_assertions::assert_eq;

pub(super) fn select(event: fn(bool) -> InputEvent) -> InputEvent {
    event(true)
}

pub(super) fn plain(event: fn(bool) -> InputEvent) -> InputEvent {
    event(false)
}

pub(super) fn right(select: bool) -> InputEvent {
    InputEvent::Right { select }
}

pub(super) fn left(select: bool) -> InputEvent {
    InputEvent::Left { select }
}

pub(super) fn down(select: bool) -> InputEvent {
    InputEvent::Down { select }
}

pub(super) fn selected(session: &CodeSession) -> Range<usize> {
    let id = session.tabs().active().unwrap();
    session.tabs().selection(id).unwrap().range()
}

pub(super) fn caret(session: &CodeSession) -> usize {
    let id = session.tabs().active().unwrap();
    session.tabs().selection(id).unwrap().caret().byte()
}

/// An editor showing `text` as the one open document, with `settings`.
pub(super) fn open(
    fixture: &Fixture,
    text: &str,
    settings: EditorSettings,
) -> (CodeEditor, LoadedCartridge, CodeSession) {
    let (cartridge, mut session) = session_with_text(fixture, &["a.luau"], text);
    let mut editor = editor();
    editor.set_settings(settings);
    editor.refresh_metrics(&mut session);
    (editor, cartridge, session)
}

pub(super) fn geometry(editor: &CodeEditor, session: &CodeSession) -> TextGeometry {
    editor.geometry(session.active_document().unwrap())
}

/// The middle of the left edge of the cell at `column` of the view's row
/// `row`, where a press puts the caret before that column's character.
pub(super) fn cell(geometry: &TextGeometry, row: i32, column: i32) -> Point {
    let x = geometry.viewport.x + column * geometry.cell_w;
    let y = geometry.viewport.y + row * geometry.cell_h + geometry.cell_h / 2;
    Point::new(x as u16, y as u16)
}

fn shift_press(at: Point) -> InputEvent {
    InputEvent::PointerDown {
        button: PointerButton::Primary,
        at,
        select: true,
    }
}

pub(super) fn click(editor: &mut CodeEditor, session: &mut CodeSession, at: Point) {
    feed(
        editor,
        session,
        &[press(PointerButton::Primary, at), release(at)],
    );
}

#[test]
fn shift_moves_grow_and_shrink_a_selection_past_its_anchor() {
    let fixture = Fixture::new("code-sel-flip");
    let (mut editor, _, mut session) = open(&fixture, "abcdef\nghij", EditorSettings::default());
    feed(&mut editor, &mut session, &[plain(right), plain(right)]);
    feed(&mut editor, &mut session, &[select(right); 3]);
    assert_eq!(selected(&session), 2..5);
    feed(&mut editor, &mut session, &[select(left); 5]);
    assert_eq!((selected(&session), caret(&session)), (0..2, 0));
    feed(&mut editor, &mut session, &[select(down)]);
    assert_eq!((selected(&session), caret(&session)), (2..7, 7));
}

#[test]
fn every_kind_of_move_extends_with_shift() {
    let fixture = Fixture::new("code-sel-moves");
    let text = "one two three\n  four five\nsix";
    let (mut editor, _, mut session) = open(&fixture, text, EditorSettings::default());
    let steps: [(InputEvent, Range<usize>); 6] = [
        (InputEvent::WordRight { select: true }, 0..3),
        (InputEvent::End { select: true }, 0..13),
        (down(true), 0..25),
        // Home goes to the first non-blank character first.
        (InputEvent::Home { select: true }, 0..16),
        (InputEvent::DocumentEnd { select: true }, 0..29),
        (InputEvent::WordLeft { select: true }, 0..26),
    ];
    for (event, expected) in steps {
        feed(&mut editor, &mut session, &[event]);
        assert_eq!(selected(&session), expected, "after {event:?}");
    }
    feed(
        &mut editor,
        &mut session,
        &[InputEvent::DocumentStart { select: true }],
    );
    assert_eq!((selected(&session), caret(&session)), (0..0, 0));
}

#[test]
fn plain_moves_drop_the_selection() {
    let fixture = Fixture::new("code-sel-plain");
    let (mut editor, _, mut session) = open(&fixture, "abcdef\nghij", EditorSettings::default());
    let shift_to = |editor: &mut CodeEditor, session: &mut CodeSession| {
        feed(
            editor,
            session,
            &[
                InputEvent::DocumentStart { select: false },
                plain(right),
                select(right),
                select(right),
                select(right),
            ],
        );
        assert_eq!(selected(session), 1..4);
    };
    // A single step collapses the selection to the side it heads for.
    shift_to(&mut editor, &mut session);
    feed(&mut editor, &mut session, &[plain(left)]);
    assert_eq!((selected(&session), caret(&session)), (1..1, 1));
    shift_to(&mut editor, &mut session);
    feed(&mut editor, &mut session, &[plain(right)]);
    assert_eq!((selected(&session), caret(&session)), (4..4, 4));
    // Any other move goes from the caret.
    shift_to(&mut editor, &mut session);
    feed(&mut editor, &mut session, &[plain(down)]);
    assert_eq!((selected(&session), caret(&session)), (11..11, 11));
    shift_to(&mut editor, &mut session);
    feed(
        &mut editor,
        &mut session,
        &[InputEvent::End { select: false }],
    );
    assert_eq!(selected(&session), 6..6);
}

#[test]
fn shift_down_extends_across_a_soft_wrap_as_far_as_down_moves() {
    let fixture = Fixture::new("code-sel-wrap");
    let text = "alpha beta gamma delta ".repeat(5);
    let settings = EditorSettings {
        word_wrap: true,
        ..EditorSettings::default()
    };
    let (mut editor, _, mut session) = open(&fixture, &text, settings);
    let plain_fixture = Fixture::new("code-sel-wrap-plain");
    let (mut plain_editor, _, mut moved) = open(&plain_fixture, &text, settings);
    feed(&mut editor, &mut session, &[plain(right), plain(right)]);
    feed(
        &mut plain_editor,
        &mut moved,
        &[plain(right), plain(right), plain(down)],
    );
    feed(&mut editor, &mut session, &[select(down)]);
    let target = caret(&moved);
    // Still the first line: the row below is one of its folds.
    assert!(target > 2 && target < text.len(), "{target}");
    assert_eq!(selected(&session), 2..target);
}

#[test]
fn shift_page_down_selects_a_screenful_of_rows() {
    let fixture = Fixture::new("code-sel-page");
    let (_, mut session) = session_with(&fixture, &["a.luau"], 100);
    let mut editor = editor();
    editor.refresh_metrics(&mut session);
    let rows = editor.geometry(session.active_document().unwrap()).rows;
    feed(
        &mut editor,
        &mut session,
        &[InputEvent::PageDown { select: true }],
    );
    // "line N\n" is 7 bytes for N below 10, 8 below 100.
    let start_of = |line: usize| (0..line).map(|n| format!("line {}\n", n + 1).len()).sum();
    assert_eq!(selected(&session), 0..start_of(rows));
    feed(
        &mut editor,
        &mut session,
        &[InputEvent::PageUp { select: true }],
    );
    assert_eq!(selected(&session), 0..0);
}

#[test]
fn select_all_selects_the_whole_document_and_keeps_the_view() {
    let fixture = Fixture::new("code-sel-all");
    let (_, mut session) = session_with(&fixture, &["a.luau"], 100);
    let mut editor = editor();
    editor.refresh_metrics(&mut session);
    let len = session.active_document().unwrap().text().len();
    feed(&mut editor, &mut session, &[InputEvent::SelectAll]);
    assert_eq!((selected(&session), caret(&session)), (0..len, len));
    let id = session.tabs().active().unwrap();
    assert_eq!(session.tabs().view(id).unwrap().top_line, 0);
}

#[test]
fn each_document_keeps_its_own_selection() {
    let fixture = Fixture::new("code-sel-tabs");
    let (_, mut session) = session_with(&fixture, &["a.luau", "b.luau"], 10);
    let mut editor = editor();
    editor.refresh_metrics(&mut session);
    feed(&mut editor, &mut session, &[select(right), select(right)]);
    feed(&mut editor, &mut session, &[InputEvent::PreviousDocument]);
    assert_eq!(selected(&session), 0..0);
    feed(&mut editor, &mut session, &[InputEvent::SelectAll]);
    let all = selected(&session);
    feed(&mut editor, &mut session, &[InputEvent::NextDocument]);
    assert_eq!(selected(&session), 0..2);
    feed(&mut editor, &mut session, &[InputEvent::PreviousDocument]);
    assert_eq!(selected(&session), all);
}

#[test]
fn a_shift_press_extends_from_the_anchor_either_way() {
    let fixture = Fixture::new("code-sel-shift-click");
    let (mut editor, _, mut session) = open(&fixture, "abcdef\nghijkl", EditorSettings::default());
    let g = geometry(&editor, &session);
    click(&mut editor, &mut session, cell(&g, 0, 2));
    let below = cell(&g, 1, 3);
    feed(
        &mut editor,
        &mut session,
        &[shift_press(below), release(below)],
    );
    assert_eq!(selected(&session), 2..10);
    let before = cell(&g, 0, 0);
    feed(
        &mut editor,
        &mut session,
        &[shift_press(before), release(before)],
    );
    assert_eq!((selected(&session), caret(&session)), (0..2, 0));
}

#[test]
fn a_drag_selects_from_the_press_until_its_release() {
    let fixture = Fixture::new("code-sel-drag");
    let (mut editor, _, mut session) = open(&fixture, "abcdef\nghijkl", EditorSettings::default());
    let g = geometry(&editor, &session);
    let from = cell(&g, 0, 4);
    let to = cell(&g, 1, 2);
    feed(
        &mut editor,
        &mut session,
        &[
            press(PointerButton::Primary, from),
            InputEvent::PointerMove(Some(to)),
        ],
    );
    assert_eq!(selected(&session), 4..9);
    let back = cell(&g, 0, 1);
    feed(
        &mut editor,
        &mut session,
        &[InputEvent::PointerMove(Some(back))],
    );
    assert_eq!((selected(&session), caret(&session)), (1..4, 1));
    feed(&mut editor, &mut session, &[release(back)]);
    let later = cell(&g, 1, 5);
    feed(
        &mut editor,
        &mut session,
        &[InputEvent::PointerMove(Some(later))],
    );
    assert_eq!(selected(&session), 1..4);
}

#[test]
fn a_cancelled_press_stops_selecting() {
    let fixture = Fixture::new("code-sel-cancel");
    let (mut editor, _, mut session) = open(&fixture, "abcdef\nghijkl", EditorSettings::default());
    let g = geometry(&editor, &session);
    let from = cell(&g, 0, 1);
    feed(
        &mut editor,
        &mut session,
        &[
            press(PointerButton::Primary, from),
            InputEvent::PointerMove(Some(cell(&g, 0, 3))),
            InputEvent::PointerCancel,
            InputEvent::PointerMove(Some(cell(&g, 1, 3))),
        ],
    );
    assert_eq!(selected(&session), 1..3);
}

#[test]
fn double_and_triple_clicks_select_a_word_and_a_line() {
    let fixture = Fixture::new("code-sel-multi");
    let text = "foo bar.baz\nnext\n";
    let (mut editor, _, mut session) = open(&fixture, text, EditorSettings::default());
    let g = geometry(&editor, &session);
    let at = cell(&g, 0, 5);
    click(&mut editor, &mut session, at);
    assert_eq!(selected(&session), 5..5);
    editor.tick(None);
    click(&mut editor, &mut session, at);
    assert_eq!(selected(&session), 4..7);
    // A pixel away still continues the run.
    click(&mut editor, &mut session, Point::new(at.x + 1, at.y));
    // The line with its ending.
    assert_eq!((selected(&session), caret(&session)), (0..12, 12));
    // A fourth press starts over.
    click(&mut editor, &mut session, at);
    assert_eq!(selected(&session), 5..5);
}

#[test]
fn presses_too_late_or_too_far_apart_are_single_clicks() {
    let fixture = Fixture::new("code-sel-multi-late");
    let (mut editor, _, mut session) = open(&fixture, "foo bar baz", EditorSettings::default());
    let g = geometry(&editor, &session);
    let at = cell(&g, 0, 5);
    click(&mut editor, &mut session, at);
    for _ in 0..MULTI_CLICK_TICKS {
        editor.tick(None);
    }
    click(&mut editor, &mut session, at);
    assert_eq!(selected(&session), 5..5);
    click(&mut editor, &mut session, cell(&g, 0, 9));
    assert_eq!(selected(&session), 9..9);
}

#[test]
fn a_drag_ends_the_multi_click_run() {
    let fixture = Fixture::new("code-sel-multi-drag");
    let (mut editor, _, mut session) = open(&fixture, "foo bar baz", EditorSettings::default());
    let g = geometry(&editor, &session);
    let at = cell(&g, 0, 5);
    feed(
        &mut editor,
        &mut session,
        &[
            press(PointerButton::Primary, at),
            InputEvent::PointerMove(Some(cell(&g, 0, 9))),
            InputEvent::PointerMove(Some(at)),
            release(at),
        ],
    );
    // Right where the drag began, yet a single click.
    click(&mut editor, &mut session, at);
    assert_eq!(selected(&session), 5..5);
    click(&mut editor, &mut session, at);
    assert_eq!(selected(&session), 4..7);
}

#[test]
fn a_double_click_drag_extends_by_whole_words() {
    let fixture = Fixture::new("code-sel-word-drag");
    let (mut editor, _, mut session) = open(&fixture, "foo bar.baz qux", EditorSettings::default());
    let g = geometry(&editor, &session);
    let at = cell(&g, 0, 5);
    click(&mut editor, &mut session, at);
    feed(
        &mut editor,
        &mut session,
        &[
            press(PointerButton::Primary, at),
            InputEvent::PointerMove(Some(cell(&g, 0, 9))),
        ],
    );
    assert_eq!(selected(&session), 4..11);
    feed(
        &mut editor,
        &mut session,
        &[InputEvent::PointerMove(Some(cell(&g, 0, 1)))],
    );
    // The word pressed on stays selected, now from its end.
    assert_eq!((selected(&session), caret(&session)), (0..7, 0));
}

#[test]
fn a_triple_click_takes_crlf_endings_and_stops_at_the_last_line() {
    let fixture = Fixture::new("code-sel-triple-crlf");
    let (mut editor, _, mut session) = open(&fixture, "a\r\nbb\r\ncc", EditorSettings::default());
    let g = geometry(&editor, &session);
    let triple = |editor: &mut CodeEditor, session: &mut CodeSession, row| {
        let at = cell(&g, row, 1);
        for _ in 0..3 {
            click(editor, session, at);
        }
        selected(session)
    };
    assert_eq!(triple(&mut editor, &mut session, 1), 3..7);
    for _ in 0..MULTI_CLICK_TICKS {
        editor.tick(None);
    }
    assert_eq!(triple(&mut editor, &mut session, 2), 7..9);
}

#[test]
fn dragging_past_the_text_scrolls_until_released() {
    let fixture = Fixture::new("code-sel-autoscroll");
    let (_, mut session) = session_with(&fixture, &["a.luau"], 100);
    let mut editor = editor();
    editor.refresh_metrics(&mut session);
    let g = geometry(&editor, &session);
    let id = session.tabs().active().unwrap();
    let top = |session: &CodeSession| session.tabs().view(id).unwrap().top_line;
    let below = Point::new(40, editor.layout().status.y as u16 + 2);
    feed(
        &mut editor,
        &mut session,
        &[
            press(PointerButton::Primary, cell(&g, 0, 0)),
            InputEvent::PointerMove(Some(below)),
        ],
    );
    let selected_before = selected(&session);
    for _ in 0..4 * AUTOSCROLL_TICKS {
        editor.tick(Some(&mut session));
    }
    assert!(top(&session) >= 4, "scrolled to {}", top(&session));
    // The selection follows what scrolls under the pointer.
    assert!(selected(&session).end > selected_before.end);
    let document = session.active_document().unwrap();
    let rows = editor.geometry(document).rows;
    assert_eq!(document.line_of(caret(&session)), top(&session) + rows - 1);

    feed(&mut editor, &mut session, &[release(below)]);
    let stopped = top(&session);
    for _ in 0..4 * AUTOSCROLL_TICKS {
        editor.tick(Some(&mut session));
    }
    assert_eq!(top(&session), stopped);
}

#[test]
fn dragging_above_the_text_scrolls_back_up() {
    let fixture = Fixture::new("code-sel-autoscroll-up");
    let (_, mut session) = session_with(&fixture, &["a.luau"], 100);
    let mut editor = editor();
    editor.refresh_metrics(&mut session);
    let g = geometry(&editor, &session);
    let id = session.tabs().active().unwrap();
    feed(
        &mut editor,
        &mut session,
        &[InputEvent::PageDown { select: false }; 2],
    );
    let start = session.tabs().view(id).unwrap().top_line;
    let above = Point::new(40, editor.layout().strip.y as u16);
    feed(
        &mut editor,
        &mut session,
        &[
            press(PointerButton::Primary, cell(&g, 0, 0)),
            InputEvent::PointerMove(Some(above)),
        ],
    );
    for _ in 0..AUTOSCROLL_TICKS {
        editor.tick(Some(&mut session));
    }
    let top = session.tabs().view(id).unwrap().top_line;
    assert!(top < start, "{top} from {start}");
    let document = session.active_document().unwrap();
    assert_eq!(document.line_of(caret(&session)), top);
}

#[test]
fn presses_land_on_edges_of_tabs_and_wide_glyphs_however_lines_fold() {
    // A tab takes columns 0..4, x 4, the wide ✅ 5..7 and y 7.
    let text = "\tx\u{2705}y";
    for word_wrap in [false, true] {
        let fixture = Fixture::new(if word_wrap {
            "code-sel-wide-wrap"
        } else {
            "code-sel-wide"
        });
        let settings = EditorSettings {
            word_wrap,
            ..EditorSettings::default()
        };
        let (mut editor, _, mut session) = open(&fixture, text, settings);
        let g = geometry(&editor, &session);
        let mut hit = |column| {
            click(&mut editor, &mut session, cell(&g, 0, column));
            for _ in 0..MULTI_CLICK_TICKS {
                editor.tick(None);
            }
            caret(&session)
        };
        let hits: Vec<usize> = [1, 3, 5, 6, 7].into_iter().map(&mut hit).collect();
        assert_eq!(hits, [0, 1, 2, 5, 5], "wrap {word_wrap}");
        let from = cell(&g, 0, 0);
        feed(
            &mut editor,
            &mut session,
            &[
                press(PointerButton::Primary, from),
                InputEvent::PointerMove(Some(cell(&g, 0, 6))),
                release(from),
            ],
        );
        assert_eq!(selected(&session), 0..5, "wrap {word_wrap}");
    }
}

/// Every script of both Signal Lost examples, with lines folded and not:
/// selecting by screenfuls, by words and by lines only ever ends on valid
/// caret positions, and covers each whole script.
#[test]
fn signal_lost_scripts_select_on_valid_positions() {
    use crate::system::code_cursor::{line_at, word_at};

    let examples = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples");
    let storage = crate::cartridge_fs::CartridgeStorage::Available(
        crate::cartridge_fs::CartridgeFs::new(fs::canonicalize(examples).unwrap()),
    );
    let valid = |document: &CodeDocument, byte: usize| Caret::at(document, byte).byte() == byte;
    for example in ["signal-lost", "signal-lost-rhai"] {
        let cartridge = LoadedCartridge::load(&storage, example).unwrap();
        let scripts = cartridge.script_files().unwrap().paths;
        assert!(scripts.len() >= 10, "{example}: {scripts:?}");
        for word_wrap in [false, true] {
            let mut session = CodeSession::default();
            for path in &scripts {
                session.open(&cartridge, path).unwrap();
            }
            let mut editor = editor();
            editor.set_settings(EditorSettings {
                word_wrap,
                ..EditorSettings::default()
            });
            editor.refresh_metrics(&mut session);
            for path in &scripts {
                let name = format!("{example}/{path} wrap {word_wrap}");
                let id = session
                    .tabs()
                    .ids()
                    .find(|&id| session.document(id).unwrap().path() == path)
                    .unwrap();
                session.tabs_mut().activate(id);
                let len = session.document(id).unwrap().text().len();
                feed(&mut editor, &mut session, &[InputEvent::SelectAll]);
                assert_eq!(selected(&session), 0..len, "{name}");

                feed(
                    &mut editor,
                    &mut session,
                    &[InputEvent::DocumentStart { select: false }],
                );
                let mut pages = 0;
                while caret(&session) < len {
                    feed(
                        &mut editor,
                        &mut session,
                        &[InputEvent::PageDown { select: true }],
                    );
                    let document = session.document(id).unwrap();
                    assert!(valid(document, caret(&session)), "{name}");
                    assert_eq!(selected(&session).start, 0, "{name}");
                    pages += 1;
                    assert!(pages <= len, "{name}: paging never reaches the end");
                }

                feed(
                    &mut editor,
                    &mut session,
                    &[InputEvent::DocumentStart { select: false }],
                );
                let g = geometry(&editor, &session);
                for row in 0..g.rows as i32 {
                    for _ in 0..MULTI_CLICK_TICKS {
                        editor.tick(None);
                    }
                    let at = cell(&g, row, 6);
                    click(&mut editor, &mut session, at);
                    let byte = caret(&session);
                    click(&mut editor, &mut session, at);
                    let document = session.document(id).unwrap();
                    assert_eq!(
                        selected(&session),
                        word_at(document, byte),
                        "{name} row {row}"
                    );
                    click(&mut editor, &mut session, at);
                    let document = session.document(id).unwrap();
                    let line = selected(&session);
                    assert_eq!(line, line_at(document, byte), "{name} row {row}");
                    assert!(
                        valid(document, line.start) && valid(document, line.end),
                        "{name}"
                    );
                }
            }
        }
    }
}
