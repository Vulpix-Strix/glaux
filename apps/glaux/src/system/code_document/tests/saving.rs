//! Writing documents back to their files.

use std::path::Path;

use super::*;
use pretty_assertions::assert_eq;

/// Opens `file` of `cart` in `docs` and replaces its whole text with
/// `text`, keeping any byte order mark.
fn open_edited(
    docs: &mut CodeDocuments,
    cart: &LoadedCartridge,
    file: &str,
    text: &str,
) -> DocumentId {
    let id = docs.open(cart, &path(file)).unwrap();
    let doc = docs.get_mut(id).unwrap();
    let start = if doc.text().starts_with(BOM) {
        BOM.len_utf8()
    } else {
        0
    };
    let end = doc.text().len();
    doc.replace(start..end, text).unwrap();
    id
}

fn on_disk(dir: &Path, file: &str) -> Vec<u8> {
    fs::read(dir.join(file)).unwrap()
}

fn written(file: &str, bytes: usize) -> Result<CodeSaved, CodeSaveError> {
    Ok(CodeSaved::Written {
        path: path(file),
        bytes,
    })
}

fn conflict(file: &str, disk: Option<&[u8]>) -> Result<CodeSaved, CodeSaveError> {
    Err(CodeSaveError::Conflict(SourceConflict {
        path: path(file),
        kind: if disk.is_some() {
            SourceConflictKind::Modified
        } else {
            SourceConflictKind::Deleted
        },
        disk: disk.map(<[u8]>::to_vec),
    }))
}

#[test]
fn saving_writes_luau_and_rhai_text_and_makes_it_the_baseline() {
    let fixture = Fixture::new("code-save-langs");
    let (cart, dir) = cartridge(
        &fixture,
        &[
            ("code/hud.rhai", b"let a = 1;\n"),
            ("code/p.luau", b"x = 1\n"),
        ],
    );
    let mut docs = CodeDocuments::default();
    for (file, text) in [
        ("code/hud.rhai", "let a = 2;\n"),
        ("code/p.luau", "y = 2\n"),
    ] {
        let id = open_edited(&mut docs, &cart, file, text);

        assert_eq!(docs.save(&cart, id), written(file, text.len()), "{file}");
        assert_eq!(on_disk(&dir, file), text.as_bytes(), "{file}");
        let doc = docs.get(id).unwrap();
        assert!(!doc.is_modified(), "{file}");
        assert_eq!(doc.origin().bytes(), text.as_bytes(), "{file}");
    }
}

#[test]
fn saving_keeps_the_bom_line_endings_and_missing_final_newline() {
    let fixture = Fixture::new("code-save-bytes");
    let crlf = "\u{feff}a = 1\r\nb = 2".as_bytes();
    let lf = b"a = 1\nb = 2\n";
    let (cart, dir) = cartridge(&fixture, &[("code/crlf.luau", crlf), ("code/lf.luau", lf)]);
    let mut docs = CodeDocuments::default();

    // Typing a character after "a" on the first line of each file.
    for (file, original) in [("code/crlf.luau", crlf), ("code/lf.luau", &lf[..])] {
        let id = docs.open(&cart, &path(file)).unwrap();
        let at = docs.get(id).unwrap().line_range(0).unwrap().start + 1;
        docs.get_mut(id).unwrap().replace(at..at, "a").unwrap();
        docs.save(&cart, id).unwrap();

        let mut expected = original.to_vec();
        let at = if original.starts_with("\u{feff}".as_bytes()) {
            4
        } else {
            1
        };
        expected.insert(at, b'a');
        assert_eq!(on_disk(&dir, file), expected, "{file}");
    }
}

#[test]
fn an_unchanged_document_is_not_written() {
    let fixture = Fixture::new("code-save-unchanged");
    let (cart, dir) = cartridge(&fixture, &[("code/a.luau", b"a")]);
    let mut docs = CodeDocuments::default();
    let id = docs.open(&cart, &path("code/a.luau")).unwrap();
    // Even a file changed on disk is left alone: there is nothing to save.
    fs::write(dir.join("code/a.luau"), "outside").unwrap();

    assert_eq!(
        docs.save(&cart, id),
        Ok(CodeSaved::NothingToSave(path("code/a.luau")))
    );
    assert_eq!(on_disk(&dir, "code/a.luau"), b"outside");

    // Text edited back to the baseline is unchanged too.
    let doc = docs.get_mut(id).unwrap();
    doc.replace(1..1, "b").unwrap();
    doc.replace(1..2, "").unwrap();
    assert!(!doc.is_modified());
    assert_eq!(
        docs.save(&cart, id),
        Ok(CodeSaved::NothingToSave(path("code/a.luau")))
    );
}

#[test]
fn dirty_state_follows_the_last_saved_text_not_the_edit_count() {
    let fixture = Fixture::new("code-save-dirty");
    let (cart, _) = cartridge(&fixture, &[("code/a.luau", b"one")]);
    let mut docs = CodeDocuments::default();
    let id = open_edited(&mut docs, &cart, "code/a.luau", "two");
    assert!(docs.get(id).unwrap().is_modified());

    docs.save(&cart, id).unwrap();
    assert!(!docs.get(id).unwrap().is_modified());

    let doc = docs.get_mut(id).unwrap();
    doc.replace(0..3, "one").unwrap();
    assert!(doc.is_modified(), "the file now holds two");
    doc.replace(0..3, "two").unwrap();
    assert!(!doc.is_modified());
}

#[test]
fn documents_keep_their_own_dirty_state() {
    let fixture = Fixture::new("code-save-independent");
    let (cart, dir) = cartridge(&fixture, &[("code/a.luau", b"a"), ("code/b.luau", b"b")]);
    let mut docs = CodeDocuments::default();
    let a = open_edited(&mut docs, &cart, "code/a.luau", "a2");
    let b = open_edited(&mut docs, &cart, "code/b.luau", "b2");

    docs.save(&cart, a).unwrap();
    assert!(!docs.get(a).unwrap().is_modified());
    assert!(docs.get(b).unwrap().is_modified());
    assert!(docs.is_modified());
    assert_eq!(on_disk(&dir, "code/b.luau"), b"b");
}

#[test]
fn a_file_changed_outside_is_a_conflict_until_overwritten_as_seen() {
    let fixture = Fixture::new("code-save-changed");
    let (cart, dir) = cartridge(&fixture, &[("code/a.luau", b"a")]);
    let mut docs = CodeDocuments::default();
    let id = open_edited(&mut docs, &cart, "code/a.luau", "mine");
    fs::write(dir.join("code/a.luau"), "theirs").unwrap();

    assert_eq!(
        docs.save(&cart, id),
        conflict("code/a.luau", Some(b"theirs"))
    );
    assert_eq!(on_disk(&dir, "code/a.luau"), b"theirs");
    let doc = docs.get(id).unwrap();
    assert!(doc.is_modified());
    assert_eq!(doc.origin().bytes(), b"a", "the baseline is kept");

    // A change after the conflict was shown is another conflict.
    fs::write(dir.join("code/a.luau"), "theirs, again").unwrap();
    assert_eq!(
        docs.overwrite(&cart, id, Some(b"theirs")),
        conflict("code/a.luau", Some(b"theirs, again"))
    );
    assert_eq!(on_disk(&dir, "code/a.luau"), b"theirs, again");

    assert_eq!(
        docs.overwrite(&cart, id, Some(b"theirs, again")),
        written("code/a.luau", 4)
    );
    assert_eq!(on_disk(&dir, "code/a.luau"), b"mine");
    assert!(!docs.get(id).unwrap().is_modified());
}

#[test]
fn an_overwrite_never_falls_back_to_the_baseline() {
    let fixture = Fixture::new("code-save-seen");
    let (cart, dir) = cartridge(&fixture, &[("code/a.luau", b"a")]);
    let mut docs = CodeDocuments::default();
    let id = open_edited(&mut docs, &cart, "code/a.luau", "mine");

    // The file went back to the baseline after the user saw it changed.
    assert_eq!(
        docs.overwrite(&cart, id, Some(b"theirs")),
        conflict("code/a.luau", Some(b"a"))
    );
    assert_eq!(on_disk(&dir, "code/a.luau"), b"a");
}

#[test]
fn a_deleted_file_is_a_conflict_and_an_overwrite_writes_it_again() {
    let fixture = Fixture::new("code-save-deleted");
    let (cart, dir) = cartridge(&fixture, &[("code/a.luau", b"a")]);
    let mut docs = CodeDocuments::default();
    let id = open_edited(&mut docs, &cart, "code/a.luau", "mine");
    fs::remove_file(dir.join("code/a.luau")).unwrap();

    assert_eq!(docs.save(&cart, id), conflict("code/a.luau", None));
    assert!(!dir.join("code/a.luau").exists());

    assert_eq!(docs.overwrite(&cart, id, None), written("code/a.luau", 4));
    assert_eq!(on_disk(&dir, "code/a.luau"), b"mine");
}

#[test]
fn a_file_that_reappears_after_deletion_was_seen_is_a_conflict() {
    let fixture = Fixture::new("code-save-reappeared");
    let (cart, dir) = cartridge(&fixture, &[("code/a.luau", b"a")]);
    let mut docs = CodeDocuments::default();
    let id = open_edited(&mut docs, &cart, "code/a.luau", "mine");
    fs::write(dir.join("code/a.luau"), "new").unwrap();

    assert_eq!(
        docs.overwrite(&cart, id, None),
        conflict("code/a.luau", Some(b"new"))
    );
    assert_eq!(on_disk(&dir, "code/a.luau"), b"new");
}

#[test]
fn failed_writes_keep_the_file_and_the_baseline() {
    let fixture = Fixture::new("code-save-failed");
    let (cart, dir) = cartridge(
        &fixture,
        &[("code/ro.luau", b"ro"), ("code/dir.luau", b"d")],
    );
    let mut docs = CodeDocuments::default();

    let ro = open_edited(&mut docs, &cart, "code/ro.luau", "mine");
    let file = dir.join("code/ro.luau");
    let writable = fs::metadata(&file).unwrap().permissions();
    let mut read_only = writable.clone();
    read_only.set_readonly(true);
    fs::set_permissions(&file, read_only).unwrap();
    assert_eq!(
        docs.save(&cart, ro),
        Err(CodeSaveError::Failed {
            path: path("code/ro.luau"),
            error: FsError::ReadOnly("/code/ro.luau".to_owned()),
        })
    );
    fs::set_permissions(&file, writable).unwrap();
    assert_eq!(on_disk(&dir, "code/ro.luau"), b"ro");
    assert_eq!(docs.get(ro).unwrap().origin().bytes(), b"ro");
    assert!(docs.get(ro).unwrap().is_modified());

    let replaced = open_edited(&mut docs, &cart, "code/dir.luau", "mine");
    fs::remove_file(dir.join("code/dir.luau")).unwrap();
    fs::create_dir(dir.join("code/dir.luau")).unwrap();
    assert_eq!(
        docs.save(&cart, replaced),
        Err(CodeSaveError::Failed {
            path: path("code/dir.luau"),
            error: FsError::NotAFile("/code/dir.luau".to_owned()),
        })
    );
    assert!(dir.join("code/dir.luau").is_dir());
    assert_eq!(docs.get(replaced).unwrap().origin().bytes(), b"d");
}

#[test]
fn a_missing_folder_fails_an_overwrite_without_creating_it() {
    let fixture = Fixture::new("code-save-no-dir");
    let (cart, dir) = cartridge(&fixture, &[("code/sub/a.luau", b"a")]);
    let mut docs = CodeDocuments::default();
    let id = open_edited(&mut docs, &cart, "code/sub/a.luau", "mine");
    fs::remove_dir_all(dir.join("code/sub")).unwrap();

    assert_eq!(docs.save(&cart, id), conflict("code/sub/a.luau", None));
    assert!(matches!(
        docs.overwrite(&cart, id, None),
        Err(CodeSaveError::Failed { .. })
    ));
    assert!(!dir.join("code/sub").exists());
    assert!(docs.get(id).unwrap().is_modified());
}

#[test]
fn closing_forgets_the_id() {
    let fixture = Fixture::new("code-save-closed");
    let (cart, _) = cartridge(&fixture, &[("code/a.luau", b"a")]);
    let mut docs = CodeDocuments::default();
    let id = open_edited(&mut docs, &cart, "code/a.luau", "mine");
    docs.close(id);
    assert_eq!(docs.save(&cart, id), Err(CodeSaveError::NotOpen));
}

#[cfg(unix)]
#[test]
fn a_file_swapped_for_a_link_out_of_the_cartridge_is_never_written_through() {
    use std::os::unix::fs::symlink;

    let fixture = Fixture::new("code-save-link");
    let (cart, dir) = cartridge(&fixture, &[("code/a.luau", b"a")]);
    let outside = fixture.outside().join("host.luau");
    fs::write(&outside, "host file").unwrap();
    let mut docs = CodeDocuments::default();
    let id = open_edited(&mut docs, &cart, "code/a.luau", "mine");
    fs::remove_file(dir.join("code/a.luau")).unwrap();
    symlink(&outside, dir.join("code/a.luau")).unwrap();

    // The link hides the file, so it reads as deleted.
    assert_eq!(docs.save(&cart, id), conflict("code/a.luau", None));
    assert!(matches!(
        docs.overwrite(&cart, id, None),
        Err(CodeSaveError::Failed { .. })
    ));
    assert_eq!(fs::read(&outside).unwrap(), b"host file");
    assert!(
        fs::symlink_metadata(dir.join("code/a.luau"))
            .unwrap()
            .file_type()
            .is_symlink()
    );
}

#[cfg(unix)]
#[test]
fn a_link_inside_the_cartridge_is_saved_at_its_target() {
    use std::os::unix::fs::symlink;

    let fixture = Fixture::new("code-save-inner-link");
    let (cart, dir) = cartridge(&fixture, &[("code/real.luau", b"a")]);
    symlink("real.luau", dir.join("code/alias.luau")).unwrap();
    let mut docs = CodeDocuments::default();
    let id = open_edited(&mut docs, &cart, "code/alias.luau", "mine");

    docs.save(&cart, id).unwrap();
    assert_eq!(on_disk(&dir, "code/real.luau"), b"mine");
    assert!(
        fs::symlink_metadata(dir.join("code/alias.luau"))
            .unwrap()
            .file_type()
            .is_symlink()
    );
}
