use std::fs;

use super::*;
use crate::cartridge::CartridgeManifest;
use crate::cartridge::fixture::write_cartridge;
use crate::cartridge_fs::fixture::Fixture;
use pretty_assertions::assert_eq;

fn spec(id: &str) -> NewCartridge {
    NewCartridge::new(id, None, None, None).unwrap()
}

#[test]
fn defaults_derive_the_name_from_the_id() {
    let spec = spec("my-first-game");
    let manifest = CartridgeManifest::parse(spec.manifest_ron().as_bytes()).unwrap();
    assert_eq!(manifest.cartridge.name, "My First Game");
    assert_eq!(manifest.cartridge.version, Version::new(0, 1, 0));
    assert_eq!(manifest.cartridge.author, None);
    assert_eq!(manifest.runtime.language, Language::Luau);
    assert_eq!(manifest.runtime.entry, "code/main.luau");
    assert_eq!(manifest.hardware.model, HardwareModel::G1);
    assert_eq!(manifest.hardware.graphics, GraphicsProfile::G1Bpp4);
    assert_eq!(manifest.format, FORMAT);
    assert_eq!(&manifest.compatibility.min_glaux, glaux_version());
    assert!(!spec.manifest_ron().contains("author"));
    assert_eq!(display_name("a1-b-c2"), "A1 B C2");
}

#[test]
fn explicit_metadata_is_kept_as_given() {
    let spec = NewCartridge::new(
        "space-adventure",
        Some(" Space Adventure "),
        Some("1.2.0-beta.1"),
        Some("Cuervolu \"El\" Ñandú"),
    )
    .unwrap();
    let manifest = CartridgeManifest::parse(spec.manifest_ron().as_bytes()).unwrap();
    assert_eq!(manifest.cartridge.name, "Space Adventure");
    assert_eq!(
        manifest.cartridge.version,
        Version::parse("1.2.0-beta.1").unwrap()
    );
    assert_eq!(
        manifest.cartridge.author.as_deref(),
        Some("Cuervolu \"El\" Ñandú")
    );
}

#[test]
fn ids_must_be_portable_kebab_case() {
    let too_long = "a".repeat(MAX_ID_LEN + 1);
    for id in [
        "", "My-Game", "my game", "-a", "a-", "a--b", "1abc", "a_b", "a/b", "..", ".a", "é", "con",
        "lpt1", &too_long,
    ] {
        assert_eq!(
            NewCartridge::new(id, None, None, None),
            Err(CreateError::InvalidId(id.to_owned())),
            "{id:?}"
        );
    }
    for id in ["a", "pong", "my-game-2", "x1-y2", &"a".repeat(MAX_ID_LEN)] {
        assert!(NewCartridge::new(id, None, None, None).is_ok(), "{id:?}");
    }
}

#[test]
fn ids_are_suggested_from_free_text() {
    for (text, id) in [
        ("My New Game", Some("my-new-game")),
        ("my-game", Some("my-game")),
        ("  Space   Adventure 2! ", Some("space-adventure-2")),
        ("Ñandú Quest", None),
        ("2048", None),
        ("!!", None),
        ("con", None),
    ] {
        assert_eq!(suggest_id(text).as_deref(), id, "{text:?}");
    }
}

#[test]
fn versions_must_be_semver() {
    for version in ["1", "1.0", "v1.0.0", "", "1.0.0.0"] {
        assert_eq!(
            NewCartridge::new("a", None, Some(version), None),
            Err(CreateError::InvalidVersion(version.to_owned())),
            "{version:?}"
        );
    }
    assert_eq!(
        CreateError::InvalidVersion("1.0".into()).to_string(),
        "Invalid version '1.0': use semver, like 1.2.0"
    );
}

#[test]
fn names_and_authors_are_checked() {
    let long = "x".repeat(MAX_TEXT_LEN + 1);
    let new = |name, author| NewCartridge::new("a", name, None, author).map(drop);
    assert_eq!(new(Some("  "), None), Err(CreateError::EmptyName));
    assert_eq!(new(Some("a\nb"), None), Err(CreateError::InvalidName));
    assert_eq!(new(Some(&long), None), Err(CreateError::NameTooLong));
    for author in ["", " ", "a\tb", &long] {
        assert_eq!(
            new(None, Some(author)),
            Err(CreateError::InvalidAuthor),
            "{author:?}"
        );
    }
}

#[test]
fn created_cartridges_load_with_the_starter() {
    let fixture = Fixture::new("template-create");
    let mut storage = fixture.storage();
    storage.change_dir("games").unwrap();
    let cartridge = create(&storage, &spec("my-game")).unwrap();
    assert_eq!(cartridge.manifest().cartridge.name, "My Game");
    assert_eq!(cartridge.entry_source(), Ok(STARTER.as_bytes().to_vec()));
    let dir = fixture.root().join("games/my-game");
    assert_eq!(
        fs::read_to_string(dir.join("cartridge.ron")).unwrap(),
        spec("my-game").manifest_ron()
    );
    let mut names: Vec<_> = fs::read_dir(&dir)
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect();
    names.sort();
    assert_eq!(names, ["cartridge.ron", "code"]);
}

#[test]
fn existing_entries_are_never_replaced() {
    let fixture = Fixture::new("template-exists");
    let dir = fixture.root().join("pong");
    write_cartridge(&dir, "Pong");
    let before = fs::read(dir.join("cartridge.ron")).unwrap();
    let storage = fixture.storage();
    assert_eq!(
        create(&storage, &spec("pong")).map(drop),
        Err(CreateError::Exists("/pong".into()))
    );
    assert_eq!(fs::read(dir.join("cartridge.ron")).unwrap(), before);
    assert_eq!(fs::read(dir.join("code/main.luau")).unwrap(), b"");

    fs::write(fixture.root().join("note"), "kept").unwrap();
    assert_eq!(
        create(&storage, &spec("note")).map(drop),
        Err(CreateError::Exists("/note".into()))
    );
    assert_eq!(fs::read(fixture.root().join("note")).unwrap(), b"kept");
}

#[test]
fn unavailable_storage_creates_nothing() {
    assert_eq!(
        create(&CartridgeStorage::Unavailable, &spec("a")).map(drop),
        Err(CreateError::Fs(FsError::Unavailable))
    );
}
