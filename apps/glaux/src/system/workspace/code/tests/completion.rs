//! Picking and accepting suggestions in the Code view.

use std::ops::Range;

use crate::system::editing::history::EditKind;
use glaux_sense::{Candidate, CandidateKind, Completions, Doc, Insertion, Origin};

use super::selection::{caret, open, plain, right};
use super::*;
use pretty_assertions::assert_eq;

const DOWN: InputEvent = InputEvent::Down { select: false };
const TAB: InputEvent = InputEvent::Tab { reverse: false };

fn text(session: &CodeSession) -> &str {
    session.active_document().unwrap().text()
}

pub(super) fn suggestion(label: &str) -> Candidate {
    Candidate {
        label: label.to_owned(),
        kind: CandidateKind::Function,
        origin: Origin::Api,
        insertion: Insertion::Name(label.to_owned()),
        detail: "gfx".to_owned(),
        doc: Doc::None,
    }
}

/// Lists `labels` for the word at `anchor` of `session`'s active document,
/// with the caret at the word's end.
pub(super) fn list(
    editor: &mut CodeEditor,
    session: &mut CodeSession,
    labels: &[&str],
    anchor: Range<usize>,
) {
    list_with_panel(editor, session, labels, anchor, None);
}

/// As [`list`], with `panel` as the documentation of the first suggestion.
pub(in crate::system::workspace::code) fn list_with_panel(
    editor: &mut CodeEditor,
    session: &mut CodeSession,
    labels: &[&str],
    anchor: Range<usize>,
    panel: Option<crate::system::workspace::code::completion::DocPanel>,
) {
    let document = session.active_document().unwrap();
    let id = document.id();
    let completions = Completions {
        revision: document.revision(),
        anchor: anchor.clone(),
        prefix: String::new(),
        items: labels.iter().map(|label| suggestion(label)).collect(),
    };
    let document = session.document(id).unwrap();
    let selection = Selection::collapsed(crate::system::editing::cursor::Caret::at(
        document, anchor.end,
    ));
    session.tabs_mut().set_selection(id, selection);
    let panels = labels.iter().map(|_| panel.clone()).collect();
    editor.show_completions(id, completions, panels);
}

fn listing(text: &str, labels: &[&str], anchor: Range<usize>) -> (CodeEditor, CodeSession) {
    let fixture = Fixture::new("code-completion");
    let (mut editor, _, mut session) = open(&fixture, text, EditorSettings::default());
    list(&mut editor, &mut session, labels, anchor);
    (editor, session)
}

#[test]
fn tab_replaces_the_word_with_the_picked_suggestion() {
    let (mut editor, mut session) = listing("x = au", &["audio", "auto"], 4..6);
    feed(&mut editor, &mut session, &[DOWN, TAB]);
    assert_eq!((text(&session), caret(&session)), ("x = auto", 8));
    assert_eq!(editor.completion(), None);
}

#[test]
fn accepting_is_a_single_step_of_undo() {
    let (mut editor, mut session) = listing("x = au", &["audio"], 4..6);
    feed(&mut editor, &mut session, &[TAB]);
    assert_eq!(text(&session), "x = audio");
    feed(&mut editor, &mut session, &[InputEvent::Undo]);
    assert_eq!((text(&session), caret(&session)), ("x = au", 6));
}

#[test]
fn a_declaration_leaves_the_caret_in_its_body() {
    let fixture = Fixture::new("code-completion-declaration");
    let (mut editor, _, mut session) = open(&fixture, "ti", EditorSettings::default());
    list(&mut editor, &mut session, &["tick"], 0..2);
    let id = session.tabs().active().unwrap();
    let revision = session.document(id).unwrap().revision();
    let mut tick = suggestion("tick");
    tick.insertion = Insertion::Declaration {
        text: "function tick()\n  \nend".to_owned(),
        caret_in_text: 18,
    };
    editor.show_completions(
        id,
        Completions {
            revision,
            anchor: 0..2,
            prefix: "ti".to_owned(),
            items: vec![tick],
        },
        Vec::new(),
    );
    feed(&mut editor, &mut session, &[TAB]);
    assert_eq!(text(&session), "function tick()\n  \nend");
    assert_eq!(caret(&session), 18);
}

#[test]
fn enter_is_a_line_break_until_the_user_picks() {
    let (mut editor, mut session) = listing("au", &["audio", "auto"], 0..2);
    feed(&mut editor, &mut session, &[InputEvent::Enter]);
    assert_eq!((text(&session), caret(&session)), ("au\n", 3));
    assert_eq!(editor.completion(), None);
}

#[test]
fn enter_accepts_once_the_user_has_moved_the_pick() {
    let (mut editor, mut session) = listing("au", &["audio", "auto"], 0..2);
    feed(&mut editor, &mut session, &[DOWN, InputEvent::Enter]);
    assert_eq!(text(&session), "auto");
}

#[test]
fn up_and_down_move_the_pick_instead_of_the_caret_while_listed() {
    let fixture = Fixture::new("code-completion-arrows");
    let (mut editor, _, mut session) = open(&fixture, "au\nzz", EditorSettings::default());
    list(&mut editor, &mut session, &["audio", "auto"], 0..2);
    feed(&mut editor, &mut session, &[DOWN]);
    assert_eq!(caret(&session), 2);
    assert_eq!(editor.completion().unwrap().selected(), 1);
    // Closed, the same key moves the caret as it always did.
    feed(&mut editor, &mut session, &[InputEvent::Break, DOWN]);
    assert_eq!(caret(&session), 5);
}

#[test]
fn escape_closes_the_list_and_stays_in_the_view() {
    let (mut editor, mut session) = listing("au", &["audio"], 0..2);
    assert!(editor.claims_break());
    assert_eq!(editor.hint(), "Up/Down: pick  Tab: accept  Esc: close");
    feed(&mut editor, &mut session, &[InputEvent::Break]);
    assert_eq!(editor.completion(), None);
    assert!(!editor.claims_break());
    assert_eq!(text(&session), "au");
}

#[test]
fn typing_goes_on_to_the_text_and_leaves_no_stale_list() {
    let (mut editor, mut session) = listing("au", &["audio"], 0..2);
    feed(&mut editor, &mut session, &[InputEvent::Text('x')]);
    assert_eq!(text(&session), "aux");
    assert_eq!(editor.completion(), None);
}

#[test]
fn moving_the_caret_closes_the_list() {
    let (mut editor, mut session) = listing("au", &["audio"], 0..2);
    feed(&mut editor, &mut session, &[plain(super::selection::left)]);
    assert_eq!(editor.completion(), None);
    assert_eq!(caret(&session), 1);
}

#[test]
fn a_list_for_other_text_is_never_applied() {
    let (mut editor, mut session) = listing("au", &["audio"], 0..2);
    let id = session.tabs().active().unwrap();
    // The text changes without the editor seeing a key.
    session.edit(id, 0..0, "z", EditKind::Typing, None);
    feed(&mut editor, &mut session, &[TAB]);
    // Tab is then only indentation, and nothing is inserted from the list.
    assert!(!text(&session).contains("audio"));
    assert_eq!(editor.completion(), None);
}

#[test]
fn clicking_a_suggestion_accepts_it_and_clicking_elsewhere_only_closes() {
    let (mut editor, mut session) = listing("au", &["audio", "auto", "autumn"], 0..2);
    let layout = editor.completion_layout(&session).unwrap();
    let row = layout.rows[2].1;
    let at = Point::new(row.x as u16 + 2, row.y as u16 + 2);
    feed(
        &mut editor,
        &mut session,
        &[press(PointerButton::Primary, at)],
    );
    assert_eq!(text(&session), "autumn");

    let (mut editor, mut session) = listing("au", &["audio"], 0..2);
    let away = Point::new(2, 200);
    feed(
        &mut editor,
        &mut session,
        &[press(PointerButton::Primary, away)],
    );
    assert_eq!((text(&session), editor.completion()), ("au", None));
}

#[test]
fn forgetting_the_cartridge_closes_the_list() {
    let (mut editor, mut session) = listing("au", &["audio"], 0..2);
    editor.forget();
    assert_eq!(editor.completion(), None);
    feed(&mut editor, &mut session, &[TAB]);
    assert_ne!(text(&session), "audio");
}

#[test]
fn tab_indents_when_nothing_is_listed() {
    let fixture = Fixture::new("code-completion-tab");
    let (mut editor, _, mut session) = open(&fixture, "", EditorSettings::default());
    feed(&mut editor, &mut session, &[TAB, plain(right)]);
    assert!(!text(&session).is_empty());
    assert!(text(&session).trim().is_empty());
}
