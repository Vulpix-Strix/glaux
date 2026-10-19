//! The signature of the call being typed.

use super::selection::caret;
use super::*;
use crate::system::code_history::EditKind;
use pretty_assertions::assert_eq;

const LEFT: InputEvent = InputEvent::Left { select: false };
const UP: InputEvent = InputEvent::Up { select: false };

fn text(session: &CodeSession) -> &str {
    session.active_document().unwrap().text()
}

/// An editor over the one document `file` holding `text`, caret at its end.
fn editing(file: &str, text: &str) -> (CodeEditor, CodeSession) {
    let fixture = Fixture::new("code-signature");
    let (cartridge, mut session) = session_with_text(&fixture, &[file], text);
    let mut editor = editor();
    editor.refresh_metrics(&mut session);
    drop(cartridge);
    feed(
        &mut editor,
        &mut session,
        &[InputEvent::DocumentEnd { select: false }],
    );
    (editor, session)
}

fn luau(text: &str) -> (CodeEditor, CodeSession) {
    editing("a.luau", text)
}

fn rhai(text: &str) -> (CodeEditor, CodeSession) {
    editing("a.rhai", text)
}

fn type_str(editor: &mut CodeEditor, session: &mut CodeSession, typed: &str) {
    for ch in typed.chars() {
        feed(editor, session, &[InputEvent::Text(ch)]);
    }
}

/// The function and parameter the popup is on.
fn shown(editor: &CodeEditor) -> Option<(String, Option<usize>)> {
    editor
        .signature()
        .map(|popup| (popup.view().name.clone(), popup.view().active))
}

fn is_open(editor: &CodeEditor) -> bool {
    editor.signature().is_some()
}

#[test]
fn typing_the_parenthesis_of_a_known_function_opens_the_popup() {
    let (mut editor, mut session) = luau("");
    type_str(&mut editor, &mut session, "spr");
    assert!(!is_open(&editor));
    type_str(&mut editor, &mut session, "(");
    assert_eq!(shown(&editor), Some(("spr".to_owned(), Some(0))));
    assert_eq!(editor.signature().unwrap().help().open, 3);
}

#[test]
fn each_comma_moves_to_the_next_parameter() {
    let (mut editor, mut session) = luau("");
    type_str(&mut editor, &mut session, "spr(1");
    assert_eq!(shown(&editor), Some(("spr".to_owned(), Some(0))));
    type_str(&mut editor, &mut session, ", 2");
    assert_eq!(shown(&editor), Some(("spr".to_owned(), Some(1))));
    type_str(&mut editor, &mut session, ", ");
    assert_eq!(shown(&editor), Some(("spr".to_owned(), Some(2))));
}

#[test]
fn the_same_works_in_rhai_with_a_namespaced_function() {
    let (mut editor, mut session) = rhai("");
    type_str(&mut editor, &mut session, "audio::tone(1, ");
    assert_eq!(shown(&editor), Some(("audio::tone".to_owned(), Some(1))));
}

#[test]
fn closing_the_call_closes_the_popup() {
    let (mut editor, mut session) = luau("");
    type_str(&mut editor, &mut session, "spr(1, 2");
    assert!(is_open(&editor));
    type_str(&mut editor, &mut session, ")");
    assert!(!is_open(&editor));
}

#[test]
fn typing_into_a_nested_call_shows_it_and_leaving_it_returns_to_the_outer() {
    let (mut editor, mut session) = luau("");
    type_str(&mut editor, &mut session, "spr(btnp(");
    assert_eq!(shown(&editor), Some(("btnp".to_owned(), Some(0))));
    type_str(&mut editor, &mut session, "1)");
    assert_eq!(shown(&editor), Some(("spr".to_owned(), Some(0))));
}

#[test]
fn unknown_functions_and_grouping_parentheses_open_nothing() {
    let (mut editor, mut session) = luau("");
    type_str(&mut editor, &mut session, "unknown(");
    assert!(!is_open(&editor));
    let (mut editor, mut session) = luau("");
    type_str(&mut editor, &mut session, "local x = (1 + ");
    assert!(!is_open(&editor));
    let (mut editor, mut session) = luau("");
    type_str(&mut editor, &mut session, "function f(");
    assert!(!is_open(&editor));
}

#[test]
fn a_function_of_the_script_is_described_by_its_parameter_names() {
    let (mut editor, mut session) = luau("local function f(a, b)\nend\n");
    type_str(&mut editor, &mut session, "f(1, ");
    let popup = editor.signature().expect("popup");
    assert_eq!(popup.view().params, ["a", "b"]);
    assert_eq!(popup.view().active, Some(1));
}

#[test]
fn comments_never_open_it() {
    let (mut editor, mut session) = luau("-- ");
    type_str(&mut editor, &mut session, "spr(");
    assert!(!is_open(&editor));
    let (mut editor, mut session) = luau("--[[ ");
    type_str(&mut editor, &mut session, "spr(");
    assert!(!is_open(&editor));
    let (mut editor, mut session) = rhai("// ");
    type_str(&mut editor, &mut session, "spr(");
    assert!(!is_open(&editor));
}

#[test]
fn the_popup_follows_the_caret_through_the_arguments() {
    let (mut editor, mut session) = luau("");
    type_str(&mut editor, &mut session, "spr(1, 2, 3");
    assert_eq!(shown(&editor).unwrap().1, Some(2));
    feed(&mut editor, &mut session, &[LEFT, LEFT, LEFT]);
    assert_eq!(shown(&editor).unwrap().1, Some(1));
    feed(&mut editor, &mut session, &[LEFT, LEFT, LEFT]);
    assert_eq!(shown(&editor).unwrap().1, Some(0));
}

#[test]
fn moving_the_caret_out_of_the_call_closes_the_popup() {
    let (mut editor, mut session) = luau("x = 1\n");
    type_str(&mut editor, &mut session, "spr(1");
    assert!(is_open(&editor));
    feed(
        &mut editor,
        &mut session,
        &[InputEvent::Home { select: false }, UP],
    );
    assert!(!is_open(&editor));
}

#[test]
fn moving_into_a_call_does_not_open_the_popup() {
    let (mut editor, mut session) = luau("spr(1, 2)");
    feed(&mut editor, &mut session, &[LEFT, LEFT]);
    assert!(!is_open(&editor));
}

#[test]
fn asking_for_it_opens_the_popup_inside_a_call() {
    let (mut editor, mut session) = luau("spr(1, 2)");
    feed(&mut editor, &mut session, &[LEFT, LEFT]);
    feed(&mut editor, &mut session, &[InputEvent::SignatureHelp]);
    assert_eq!(shown(&editor), Some(("spr".to_owned(), Some(1))));
}

#[test]
fn asking_for_it_outside_a_call_opens_nothing() {
    let (mut editor, mut session) = luau("spr(1, 2)");
    feed(&mut editor, &mut session, &[InputEvent::SignatureHelp]);
    assert!(!is_open(&editor));
}

#[test]
fn undoing_the_call_closes_the_popup_and_redoing_it_does_not_bring_it_back() {
    let (mut editor, mut session) = luau("");
    type_str(&mut editor, &mut session, "spr(");
    assert!(is_open(&editor));
    feed(&mut editor, &mut session, &[InputEvent::Undo]);
    assert!(!is_open(&editor));
    feed(&mut editor, &mut session, &[InputEvent::Redo]);
    assert!(!is_open(&editor));
}

#[test]
fn a_selection_closes_the_popup() {
    let (mut editor, mut session) = luau("");
    type_str(&mut editor, &mut session, "spr(1");
    feed(
        &mut editor,
        &mut session,
        &[InputEvent::Left { select: true }],
    );
    assert!(!is_open(&editor));
}

#[test]
fn escape_closes_the_popup_and_is_not_left_for_the_workspace() {
    let (mut editor, mut session) = luau("");
    type_str(&mut editor, &mut session, "spr(1");
    assert!(editor.claims_break());
    assert_eq!(
        requests(&mut editor, &mut session, &[InputEvent::Break]),
        []
    );
    assert!(!is_open(&editor));
    assert!(!editor.claims_break());
}

#[test]
fn after_escape_typing_on_in_the_same_call_does_not_bring_it_back() {
    let (mut editor, mut session) = luau("");
    type_str(&mut editor, &mut session, "spr(1");
    feed(&mut editor, &mut session, &[InputEvent::Break]);
    type_str(&mut editor, &mut session, ", 2, ");
    assert!(!is_open(&editor));
}

#[test]
fn after_escape_a_new_call_opens_it_again() {
    let (mut editor, mut session) = luau("");
    type_str(&mut editor, &mut session, "spr(1");
    feed(&mut editor, &mut session, &[InputEvent::Break]);
    type_str(&mut editor, &mut session, ", btnp(");
    assert_eq!(shown(&editor), Some(("btnp".to_owned(), Some(0))));
}

#[test]
fn after_escape_asking_for_it_opens_the_popup_again() {
    let (mut editor, mut session) = luau("");
    type_str(&mut editor, &mut session, "spr(1");
    feed(&mut editor, &mut session, &[InputEvent::Break]);
    feed(&mut editor, &mut session, &[InputEvent::SignatureHelp]);
    assert!(is_open(&editor));
}

#[test]
fn leaving_the_call_after_escape_lets_the_next_visit_open_it() {
    let (mut editor, mut session) = luau("");
    type_str(&mut editor, &mut session, "spr(1");
    feed(&mut editor, &mut session, &[InputEvent::Break]);
    type_str(&mut editor, &mut session, ")");
    type_str(&mut editor, &mut session, " spr(");
    assert!(is_open(&editor));
}

#[test]
fn the_popup_takes_no_key_but_escape() {
    let (mut editor, mut session) = luau("x = 1\n");
    type_str(&mut editor, &mut session, "spr(12");
    let before = caret(&session);
    feed(&mut editor, &mut session, &[LEFT]);
    assert_eq!(caret(&session), before - 1);
    feed(&mut editor, &mut session, &[InputEvent::Enter]);
    assert_eq!(text(&session), "x = 1\nspr(1\n2");
}

#[test]
fn the_status_bar_names_the_key_that_closes_it() {
    let (mut editor, mut session) = luau("");
    assert_ne!(editor.hint(), SIGNATURE_HINT);
    type_str(&mut editor, &mut session, "spr(");
    assert_eq!(editor.hint(), SIGNATURE_HINT);
}

#[test]
fn the_list_of_suggestions_keeps_its_hint_and_its_escape() {
    let (mut editor, mut session) = luau("");
    type_str(&mut editor, &mut session, "spr(re");
    assert!(editor.completion().is_some());
    assert!(is_open(&editor));
    assert_eq!(editor.hint(), COMPLETION_HINT);
    feed(&mut editor, &mut session, &[InputEvent::Break]);
    assert!(editor.completion().is_none());
    assert!(is_open(&editor), "the first Escape closes only the list");
    feed(&mut editor, &mut session, &[InputEvent::Break]);
    assert!(!is_open(&editor));
}

#[test]
fn forgetting_the_cartridge_drops_the_popup_and_the_dismissed_call() {
    let (mut editor, mut session) = luau("");
    type_str(&mut editor, &mut session, "spr(1");
    editor.forget();
    assert!(!is_open(&editor));

    type_str(&mut editor, &mut session, ", ");
    feed(&mut editor, &mut session, &[InputEvent::Break]);
    editor.forget();
    feed(&mut editor, &mut session, &[InputEvent::SignatureHelp]);
    assert!(is_open(&editor));
}

#[test]
fn the_popup_is_not_shown_for_text_that_has_changed_behind_its_back() {
    let (mut editor, mut session) = luau("");
    type_str(&mut editor, &mut session, "spr(");
    let id = session.active_document().unwrap().id();
    let end = session.active_document().unwrap().text().len();
    session.edit(id, end..end, "1", EditKind::Typing, None);
    assert!(editor.signature_layout(&session).is_none());
}
