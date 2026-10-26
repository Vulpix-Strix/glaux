use std::fs;

use super::*;
use crate::cartridge::Language;
use crate::cartridge::fixture::write_cartridge;
use crate::cartridge_fs::fixture::Fixture;
use crate::system::editing::cursor::Caret;
use crate::system::editing::document::CodeDocumentError;
use glaux_docs::DocId;
use pretty_assertions::assert_eq;

/// A Luau cartridge at `/games/pong` with `files` written into it.
pub(in crate::system) fn cartridge(fixture: &Fixture, files: &[(&str, &[u8])]) -> LoadedCartridge {
    let dir = fixture.root().join("games/pong");
    write_cartridge(&dir, "Pong");
    for (path, bytes) in files {
        let host = dir.join(path);
        fs::create_dir_all(host.parent().unwrap()).unwrap();
        fs::write(host, bytes).unwrap();
    }
    LoadedCartridge::load(&fixture.storage(), "games/pong").unwrap()
}

pub(in crate::system) fn path(path: &str) -> SourcePath {
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

/// Types `text` into `id` at `at` the way the Code view does: one
/// character at a time, each from where the last left the caret.
fn type_at(session: &mut CodeSession, id: DocumentId, at: usize, text: &str) {
    let mut at = at;
    for ch in text.chars() {
        let document = session.document(id).unwrap();
        let selection = Selection::collapsed(Caret::at(document, at));
        session.tabs_mut().set_selection(id, selection);
        let typed = ch.to_string();
        session
            .edit(id, at..at, &typed, EditKind::Typing, None)
            .unwrap()
            .unwrap();
        at += typed.len();
    }
}

fn text(session: &CodeSession, id: DocumentId) -> &str {
    session.document(id).unwrap().text()
}

#[test]
fn undo_follows_the_saved_text_for_dirty_state_and_keeps_history_across_saves() {
    let fixture = Fixture::new("code-session-undo-save");
    let cart = cartridge(&fixture, &[("code/a.luau", b"x")]);
    let mut session = CodeSession::default();
    let a = session.open(&cart, &path("code/a.luau")).unwrap();

    type_at(&mut session, a, 1, "ab");
    session.save(&cart, a).unwrap();
    assert!(!session.is_modified());
    // Saving ends the run, so undo stops at the saved text.
    type_at(&mut session, a, 3, "cd");
    assert!(session.is_modified());

    assert!(session.undo(a).is_some());
    assert_eq!(text(&session, a), "xab");
    assert!(!session.is_modified());
    assert!(session.undo(a).is_some());
    assert_eq!(text(&session, a), "x");
    assert!(session.is_modified());
    assert_eq!(session.modified().len(), 1);

    assert!(session.redo(a).is_some());
    assert!(!session.is_modified());
    assert!(session.modified().is_empty());
}

#[test]
fn documents_keep_separate_histories_until_closed() {
    let fixture = Fixture::new("code-session-undo-docs");
    let cart = cartridge(&fixture, &[("code/a.luau", b""), ("code/b.luau", b"")]);
    let mut session = CodeSession::default();
    let a = session.open(&cart, &path("code/a.luau")).unwrap();
    let b = session.open(&cart, &path("code/b.luau")).unwrap();
    type_at(&mut session, a, 0, "aa");
    type_at(&mut session, b, 0, "bb");

    assert!(session.undo(a).is_some());
    assert_eq!((text(&session, a), text(&session, b)), ("", "bb"));
    assert_eq!(session.undo(a), None);

    session.close(b);
    let b = session.open(&cart, &path("code/b.luau")).unwrap();
    assert_eq!(session.undo(b), None);
}

mod moving {
    use super::*;
    use crate::system::editing::document::{CodeRenameError, DiskState};
    use crate::system::editing::history::EditKind;
    use pretty_assertions::assert_eq;

    fn host(fixture: &Fixture, file: &str) -> std::path::PathBuf {
        fixture.root().join("games/pong").join(file)
    }

    /// `code/a.luau` holding `base`, opened and edited to `base!`.
    fn edited(
        fixture: &Fixture,
        other: &[(&str, &[u8])],
    ) -> (LoadedCartridge, CodeSession, DocumentId) {
        let mut files = vec![("code/a.luau", b"base".as_slice())];
        files.extend_from_slice(other);
        let cart = cartridge(fixture, &files);
        let mut session = CodeSession::default();
        let id = session.open(&cart, &path("code/a.luau")).unwrap();
        session
            .edit(id, 4..4, "!", EditKind::Typing, None)
            .unwrap()
            .unwrap();
        (cart, session, id)
    }

    #[test]
    fn a_moved_document_keeps_its_id_tab_edits_history_and_modified_state() {
        let fixture = Fixture::new("code-session-move");
        let (cart, mut session, id) = edited(&fixture, &[("code/b.luau", b"b")]);
        let b = session.open(&cart, &path("code/b.luau")).unwrap();
        session.tabs_mut().activate(id);

        session
            .move_script(&cart, id, &path("code/lib/renamed.luau"))
            .unwrap();

        let document = session.document(id).unwrap();
        assert_eq!(document.path().as_str(), "code/lib/renamed.luau");
        assert_eq!(document.text(), "base!");
        assert!(document.is_modified());
        assert_eq!(session.tabs().ids().collect::<Vec<_>>(), [id, b]);
        assert_eq!(session.tabs().active(), Some(id));
        assert!(session.is_open(&path("code/lib/renamed.luau")));
        assert!(!session.is_open(&path("code/a.luau")));
        assert_eq!(
            std::fs::read(host(&fixture, "code/lib/renamed.luau")).unwrap(),
            b"base"
        );
        assert!(!host(&fixture, "code/a.luau").exists());

        // The history survived: undo returns to the unmodified text.
        assert!(session.can_undo(id));
        session.undo(id).unwrap();
        assert!(!session.document(id).unwrap().is_modified());
    }

    #[test]
    fn saving_after_a_move_writes_the_new_path_only() {
        let fixture = Fixture::new("code-session-move-save");
        let (cart, mut session, id) = edited(&fixture, &[]);
        session
            .move_script(&cart, id, &path("code/z.luau"))
            .unwrap();

        session.save(&cart, id).unwrap();
        assert_eq!(
            std::fs::read(host(&fixture, "code/z.luau")).unwrap(),
            b"base!"
        );
        assert!(!host(&fixture, "code/a.luau").exists());
        assert!(!session.is_modified());
    }

    #[test]
    fn a_path_open_in_another_tab_is_refused_before_anything_moves() {
        let fixture = Fixture::new("code-session-move-taken");
        let (cart, mut session, id) = edited(&fixture, &[("code/b.luau", b"b")]);
        session.open(&cart, &path("code/b.luau")).unwrap();

        assert_eq!(
            session.move_script(&cart, id, &path("code/b.luau")),
            Err(MoveScriptError::Document(CodeRenameError::PathTaken(path(
                "code/b.luau"
            ))))
        );
        assert_eq!(session.document(id).unwrap().path().as_str(), "code/a.luau");
        assert_eq!(
            std::fs::read(host(&fixture, "code/a.luau")).unwrap(),
            b"base"
        );
        assert_eq!(std::fs::read(host(&fixture, "code/b.luau")).unwrap(), b"b");
    }

    #[test]
    fn a_file_in_the_way_that_is_not_open_is_never_replaced() {
        let fixture = Fixture::new("code-session-move-exists");
        let (cart, mut session, id) = edited(&fixture, &[("code/b.luau", b"b")]);

        let result = session.move_script(&cart, id, &path("code/b.luau"));
        assert!(
            matches!(result, Err(MoveScriptError::File(_))),
            "{result:?}"
        );
        assert_eq!(session.document(id).unwrap().path().as_str(), "code/a.luau");
        assert_eq!(std::fs::read(host(&fixture, "code/b.luau")).unwrap(), b"b");
        assert_eq!(
            std::fs::read(host(&fixture, "code/a.luau")).unwrap(),
            b"base"
        );
    }

    #[test]
    fn a_file_changed_or_deleted_outside_glaux_stays_where_it_is() {
        let fixture = Fixture::new("code-session-move-outside");
        let (cart, mut session, id) = edited(&fixture, &[]);
        let a = host(&fixture, "code/a.luau");

        std::fs::write(&a, "changed elsewhere").unwrap();
        assert_eq!(
            session.disk_state(&cart, id),
            Some(Ok(DiskState::Modified(b"changed elsewhere".to_vec())))
        );
        assert_eq!(
            session.move_script(&cart, id, &path("code/z.luau")),
            Err(MoveScriptError::Changed(path("code/a.luau")))
        );
        assert_eq!(std::fs::read(&a).unwrap(), b"changed elsewhere");

        std::fs::remove_file(&a).unwrap();
        assert_eq!(session.disk_state(&cart, id), Some(Ok(DiskState::Missing)));
        assert_eq!(
            session.move_script(&cart, id, &path("code/z.luau")),
            Err(MoveScriptError::Missing(path("code/a.luau")))
        );
        assert_eq!(session.document(id).unwrap().path().as_str(), "code/a.luau");
        assert_eq!(session.document(id).unwrap().text(), "base!");
        assert!(!host(&fixture, "code/z.luau").exists());
    }

    #[test]
    fn the_entry_script_and_other_languages_are_refused_and_the_same_path_is_a_no_op() {
        let fixture = Fixture::new("code-session-move-refused");
        let (cart, mut session, id) = edited(&fixture, &[]);
        let main = session.open(&cart, &path("code/main.luau")).unwrap();

        assert!(matches!(
            session.move_script(&cart, main, &path("code/x.luau")),
            Err(MoveScriptError::File(SourceEditError::Entry(_)))
        ));
        assert_eq!(
            session.move_script(&cart, id, &path("code/a.rhai")),
            Err(MoveScriptError::Document(CodeRenameError::Language(path(
                "code/a.rhai"
            ))))
        );
        assert_eq!(session.move_script(&cart, id, &path("code/a.luau")), Ok(()));
        assert_eq!(session.document(id).unwrap().path().as_str(), "code/a.luau");
        assert!(host(&fixture, "code/main.luau").exists());
    }
}

#[test]
fn the_symbol_at_a_caret_is_a_page_a_local_or_unknown_in_both_languages() {
    let fixture = Fixture::new("code-session-symbol");
    let luau = "local function mine() end\nspr(1)\naudio.tone(1)\nnothing_here()\n-- spr\n";
    let rhai = "fn mine() {}\nspr(1);\naudio::tone(1);\nnothing_here();\n";
    let cart = cartridge(
        &fixture,
        &[
            ("code/a.luau", luau.as_bytes()),
            ("code/b.rhai", rhai.as_bytes()),
        ],
    );
    let api = glaux_docs::Library::embedded();
    let api = api.api();
    let mut session = CodeSession::default();
    for (file, text, member) in [
        ("code/a.luau", luau, "audio.tone"),
        ("code/b.rhai", rhai, "audio::tone"),
    ] {
        let id = session.open(&cart, &path(file)).unwrap();
        let at = |needle: &str, inside: usize| text.find(needle).unwrap() + inside;
        let mut symbol = |caret| session.symbol_at(id, caret, api);
        let spr = Symbol::Api(DocId::parse("api/graphics/spr").unwrap());
        assert_eq!(symbol(at("spr(", 0)), Some(spr.clone()), "{file} start");
        assert_eq!(symbol(at("spr(", 3)), Some(spr), "{file} end");
        let tone = Symbol::Api(DocId::parse("api/audio/tone").unwrap());
        assert_eq!(
            symbol(at(member, member.len() - 2)),
            Some(tone),
            "{file} member"
        );
        assert_eq!(
            symbol(at("mine", 2)),
            Some(Symbol::Local("mine".to_owned()))
        );
        assert_eq!(symbol(at("nothing_here", 3)), Some(Symbol::Unknown));
        // Between two non-word characters there is no word.
        assert_eq!(symbol(at("(1)", 0) + 1 + 1), None, "{file} after the digit");
    }
    // A comment offers nothing, even for a name of the API.
    let id = session.open(&cart, &path("code/a.luau")).unwrap();
    let caret = luau.rfind("spr").unwrap() + 1;
    assert_eq!(session.symbol_at(id, caret, api), None);
}
