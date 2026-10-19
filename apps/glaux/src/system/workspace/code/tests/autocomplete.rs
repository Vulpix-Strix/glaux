use super::selection::caret;
use super::*;
use crate::system::code_history::EditKind;
use pretty_assertions::assert_eq;

const TAB: InputEvent = InputEvent::Tab { reverse: false };
const DOWN: InputEvent = InputEvent::Down { select: false };

fn text(session: &CodeSession) -> &str {
    session.active_document().unwrap().text()
}

/// An editor over the one document `file` holding `text`, caret at its end.
fn editing(file: &str, text: &str) -> (CodeEditor, CodeSession) {
    let fixture = Fixture::new("code-autocomplete");
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

fn labels(editor: &CodeEditor) -> Vec<&str> {
    editor
        .completion()
        .map(|popup| popup.labels())
        .unwrap_or_default()
}

fn is_open(editor: &CodeEditor) -> bool {
    editor.completion().is_some()
}

#[test]
fn a_word_lists_suggestions_from_the_second_letter() {
    let (mut editor, mut session) = luau("");
    type_str(&mut editor, &mut session, "r");
    assert!(!is_open(&editor));
    type_str(&mut editor, &mut session, "e");
    assert!(
        labels(&editor).contains(&"rectfill"),
        "{:?}",
        labels(&editor)
    );
}

#[test]
fn typing_a_separator_lists_the_members_of_the_receiver() {
    let (mut editor, mut session) = luau("");
    type_str(&mut editor, &mut session, "audio.");
    assert!(labels(&editor).contains(&"tone"));
    assert!(!labels(&editor).contains(&"rectfill"));

    let (mut editor, mut session) = rhai("");
    type_str(&mut editor, &mut session, "audio:");
    assert!(!is_open(&editor), "one colon is not a separator");
    type_str(&mut editor, &mut session, ":");
    assert!(labels(&editor).contains(&"tone"));
}

#[test]
fn constants_complete_their_members_in_both_languages() {
    let (mut editor, mut session) = luau("");
    type_str(&mut editor, &mut session, "Key.");
    assert!(labels(&editor).contains(&"A"));
    let (mut editor, mut session) = rhai("");
    type_str(&mut editor, &mut session, "Key::");
    assert!(labels(&editor).contains(&"A"));
}

#[test]
fn ctrl_space_lists_without_a_prefix() {
    let (mut editor, mut session) = luau("local speed = 1\n");
    feed(&mut editor, &mut session, &[InputEvent::Complete]);
    assert!(labels(&editor).contains(&"speed"));
    assert!(labels(&editor).contains(&"audio"));
}

#[test]
fn ctrl_space_after_a_separator_lists_only_members() {
    let (mut editor, mut session) = rhai("audio::");
    feed(&mut editor, &mut session, &[InputEvent::Complete]);
    assert!(labels(&editor).contains(&"tone"));
    assert!(!labels(&editor).contains(&"audio"));
}

#[test]
fn comments_and_strings_never_list() {
    for text in ["-- au", "--[[ au", "x = \"au"] {
        let (mut editor, mut session) = luau(text);
        type_str(&mut editor, &mut session, "d");
        assert!(!is_open(&editor), "{text}");
        feed(&mut editor, &mut session, &[InputEvent::Complete]);
        assert!(!is_open(&editor), "{text}");
    }
    let (mut editor, mut session) = rhai("// au");
    type_str(&mut editor, &mut session, "d");
    assert!(!is_open(&editor));
}

#[test]
fn locals_of_unsaved_text_are_offered() {
    let (mut editor, mut session) =
        luau("local velocity = 3\nfunction tick()\n  local vel_x = 1\n  ");
    type_str(&mut editor, &mut session, "ve");
    assert_eq!(labels(&editor)[..2], ["vel_x", "velocity"]);
    assert!(session.is_modified() || text(&session).ends_with("ve"));
}

#[test]
fn locals_out_of_scope_are_not_offered() {
    let (mut editor, mut session) = rhai("fn a() { let hidden = 1; }\nfn b() {\n  ");
    type_str(&mut editor, &mut session, "hi");
    assert!(!labels(&editor).contains(&"hidden"));
}

#[test]
fn a_callback_is_offered_once_at_the_top_level() {
    let (mut editor, mut session) = luau("");
    type_str(&mut editor, &mut session, "tic");
    assert_eq!(labels(&editor), ["tick"]);

    let (mut editor, mut session) = luau("function tick()\nend\n");
    type_str(&mut editor, &mut session, "tic");
    // Only the existing function is offered, as a name and not a second
    // declaration.
    feed(&mut editor, &mut session, &[TAB]);
    assert_eq!(text(&session), "function tick()\nend\ntick");

    for (file, text) in [
        ("a.rhai", "fn a() {\n  \n}"),
        ("a.luau", "function a()\n  \nend"),
    ] {
        let (mut editor, mut session) = editing(file, text);
        let inside = text.find("\n  ").unwrap() + 3;
        let id = session.tabs().active().unwrap();
        let document = session.document(id).unwrap();
        let at = crate::system::code_selection::Selection::collapsed(
            crate::system::code_cursor::Caret::at(document, inside),
        );
        session.tabs_mut().set_selection(id, at);
        type_str(&mut editor, &mut session, "tic");
        assert!(!labels(&editor).contains(&"tick"), "inside {file}");
    }
}

#[test]
fn accepting_a_callback_keeps_indentation_and_puts_the_caret_in_the_body() {
    let (mut editor, mut session) = luau("");
    type_str(&mut editor, &mut session, "tic");
    feed(&mut editor, &mut session, &[TAB]);
    assert_eq!(text(&session), "function tick()\n    \nend");
    assert_eq!(caret(&session), "function tick()\n    ".len());

    let (mut editor, mut session) = rhai("");
    type_str(&mut editor, &mut session, "dra");
    feed(&mut editor, &mut session, &[TAB]);
    assert_eq!(text(&session), "fn draw() {\n    \n}");
    assert_eq!(caret(&session), "fn draw() {\n    ".len());
}

#[test]
fn a_declaration_follows_the_files_line_ending_and_indent() {
    let (mut editor, mut session) = luau("-- a\r\nlocal t = {\r\n\tx = 1,\r\n}\r\n");
    type_str(&mut editor, &mut session, "tic");
    feed(&mut editor, &mut session, &[TAB]);
    let expected = "-- a\r\nlocal t = {\r\n\tx = 1,\r\n}\r\nfunction tick()\r\n\t\r\nend";
    assert_eq!(text(&session), expected);
    assert_eq!(caret(&session), expected.len() - "\r\nend".len());
}

#[test]
fn accepting_replaces_only_the_partial_word() {
    let (mut editor, mut session) = luau("x = au");
    type_str(&mut editor, &mut session, "d");
    feed(&mut editor, &mut session, &[TAB]);
    assert_eq!(text(&session), "x = audio");
    assert_eq!(caret(&session), 9);
}

#[test]
fn accepting_a_member_replaces_what_follows_the_separator() {
    let (mut editor, mut session) = luau("");
    type_str(&mut editor, &mut session, "audio.to");
    feed(&mut editor, &mut session, &[TAB]);
    assert_eq!(text(&session), "audio.tone");
}

#[test]
fn accepting_is_one_undo_step_and_redo_restores_it() {
    let (mut editor, mut session) = luau("");
    type_str(&mut editor, &mut session, "audio.to");
    feed(&mut editor, &mut session, &[TAB]);
    assert_eq!(text(&session), "audio.tone");
    feed(&mut editor, &mut session, &[InputEvent::Undo]);
    assert_eq!(text(&session), "audio.to");
    assert!(!is_open(&editor), "undoing never lists");
    feed(&mut editor, &mut session, &[InputEvent::Redo]);
    assert_eq!(text(&session), "audio.tone");
    assert!(!is_open(&editor));
}

#[test]
fn enter_only_accepts_after_the_pick_moved() {
    let (mut editor, mut session) = luau("");
    type_str(&mut editor, &mut session, "audio.to");
    feed(&mut editor, &mut session, &[InputEvent::Enter]);
    assert_eq!(text(&session), "audio.to\n");

    let (mut editor, mut session) = luau("");
    type_str(&mut editor, &mut session, "audio.");
    feed(&mut editor, &mut session, &[DOWN, InputEvent::Enter]);
    assert!(text(&session).starts_with("audio.") && !text(&session).ends_with('\n'));
    assert_ne!(text(&session), "audio.");
}

#[test]
fn escape_closes_the_list_and_leaves_the_text() {
    let (mut editor, mut session) = luau("");
    type_str(&mut editor, &mut session, "audio.");
    assert!(editor.claims_break());
    feed(&mut editor, &mut session, &[InputEvent::Break]);
    assert!(!is_open(&editor));
    assert_eq!(text(&session), "audio.");
}

#[test]
fn typing_goes_on_and_narrows_the_list() {
    let (mut editor, mut session) = luau("");
    type_str(&mut editor, &mut session, "audio.");
    let all = labels(&editor).len();
    type_str(&mut editor, &mut session, "t");
    assert!(labels(&editor).len() < all);
    assert!(labels(&editor).contains(&"tone"));
    // A letter that matches nothing closes it without eating the key.
    type_str(&mut editor, &mut session, "qqq");
    assert!(!is_open(&editor));
    assert_eq!(text(&session), "audio.tqqq");
}

#[test]
fn a_space_or_punctuation_closes_the_list() {
    for ch in [' ', '(', ',', '='] {
        let (mut editor, mut session) = luau("");
        type_str(&mut editor, &mut session, "audio");
        assert!(is_open(&editor));
        type_str(&mut editor, &mut session, &ch.to_string());
        assert!(!is_open(&editor), "{ch:?}");
        assert_eq!(text(&session), format!("audio{ch}"));
    }
}

#[test]
fn backspace_narrows_an_open_list_and_never_opens_one() {
    let (mut editor, mut session) = luau("audio.to");
    feed(&mut editor, &mut session, &[InputEvent::Backspace]);
    assert!(!is_open(&editor));

    let (mut editor, mut session) = luau("");
    type_str(&mut editor, &mut session, "audio.to");
    feed(&mut editor, &mut session, &[InputEvent::Backspace]);
    assert!(labels(&editor).contains(&"tone"));
    feed(&mut editor, &mut session, &[InputEvent::Backspace]);
    assert!(labels(&editor).len() > 1, "back to every member");
    // Backspacing past the separator leaves the list for `audio`.
    feed(&mut editor, &mut session, &[InputEvent::Backspace]);
    assert_eq!(text(&session), "audio");
}

#[test]
fn backspace_to_an_empty_word_closes_the_list() {
    let (mut editor, mut session) = luau("");
    type_str(&mut editor, &mut session, "re");
    assert!(is_open(&editor));
    feed(&mut editor, &mut session, &[InputEvent::Backspace]);
    feed(&mut editor, &mut session, &[InputEvent::Backspace]);
    assert!(!is_open(&editor));
}

#[test]
fn pasting_and_moving_never_open_the_list() {
    let (mut editor, mut session) = luau("");
    editor.paste_into_document(&mut session, "audio.to");
    assert!(!is_open(&editor));
    type_str(&mut editor, &mut session, " ");
    feed(
        &mut editor,
        &mut session,
        &[
            InputEvent::Left { select: false },
            InputEvent::Right { select: false },
        ],
    );
    assert!(!is_open(&editor));
}

#[test]
fn typing_in_the_middle_of_a_word_does_not_list() {
    let (mut editor, mut session) = luau("co");
    feed(
        &mut editor,
        &mut session,
        &[InputEvent::Left { select: false }],
    );
    type_str(&mut editor, &mut session, "x");
    assert!(!is_open(&editor));
}

#[test]
fn a_selection_blocks_the_list() {
    let (mut editor, mut session) = luau("re");
    feed(
        &mut editor,
        &mut session,
        &[InputEvent::Left { select: true }, InputEvent::Complete],
    );
    assert!(!is_open(&editor));
}

#[test]
fn leaving_the_word_or_the_tab_drops_the_list() {
    let fixture = Fixture::new("code-autocomplete-tabs");
    let (_cartridge, mut session) = session_with_text(&fixture, &["a.luau", "b.luau"], "");
    let mut editor = editor();
    editor.refresh_metrics(&mut session);
    type_str(&mut editor, &mut session, "audio.");
    assert!(is_open(&editor));
    feed(&mut editor, &mut session, &[InputEvent::PreviousDocument]);
    assert!(!is_open(&editor));
    feed(&mut editor, &mut session, &[InputEvent::NextDocument]);
    assert!(!is_open(&editor), "a list is not brought back with the tab");

    type_str(&mut editor, &mut session, "to");
    assert!(is_open(&editor));
    feed(
        &mut editor,
        &mut session,
        &[InputEvent::Left { select: false }],
    );
    assert!(!is_open(&editor));
}

#[test]
fn an_answer_for_another_revision_is_never_applied() {
    let (mut editor, mut session) = luau("");
    type_str(&mut editor, &mut session, "audio.to");
    let id = session.tabs().active().unwrap();
    // A change that did not come through the editor.
    let len = text(&session).len();
    session.edit(id, len..len, "x", EditKind::Typing, None);
    feed(&mut editor, &mut session, &[TAB]);
    assert!(!is_open(&editor));
    assert!(!text(&session).contains("tone"), "{:?}", text(&session));
}

#[test]
fn tab_without_a_list_still_indents() {
    let (mut editor, mut session) = luau("x");
    feed(&mut editor, &mut session, &[TAB]);
    assert_ne!(text(&session), "x");
    assert!(text(&session).starts_with('x') && text(&session).len() > 1);
}

#[test]
fn rapid_typing_ends_with_one_current_list() {
    let (mut editor, mut session) = luau("");
    type_str(&mut editor, &mut session, "audio.tone(1, Key.");
    assert!(labels(&editor).contains(&"A"));
    type_str(&mut editor, &mut session, "Le");
    assert!(
        labels(&editor)
            .iter()
            .all(|l| l.to_lowercase().contains('l'))
    );
    feed(&mut editor, &mut session, &[TAB]);
    assert!(text(&session).starts_with("audio.tone(1, Key.L"));
}

#[test]
fn incomplete_code_still_completes_members() {
    let (mut editor, mut session) = luau("function tick()\n  if btn(Key.");
    type_str(&mut editor, &mut session, "");
    feed(&mut editor, &mut session, &[InputEvent::Complete]);
    assert!(labels(&editor).contains(&"A"));

    let (mut editor, mut session) = rhai("fn tick() {\n  if btn(Key::");
    feed(&mut editor, &mut session, &[InputEvent::Complete]);
    assert!(labels(&editor).contains(&"A"));
}

#[test]
fn constants_and_functions_come_from_the_api_of_the_language() {
    let (mut editor, mut session) = luau("");
    type_str(&mut editor, &mut session, "pr");
    assert!(labels(&editor).contains(&"print"));
    let (mut editor, mut session) = rhai("");
    type_str(&mut editor, &mut session, "pr");
    assert!(!labels(&editor).contains(&"print"));
}

#[test]
fn the_panel_carries_the_documentation_of_the_pick() {
    let (mut editor, mut session) = luau("");
    type_str(&mut editor, &mut session, "audio.to");
    let panel = editor.completion().unwrap().picked_panel().unwrap();
    let signature: String = panel
        .signature
        .iter()
        .map(|(text, _)| text.as_str())
        .collect();
    assert!(signature.starts_with("audio.tone("), "{signature}");
    let summary = panel.summary.as_deref().unwrap();
    assert!(!summary.is_empty());
    assert!(!summary.starts_with("Local"), "{summary}");
}

#[test]
fn a_local_panel_names_its_declaration_line() {
    let (mut editor, mut session) = luau("local speed = 1\n");
    type_str(&mut editor, &mut session, "sp");
    let panel = editor.completion().unwrap().picked_panel().unwrap();
    assert_eq!(
        panel.summary.as_deref(),
        Some("Local variable declared on line 1.")
    );
}
