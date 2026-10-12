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
