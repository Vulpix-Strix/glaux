use super::*;
use crate::cartridge::fixture::write_cartridge;
use crate::cartridge_fs::fixture::Fixture;
use pretty_assertions::assert_eq;

#[test]
fn language_comes_from_the_extension() {
    for (path, language) in [
        ("code/main.luau", Language::Luau),
        ("code/ui/hud.rhai", Language::Rhai),
        ("main.luau", Language::Luau),
        ("code/v1.2.rhai", Language::Rhai),
    ] {
        let parsed = SourcePath::parse(path).unwrap();
        assert_eq!(parsed.language(), language, "{path}");
        assert_eq!(parsed.as_str(), path);
    }
}

#[test]
fn rejects_files_that_are_not_luau_or_rhai() {
    for path in [
        "code/main.lua",
        "code/notes.txt",
        "code/main",
        "code/main.LUAU",
        "code/.luau",
        "code/main.luau.bak",
        "code/mainluau",
    ] {
        assert_eq!(
            SourcePath::parse(path),
            Err(SourcePathError::UnsupportedExtension(path.into())),
            "{path}"
        );
    }
}

#[test]
fn rejects_paths_that_are_not_portable_and_relative() {
    for path in [
        "",
        "/code/main.luau",
        "code/../main.luau",
        "../main.luau",
        "./main.luau",
        "code//main.luau",
        "code/",
        "code\\main.luau",
        "c:main.luau",
        "code/ma\0in.luau",
    ] {
        assert_eq!(
            SourcePath::parse(path),
            Err(SourcePathError::Invalid(path.into())),
            "{path:?}"
        );
    }
}

/// A Luau cartridge at `/games/pong` with empty files at `paths`.
fn cartridge_with(fixture: &Fixture, paths: &[&str]) -> LoadedCartridge {
    let dir = fixture.root().join("games/pong");
    write_cartridge(&dir, "Pong");
    for path in paths {
        let host = dir.join(path);
        std::fs::create_dir_all(host.parent().unwrap()).unwrap();
        std::fs::write(host, "").unwrap();
    }
    LoadedCartridge::load(&fixture.storage(), "games/pong").unwrap()
}

fn listed(listing: &ScriptListing) -> Vec<&str> {
    listing.paths.iter().map(SourcePath::as_str).collect()
}

#[test]
fn lists_every_script_in_any_directory_in_a_fixed_order() {
    let fixture = Fixture::new("source-list");
    let cart = cartridge_with(
        &fixture,
        &[
            "code/ui/hud.rhai",
            "code/Enemies/bat.luau",
            "code/enemies/Ant.luau",
            "lib.luau",
            "code/notes.txt",
            "code/LOUD.LUAU",
            "sprites.bin",
        ],
    );
    let listing = cart.script_files().unwrap();
    assert_eq!(
        listed(&listing),
        [
            "code/enemies/Ant.luau",
            "code/Enemies/bat.luau",
            "code/main.luau",
            "code/ui/hud.rhai",
            "lib.luau",
        ]
    );
    assert_eq!(listing.gaps, []);
}

#[test]
fn a_truncated_listing_says_so() {
    let fixture = Fixture::new("source-list-limit");
    let cart = cartridge_with(&fixture, &["code/a.luau", "code/b.rhai"]);
    let limits = WalkLimits {
        max_depth: 16,
        max_files: 2,
    };
    let listing = cart.script_files_within(limits).unwrap();
    assert_eq!(listing.paths.len(), 2);
    assert_eq!(listing.gaps, [WalkGap::FileLimit(2)]);
}

#[test]
fn file_names_are_the_last_component() {
    assert_eq!(
        SourcePath::parse("code/ui/hud.rhai").unwrap().file_name(),
        "hud.rhai"
    );
    assert_eq!(
        SourcePath::parse("main.luau").unwrap().file_name(),
        "main.luau"
    );
}

#[test]
fn creating_a_script_makes_an_empty_file_and_its_folders() {
    let fixture = Fixture::new("source-create");
    let cartridge = cartridge_with(&fixture, &[]);
    let path = SourcePath::parse("code/enemies/owl.luau").unwrap();
    assert_eq!(cartridge.create_source(&path), Ok(()));
    assert_eq!(cartridge.read_source(&path), Ok(Vec::new()));
    assert!(listed(&cartridge.script_files().unwrap()).contains(&"code/enemies/owl.luau"));
}

#[test]
fn creating_an_existing_script_leaves_it_alone() {
    let fixture = Fixture::new("source-create-exists");
    let cartridge = cartridge_with(&fixture, &[]);
    let path = SourcePath::parse("code/main.luau").unwrap();
    let before = cartridge.read_source(&path).unwrap();
    assert_eq!(
        cartridge.create_source(&path),
        Err(FsError::AlreadyExists("/code/main.luau".into()))
    );
    assert_eq!(cartridge.read_source(&path).unwrap(), before);
}

mod editing {
    use super::*;
    use crate::cartridge::fixture::{manifest_ron, write_manifest};
    use pretty_assertions::assert_eq;

    fn path(path: &str) -> SourcePath {
        SourcePath::parse(path).unwrap()
    }

    /// A cartridge whose manifest names `entry`, with files at `paths`
    /// holding their own names.
    fn with_entry(fixture: &Fixture, entry: &str, paths: &[&str]) -> LoadedCartridge {
        let dir = fixture.root().join("games/pong");
        write_manifest(&dir, &manifest_ron("Pong", entry));
        for file in paths.iter().chain([&entry]) {
            let host = dir.join(file);
            std::fs::create_dir_all(host.parent().unwrap()).unwrap();
            std::fs::write(host, file).unwrap();
        }
        LoadedCartridge::load(&fixture.storage(), "games/pong").unwrap()
    }

    fn exists(fixture: &Fixture, file: &str) -> bool {
        fixture.root().join("games/pong").join(file).exists()
    }

    #[test]
    fn the_entry_the_manifest_names_is_neither_deleted_nor_moved() {
        let fixture = Fixture::new("source-entry");
        // Not `code/main.luau`: the protected file comes from the manifest.
        let cart = with_entry(&fixture, "code/start.luau", &["code/main.luau"]);
        let entry = path("code/start.luau");

        assert_eq!(
            cart.delete_source(&entry, Some(b"code/start.luau")),
            Err(SourceEditError::Entry(entry.clone()))
        );
        assert_eq!(
            cart.rename_source(&entry, &path("code/other.luau")),
            Err(SourceEditError::Entry(entry.clone()))
        );
        assert_eq!(
            cart.rename_source(&path("code/main.luau"), &entry),
            Err(SourceEditError::Fs(FsError::AlreadyExists(
                "/code/start.luau".into()
            )))
        );
        assert!(exists(&fixture, "code/start.luau"));
        assert!(exists(&fixture, "code/main.luau"), "main is not the entry");
        assert_eq!(
            cart.delete_source(&path("code/main.luau"), Some(b"code/main.luau")),
            Ok(())
        );
    }

    #[test]
    fn deleting_needs_the_file_to_hold_what_the_caller_saw() {
        let fixture = Fixture::new("source-delete");
        let cart = with_entry(&fixture, "code/main.luau", &["code/a.luau", "code/b.luau"]);
        let a = path("code/a.luau");

        assert_eq!(
            cart.delete_source(&a, Some(b"something else")),
            Err(SourceEditError::Changed(a.clone()))
        );
        assert_eq!(
            cart.delete_source(&a, None),
            Err(SourceEditError::Changed(a.clone())),
            "the caller saw it absent, but it exists"
        );
        assert!(exists(&fixture, "code/a.luau"));

        assert_eq!(cart.delete_source(&a, Some(b"code/a.luau")), Ok(()));
        assert!(!exists(&fixture, "code/a.luau"));
        assert!(exists(&fixture, "code/b.luau"));
        // Deleting what is already gone is as good as done.
        assert_eq!(cart.delete_source(&a, Some(b"code/a.luau")), Ok(()));
        assert_eq!(cart.delete_source(&a, None), Ok(()));
    }

    #[test]
    fn a_script_keeps_its_language_when_moved() {
        let fixture = Fixture::new("source-language");
        let cart = with_entry(&fixture, "code/main.luau", &["code/a.luau"]);
        let from = path("code/a.luau");
        let to = path("code/a.rhai");
        assert_eq!(
            cart.rename_source(&from, &to),
            Err(SourceEditError::Language {
                from: from.clone(),
                to
            })
        );
        assert!(exists(&fixture, "code/a.luau"));

        assert_eq!(cart.rename_source(&from, &path("code/sub/b.luau")), Ok(()));
        assert!(exists(&fixture, "code/sub/b.luau"));
        assert!(!exists(&fixture, "code/a.luau"));
    }
}
