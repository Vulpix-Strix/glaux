use std::fs;
use std::path::PathBuf;

use super::*;
use crate::cartridge::fixture::write_cartridge;
use crate::cartridge_fs::fixture::Fixture;
use pretty_assertions::assert_eq;

/// A Luau cartridge at `/games/pong` with `files` written into it.
fn cartridge(fixture: &Fixture, files: &[(&str, &[u8])]) -> (LoadedCartridge, PathBuf) {
    let dir = fixture.root().join("games/pong");
    write_cartridge(&dir, "Pong");
    for (path, bytes) in files {
        let host = dir.join(path);
        fs::create_dir_all(host.parent().unwrap()).unwrap();
        fs::write(host, bytes).unwrap();
    }
    let loaded = LoadedCartridge::load(&fixture.storage(), "games/pong").unwrap();
    (loaded, dir)
}

fn path(path: &str) -> SourcePath {
    SourcePath::parse(path).unwrap()
}

#[test]
fn opens_the_file_text_exactly_as_stored() {
    let fixture = Fixture::new("code-doc-exact");
    let source = b"\xef\xbb\xbflocal x = 1\r\n\r\nreturn x";
    let (cart, _) = cartridge(&fixture, &[("code/player.luau", source)]);
    let mut docs = CodeDocuments::default();

    let id = docs.open(&cart, &path("code/player.luau")).unwrap();
    let doc = docs.get(id).unwrap();
    assert_eq!(doc.id(), id);
    assert_eq!(doc.path().as_str(), "code/player.luau");
    assert_eq!(doc.language(), Language::Luau);
    assert_eq!(doc.text().as_bytes(), source);
    assert_eq!(doc.origin().bytes(), source);
}

#[test]
fn language_follows_each_file_not_the_manifest() {
    let fixture = Fixture::new("code-doc-lang");
    let (cart, _) = cartridge(&fixture, &[("code/hud.rhai", b"let a = 1;")]);
    let mut docs = CodeDocuments::default();

    let hud = docs.open(&cart, &path("code/hud.rhai")).unwrap();
    let main = docs.open(&cart, &path("code/main.luau")).unwrap();
    assert_eq!(docs.get(hud).unwrap().language(), Language::Rhai);
    assert_eq!(docs.get(main).unwrap().language(), Language::Luau);
}

#[test]
fn each_file_is_open_at_most_once() {
    let fixture = Fixture::new("code-doc-identity");
    let (cart, dir) = cartridge(&fixture, &[("code/a.luau", b"a"), ("code/b.luau", b"b")]);
    let mut docs = CodeDocuments::default();

    let a = docs.open(&cart, &path("code/a.luau")).unwrap();
    let b = docs.open(&cart, &path("code/b.luau")).unwrap();
    assert_ne!(a, b);
    assert_eq!(docs.get(a).unwrap().text(), "a");
    assert_eq!(docs.get(b).unwrap().text(), "b");

    // Opening again neither re-reads the file nor adds a document.
    fs::write(dir.join("code/a.luau"), "changed").unwrap();
    assert_eq!(docs.open(&cart, &path("code/a.luau")), Ok(a));
    assert_eq!(docs.get(a).unwrap().text(), "a");
    assert_eq!(docs.documents.len(), 2);
}

#[test]
fn closing_frees_the_file_and_retires_its_id() {
    let fixture = Fixture::new("code-doc-close");
    let (cart, _) = cartridge(&fixture, &[]);
    let mut docs = CodeDocuments::default();
    let main = path("code/main.luau");

    let first = docs.open(&cart, &main).unwrap();
    assert_eq!(docs.close(first).map(|doc| doc.id()), Some(first));
    assert_eq!(docs.get(first), None);
    assert_eq!(docs.close(first), None);

    let second = docs.open(&cart, &main).unwrap();
    assert_ne!(second, first);
    assert_eq!(
        docs.get(first),
        None,
        "the old id never names the new document"
    );
    assert!(docs.get(second).is_some());
}

#[test]
fn files_that_cannot_be_opened_open_nothing() {
    let fixture = Fixture::new("code-doc-errors");
    let (cart, dir) = cartridge(&fixture, &[("code/bad.luau", b"a\nb\nc\xff\nd")]);
    fs::create_dir_all(dir.join("code/dir.luau")).unwrap();
    let mut docs = CodeDocuments::default();

    let cases = [
        (
            "code/missing.luau",
            CodeDocumentError::NotFound(path("code/missing.luau")),
        ),
        (
            "nowhere/main.luau",
            CodeDocumentError::NotFound(path("nowhere/main.luau")),
        ),
        (
            "code/main.luau/x.luau",
            CodeDocumentError::NotFound(path("code/main.luau/x.luau")),
        ),
        (
            "code/dir.luau",
            CodeDocumentError::NotAFile(path("code/dir.luau")),
        ),
        (
            "code/bad.luau",
            CodeDocumentError::InvalidUtf8 {
                path: path("code/bad.luau"),
                line: 3,
            },
        ),
    ];
    for (file, expected) in cases {
        assert_eq!(docs.open(&cart, &path(file)), Err(expected), "{file}");
    }
    assert!(docs.documents.is_empty());
}

#[cfg(unix)]
#[test]
fn links_leaving_the_cartridge_are_not_found() {
    use std::os::unix::fs::symlink;

    let fixture = Fixture::new("code-doc-links");
    let other = fixture.root().join("games/other");
    write_cartridge(&other, "Other");
    fs::write(other.join("code/secret.luau"), "other's code").unwrap();
    fs::write(fixture.outside().join("secret.luau"), "host file").unwrap();
    let (cart, dir) = cartridge(&fixture, &[]);
    symlink(
        other.join("code/secret.luau"),
        dir.join("code/sibling.luau"),
    )
    .unwrap();
    symlink(
        fixture.outside().join("secret.luau"),
        dir.join("code/host.luau"),
    )
    .unwrap();
    symlink("main.luau", dir.join("code/alias.luau")).unwrap();
    let mut docs = CodeDocuments::default();

    for file in ["code/sibling.luau", "code/host.luau"] {
        assert_eq!(
            docs.open(&cart, &path(file)),
            Err(CodeDocumentError::NotFound(path(file))),
            "{file}"
        );
    }
    let alias = docs.open(&cart, &path("code/alias.luau")).unwrap();
    assert_eq!(
        docs.get(alias).unwrap().text(),
        "",
        "links inside are followed"
    );
}

fn doc(text: &str) -> CodeDocument {
    CodeDocument::from_text("code/main.luau", text)
}

/// Each line's content, as the line index splits it.
fn lines(doc: &CodeDocument) -> Vec<&str> {
    (0..doc.line_count())
        .map(|n| &doc.text()[doc.line_range(n).unwrap()])
        .collect()
}

#[test]
fn replace_inserts_deletes_and_replaces_multibyte_text() {
    let mut doc = doc("añb");
    let inserted = doc.replace(3..3, "漢😀").unwrap();
    assert_eq!(doc.text(), "añ漢😀b");
    assert_eq!(
        inserted,
        Replaced {
            range: 3..10,
            removed: String::new(),
        }
    );

    let deleted = doc.replace(1..3, "").unwrap();
    assert_eq!(doc.text(), "a漢😀b");
    assert_eq!(
        deleted,
        Replaced {
            range: 1..1,
            removed: "ñ".to_owned(),
        }
    );

    let replaced = doc.replace(1..4, "é").unwrap();
    assert_eq!(doc.text(), "aé😀b");
    assert_eq!(replaced.range, 1..3);
    assert_eq!(replaced.removed, "漢");
    assert_eq!(doc.origin().bytes(), "añb".as_bytes());
}

#[test]
fn replace_keeps_the_line_index_in_step() {
    let mut doc = doc("one\ntwo\nthree");
    doc.replace(5..5, "x\ny\n").unwrap();
    assert_eq!(lines(&doc), ["one", "tx", "y", "wo", "three"]);
    assert_eq!(doc.line_of(doc.text().find("three").unwrap()), 4);

    // Deleting a line ending joins the lines either side of it.
    let end = doc.line_range(0).unwrap().end;
    doc.replace(end..end + 1, "").unwrap();
    assert_eq!(lines(&doc), ["onetx", "y", "wo", "three"]);

    let mut crlf = CodeDocument::from_text("code/main.luau", "a\r\nb\r\nc");
    crlf.replace(1..1, "\r\nz").unwrap();
    assert_eq!(lines(&crlf), ["a", "z", "b", "c"]);
    // Deleting a whole CRLF joins the lines without leaving a \r behind.
    let end = crlf.line_range(1).unwrap().end;
    crlf.replace(end..end + 2, "").unwrap();
    assert_eq!(lines(&crlf), ["a", "zb", "c"]);
    assert_eq!(crlf.text(), "a\r\nzb\r\nc");
}

#[test]
fn empty_documents_and_lone_byte_order_marks_accept_text() {
    let mut empty = doc("");
    empty.replace(0..0, "x").unwrap();
    assert_eq!((empty.text(), empty.line_count()), ("x", 1));

    let mut bom = doc("\u{feff}");
    bom.replace(3..3, "x\n").unwrap();
    assert_eq!(bom.text(), "\u{feff}x\n");
    assert_eq!(lines(&bom), ["x", ""]);
}

#[test]
fn refused_edits_change_nothing() {
    let text = "\u{feff}é\r\nb";
    let cases = [
        (0..0, "x", EditError::InsideBom(0)),
        (1..3, "", EditError::NotCharBoundary(1)),
        (4..4, "x", EditError::NotCharBoundary(4)),
        (6..6, "x", EditError::SplitsLineEnding(6)),
        (5..6, "", EditError::SplitsLineEnding(6)),
        (
            3..9,
            "",
            EditError::OutOfBounds {
                start: 3,
                end: 9,
                len: 8,
            },
        ),
        // A range that ends before it starts.
        (
            Range { start: 7, end: 3 },
            "",
            EditError::OutOfBounds {
                start: 7,
                end: 3,
                len: 8,
            },
        ),
    ];
    for (range, insert, error) in cases {
        let mut doc = doc(text);
        let before = doc.clone();
        assert_eq!(doc.replace(range.clone(), insert), Err(error), "{range:?}");
        assert_eq!(doc, before, "{range:?}");
        assert_eq!(doc.revision(), 0);
    }
}

#[test]
fn edits_cannot_give_a_text_a_byte_order_mark() {
    let mut doc = doc("a\u{feff}b");
    assert_eq!(doc.replace(0..0, "\u{feff}"), Err(EditError::CreatesBom));
    assert_eq!(doc.replace(0..1, ""), Err(EditError::CreatesBom));
    assert_eq!(doc.text(), "a\u{feff}b");
    // Away from the start, U+FEFF is an ordinary character.
    doc.replace(1..1, "\u{feff}").unwrap();
    assert_eq!(doc.text(), "a\u{feff}\u{feff}b");
}

#[test]
fn the_line_ending_is_the_files_even_after_edits() {
    assert_eq!(doc("a\nb").line_ending(), LineEnding::Lf);
    assert_eq!(doc("a").line_ending(), LineEnding::Lf);
    assert_eq!(doc("\u{feff}a\r\nb\nc").line_ending(), LineEnding::CrLf);
    assert_eq!(doc("a\rb\nc").line_ending(), LineEnding::Lf);

    let mut crlf = doc("a\r\nb");
    crlf.replace(1..3, "").unwrap();
    assert_eq!(crlf.line_ending(), LineEnding::CrLf);
}

#[test]
fn the_indent_style_is_the_files_and_stays_fixed() {
    assert_eq!(doc("if x then\n\ty()\nend").indent(), Indent::Tabs);
    assert_eq!(doc("if x then\n    y()\nend").indent(), Indent::Spaces);
    assert_eq!(doc("a\n\tb\n  c").indent(), Indent::Spaces, "a tie");
    assert_eq!(doc("a\nb").indent(), Indent::Spaces, "no indentation");
    assert_eq!(doc("\u{feff}\tx\r\n\ty").indent(), Indent::Tabs);

    let mut spaces = doc("a\n  b");
    spaces.replace(0..0, "\tx\n\ty\n\tz\n").unwrap();
    assert_eq!(spaces.indent(), Indent::Spaces);
}

#[test]
fn the_indent_width_is_the_most_common_step_of_the_files_spaces() {
    assert_eq!(doc("if x then\n  y()\nend").indent_width(), Some(2));
    assert_eq!(
        doc("a\n    b\n        c\n    d\n  e").indent_width(),
        Some(4),
        "steps back out are not steps"
    );
    assert_eq!(
        doc("a\n  b\n\n    c\n").indent_width(),
        Some(2),
        "a blank line leaves the comparison to the next line"
    );
    assert_eq!(doc("a\n  b\nc\n    d").indent_width(), Some(2), "a tie");
    assert_eq!(doc("a\r\n   b\r\n").indent_width(), Some(3));
    assert_eq!(doc("a\n\tb\n\t  c").indent_width(), None, "tab indentation");
    assert_eq!(doc("a\nb").indent_width(), None);
    assert_eq!(
        doc("a\n            b").indent_width(),
        None,
        "wider than any step"
    );
}

#[test]
fn the_revision_counts_edits_while_modified_compares_with_the_file() {
    let mut doc = doc("abc");
    assert_eq!((doc.revision(), doc.is_modified()), (0, false));

    let replaced = doc.replace(1..2, "XY").unwrap();
    assert_eq!((doc.revision(), doc.is_modified()), (1, true));

    // Undoing by hand restores the file's text but not the revision.
    doc.replace(replaced.range, &replaced.removed).unwrap();
    assert_eq!(doc.text(), "abc");
    assert_eq!((doc.revision(), doc.is_modified()), (2, false));
}

mod saving;
