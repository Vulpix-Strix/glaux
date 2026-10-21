use pretty_assertions::assert_eq;

use super::*;
use crate::cartridge_fs::fixture::Fixture;
use crate::system::code_history::EditKind;
use crate::system::code_session::tests::{cartridge, path};

const A: &str = "line one\nfn target\nline three\n";
const B: &str = "alpha\nbeta\ngamma\n";

fn files() -> [(&'static str, &'static [u8]); 2] {
    [("code/a.luau", A.as_bytes()), ("code/b.luau", B.as_bytes())]
}

fn target(file: &str, at: Spot) -> Destination {
    Destination {
        path: path(file),
        at,
    }
}

fn caret(session: &CodeSession) -> (String, usize) {
    let document = session.active_document().unwrap();
    let at = session
        .tabs()
        .selection(document.id())
        .unwrap()
        .caret()
        .byte();
    (document.path().as_str().to_owned(), at)
}

/// A session with `a` open and its caret at the start of `a`'s second line.
fn started(fixture: &Fixture) -> (LoadedCartridge, CodeSession) {
    let cart = cartridge(fixture, &files());
    let mut session = CodeSession::default();
    session
        .jump(&cart, &target("code/a.luau", Spot::Offset(9)))
        .unwrap();
    (cart, session)
}

#[test]
fn back_and_forward_walk_between_files_and_return() {
    let fixture = Fixture::new("nav-round-trip");
    let (cart, mut session) = started(&fixture);
    session
        .jump(&cart, &target("code/b.luau", Spot::Offset(6)))
        .unwrap();
    assert_eq!(caret(&session), ("code/b.luau".to_owned(), 6));

    let arrival = session.go_back(&cart).unwrap();
    assert_eq!(arrival.landing, Landing::Exact);
    assert_eq!(caret(&session), ("code/a.luau".to_owned(), 9));
    assert!(session.can_go_forward());

    session.go_forward(&cart).unwrap();
    assert_eq!(caret(&session), ("code/b.luau".to_owned(), 6));
}

#[test]
fn going_back_and_forward_adds_no_entries() {
    let fixture = Fixture::new("nav-no-growth");
    let (cart, mut session) = started(&fixture);
    session
        .jump(&cart, &target("code/b.luau", Spot::Offset(6)))
        .unwrap();
    for _ in 0..5 {
        session.go_back(&cart).unwrap();
        session.go_forward(&cart).unwrap();
    }
    session.go_back(&cart).unwrap();
    assert_eq!(caret(&session), ("code/a.luau".to_owned(), 9));
    // The jump that started in `a` is the only entry there was.
    assert!(session.go_back(&cart).is_none());
}

#[test]
fn a_new_jump_forgets_what_was_ahead() {
    let fixture = Fixture::new("nav-clears-forward");
    let (cart, mut session) = started(&fixture);
    session
        .jump(&cart, &target("code/b.luau", Spot::Offset(6)))
        .unwrap();
    session.go_back(&cart).unwrap();
    assert!(session.can_go_forward());
    session
        .jump(&cart, &target("code/a.luau", Spot::Offset(0)))
        .unwrap();
    assert!(!session.can_go_forward());
}

#[test]
fn a_jump_to_where_the_caret_already_is_records_nothing() {
    let fixture = Fixture::new("nav-same-spot");
    let (cart, mut session) = started(&fixture);
    session
        .jump(&cart, &target("code/a.luau", Spot::Offset(9)))
        .unwrap();
    assert!(!session.can_go_back());
}

#[test]
fn moving_the_caret_or_switching_tabs_is_not_a_jump() {
    let fixture = Fixture::new("nav-ignores-moves");
    let (cart, mut session) = started(&fixture);
    let id = session.active_document().unwrap().id();
    let document = session.document(id).unwrap();
    let moved = Selection::collapsed(Caret::at(document, 2));
    session.tabs_mut().set_selection(id, moved);
    session.open(&cart, &path("code/b.luau")).unwrap();
    session.tabs_mut().activate(id);
    assert!(!session.can_go_back());
}

#[test]
fn a_line_target_lands_at_the_start_of_that_line() {
    let fixture = Fixture::new("nav-line");
    let cart = cartridge(&fixture, &files());
    let mut session = CodeSession::default();
    session
        .jump(&cart, &target("code/a.luau", Spot::Line(2)))
        .unwrap();
    assert_eq!(caret(&session), ("code/a.luau".to_owned(), 19));
}

#[test]
fn a_failed_jump_changes_nothing() {
    let fixture = Fixture::new("nav-failed");
    let (cart, mut session) = started(&fixture);
    assert!(
        session
            .jump(&cart, &target("code/missing.luau", Spot::Offset(0)))
            .is_err()
    );
    assert_eq!(caret(&session), ("code/a.luau".to_owned(), 9));
    assert!(!session.can_go_back());
}

#[test]
fn jumps_leave_the_undo_history_and_other_tabs_alone() {
    let fixture = Fixture::new("nav-not-an-edit");
    let (cart, mut session) = started(&fixture);
    let a = session.active_document().unwrap().id();
    session
        .edit(a, 0..0, "x", EditKind::Typing, None)
        .unwrap()
        .unwrap();
    let b = session.open(&cart, &path("code/b.luau")).unwrap();
    let before = session.tabs().selection(b).unwrap();
    session
        .jump(&cart, &target("code/a.luau", Spot::Offset(3)))
        .unwrap();
    assert_eq!(session.tabs().selection(b), Some(before));
    assert!(session.can_undo(a));
    session.go_back(&cart).unwrap();
    assert!(session.can_undo(a));
    assert_eq!(session.document(a).unwrap().text(), format!("x{A}"));
}

#[test]
fn an_edit_that_leaves_the_line_alone_keeps_the_place() {
    let fixture = Fixture::new("nav-shifted");
    let (cart, mut session) = started(&fixture);
    let a = session.active_document().unwrap().id();
    session
        .jump(&cart, &target("code/b.luau", Spot::Offset(6)))
        .unwrap();
    // Appending a line below does not move the remembered one.
    let end = session.document(a).unwrap().text().len();
    session
        .edit(a, end..end, "tail\n", EditKind::Typing, None)
        .unwrap()
        .unwrap();
    let arrival = session.go_back(&cart).unwrap();
    assert_eq!(arrival.landing, Landing::Exact);
    assert_eq!(arrival.top_line, None);
    assert_eq!(caret(&session), ("code/a.luau".to_owned(), 9));
}

#[test]
fn text_inserted_above_moves_the_line_so_the_place_is_reported_changed() {
    let fixture = Fixture::new("nav-moved-line");
    let (cart, mut session) = started(&fixture);
    let a = session.active_document().unwrap().id();
    session
        .jump(&cart, &target("code/b.luau", Spot::Offset(6)))
        .unwrap();
    session
        .edit(a, 0..0, "new\n", EditKind::Typing, None)
        .unwrap()
        .unwrap();
    let arrival = session.go_back(&cart).unwrap();
    // Line 1 now holds "line one", not "fn target": no silent landing on the wrong text.
    assert_eq!(arrival.landing, Landing::LineChanged);
    assert_eq!(caret(&session), ("code/a.luau".to_owned(), 4));
}

#[test]
fn a_place_past_the_end_of_a_shortened_document_is_skipped() {
    let fixture = Fixture::new("nav-line-gone");
    let (cart, mut session) = started(&fixture);
    let a = session.active_document().unwrap().id();
    session
        .jump(&cart, &target("code/b.luau", Spot::Offset(6)))
        .unwrap();
    let end = session.document(a).unwrap().text().len();
    session
        .edit(a, 0..end, "short", EditKind::Typing, None)
        .unwrap()
        .unwrap();
    assert!(session.go_back(&cart).is_none());
    assert_eq!(caret(&session), ("code/b.luau".to_owned(), 6));
}

/// `a` at 9, then `b` at 6, then back in `a`: going back leads to `b`, then to `a`.
fn visited_b(fixture: &Fixture) -> (LoadedCartridge, CodeSession, DocumentId) {
    let (cart, mut session) = started(fixture);
    let b = session
        .jump(&cart, &target("code/b.luau", Spot::Offset(6)))
        .unwrap();
    session
        .jump(&cart, &target("code/a.luau", Spot::Offset(0)))
        .unwrap();
    (cart, session, b)
}

#[test]
fn a_closed_file_is_reopened_when_going_back() {
    let fixture = Fixture::new("nav-closed");
    let (cart, mut session, b) = visited_b(&fixture);
    session.close(b);
    let arrival = session.go_back(&cart).unwrap();
    assert_eq!(arrival.landing, Landing::Exact);
    assert_eq!(caret(&session), ("code/b.luau".to_owned(), 6));
}

#[test]
fn a_deleted_file_is_skipped_and_leaves_no_tab_behind() {
    let fixture = Fixture::new("nav-deleted");
    let (cart, mut session, b) = visited_b(&fixture);
    session.close(b);
    std::fs::remove_file(fixture.root().join("games/pong/code/b.luau")).unwrap();
    let arrival = session.go_back(&cart).unwrap();
    assert_eq!(arrival.skipped, 1);
    assert_eq!(caret(&session), ("code/a.luau".to_owned(), 9));
    assert!(!session.is_open(&path("code/b.luau")));
}

#[test]
fn places_follow_a_renamed_script() {
    let fixture = Fixture::new("nav-renamed");
    let (cart, mut session) = started(&fixture);
    session
        .jump(&cart, &target("code/b.luau", Spot::Offset(6)))
        .unwrap();
    let a = session
        .tabs()
        .ids()
        .find(|&id| session.document(id).unwrap().path().as_str() == "code/a.luau")
        .unwrap();
    session
        .move_script(&cart, a, &path("code/renamed.luau"))
        .unwrap();
    session.go_back(&cart).unwrap();
    assert_eq!(caret(&session), ("code/renamed.luau".to_owned(), 9));
}

#[test]
fn the_history_keeps_only_the_most_recent_places() {
    let fixture = Fixture::new("nav-capacity");
    let (cart, mut session) = started(&fixture);
    for n in 0..(CAPACITY + 20) {
        let file = if n % 2 == 0 {
            "code/b.luau"
        } else {
            "code/a.luau"
        };
        session
            .jump(&cart, &target(file, Spot::Offset(n % 5)))
            .unwrap();
    }
    let mut steps = 0;
    while session.go_back(&cart).is_some() {
        steps += 1;
    }
    assert_eq!(steps, CAPACITY);
}
