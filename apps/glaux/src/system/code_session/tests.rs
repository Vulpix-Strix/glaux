use std::fs;

use super::*;
use crate::cartridge::Language;
use crate::cartridge::fixture::write_cartridge;
use crate::cartridge_fs::fixture::Fixture;
use crate::system::code_document::CodeDocumentError;
use pretty_assertions::assert_eq;

/// A Luau cartridge at `/games/pong` with `files` written into it.
fn cartridge(fixture: &Fixture, files: &[(&str, &[u8])]) -> LoadedCartridge {
    let dir = fixture.root().join("games/pong");
    write_cartridge(&dir, "Pong");
    for (path, bytes) in files {
        let host = dir.join(path);
        fs::create_dir_all(host.parent().unwrap()).unwrap();
        fs::write(host, bytes).unwrap();
    }
    LoadedCartridge::load(&fixture.storage(), "games/pong").unwrap()
}

fn path(path: &str) -> SourcePath {
    SourcePath::parse(path).unwrap()
}

fn open_paths(session: &CodeSession) -> Vec<&str> {
    session
        .tabs()
        .ids()
        .map(|id| session.document(id).unwrap().path().as_str())
        .collect()
}

#[test]
fn a_file_opened_twice_has_one_document_and_one_tab() {
    let fixture = Fixture::new("code-session-dup");
    let cart = cartridge(&fixture, &[("code/a.luau", b"a")]);
    let mut session = CodeSession::default();

    let main = session.open(&cart, &path("code/main.luau")).unwrap();
    let a = session.open(&cart, &path("code/a.luau")).unwrap();
    assert_eq!(session.open(&cart, &path("code/main.luau")), Ok(main));
    assert_eq!(open_paths(&session), ["code/main.luau", "code/a.luau"]);
    assert_eq!(session.tabs().active(), Some(main));
    assert_ne!(main, a);
}

#[test]
fn nested_luau_and_rhai_files_open_side_by_side() {
    let fixture = Fixture::new("code-session-mixed");
    let cart = cartridge(
        &fixture,
        &[
            ("code/enemies/bat.luau", b"return 1"),
            ("code/ui/hud.rhai", b"let a = 1;"),
        ],
    );
    let mut session = CodeSession::default();

    session.open(&cart, &path("code/enemies/bat.luau")).unwrap();
    session.open(&cart, &path("code/ui/hud.rhai")).unwrap();
    let active = session.active_document().unwrap();
    assert_eq!(active.path().as_str(), "code/ui/hud.rhai");
    assert_eq!(active.language(), Language::Rhai);
    assert_eq!(active.text(), "let a = 1;");
    assert!(session.is_open(&path("code/enemies/bat.luau")));
    assert!(!session.is_open(&path("code/main.luau")));
}

#[test]
fn a_file_that_fails_to_open_changes_nothing() {
    let fixture = Fixture::new("code-session-fail");
    let cart = cartridge(
        &fixture,
        &[("code/bad.luau", b"ok\n\xff"), ("code/dir.luau/x", b"")],
    );
    let mut session = CodeSession::default();
    let main = session.open(&cart, &path("code/main.luau")).unwrap();

    for (file, expected) in [
        (
            "code/missing.luau",
            CodeDocumentError::NotFound(path("code/missing.luau")),
        ),
        (
            "code/dir.luau",
            CodeDocumentError::NotAFile(path("code/dir.luau")),
        ),
        (
            "code/bad.luau",
            CodeDocumentError::InvalidUtf8 {
                path: path("code/bad.luau"),
                line: 2,
            },
        ),
    ] {
        assert_eq!(session.open(&cart, &path(file)), Err(expected), "{file}");
        assert_eq!(open_paths(&session), ["code/main.luau"], "{file}");
        assert_eq!(session.tabs().active(), Some(main), "{file}");
    }
}

#[test]
fn closing_removes_the_tab_and_the_document() {
    let fixture = Fixture::new("code-session-close");
    let cart = cartridge(&fixture, &[("code/a.luau", b"before")]);
    let mut session = CodeSession::default();
    let a = session.open(&cart, &path("code/a.luau")).unwrap();

    assert!(session.close(a));
    assert_eq!(session.document(a), None);
    assert!(!session.is_open(&path("code/a.luau")));
    assert!(!session.close(a));

    // Opening again reads the file as it is now, under a new id.
    fs::write(fixture.root().join("games/pong/code/a.luau"), "after").unwrap();
    let again = session.open(&cart, &path("code/a.luau")).unwrap();
    assert_ne!(again, a);
    assert_eq!(session.document(again).unwrap().text(), "after");
}

#[test]
fn the_entry_is_requested_once() {
    let mut session = CodeSession::default();
    assert!(session.take_entry_request());
    assert!(!session.take_entry_request());
}
