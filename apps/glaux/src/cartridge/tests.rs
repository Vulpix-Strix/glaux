use std::fs;
use std::path::Path;

use super::fixture::{manifest_ron, write_cartridge, write_manifest};
use super::*;
use crate::cartridge_fs::fixture::Fixture;
use pretty_assertions::assert_eq;

const MOCHI: &str = r#"(
    format: 1,

    cartridge: (
        name: "Mochi Adventure",
        version: "0.1.0",
        author: Some("cuervolu"),
    ),

    runtime: (
        language: Luau,
        entry: "code/main.luau",
    ),

    hardware: (
        model: G1,
        graphics: G1_4bpp,
    ),

    compatibility: (
        min_glaux: "0.1.0",
    ),
)"#;

fn manifest(ron: &str) -> Result<CartridgeManifest, CartridgeError> {
    CartridgeManifest::parse(ron.as_bytes())
}

/// `MOCHI` with the first occurrence of `from` replaced by `to`.
fn mochi_with(from: &str, to: &str) -> Result<CartridgeManifest, CartridgeError> {
    assert!(MOCHI.contains(from), "{from:?} not in the template");
    manifest(&MOCHI.replacen(from, to, 1))
}

fn with_entry(entry: &str) -> Result<CartridgeManifest, CartridgeError> {
    manifest(&manifest_ron("Pong", entry))
}

#[test]
fn parses_the_specified_v1_manifest() {
    assert_eq!(
        manifest(MOCHI),
        Ok(CartridgeManifest {
            format: 1,
            cartridge: CartridgeInfo {
                name: "Mochi Adventure".into(),
                version: Version::new(0, 1, 0),
                author: Some("cuervolu".into()),
            },
            runtime: RuntimeSpec {
                language: Language::Luau,
                entry: "code/main.luau".into(),
            },
            hardware: HardwareSpec {
                model: HardwareModel::G1,
                graphics: GraphicsProfile::G1Bpp4,
            },
            compatibility: Compatibility {
                min_glaux: Version::new(0, 1, 0),
            },
        })
    );
}

#[test]
fn author_is_optional() {
    let author = |m: CartridgeManifest| m.cartridge.author;
    assert_eq!(
        mochi_with(r#"author: Some("cuervolu"),"#, "author: None,").map(author),
        Ok(None)
    );
    assert_eq!(
        mochi_with(r#"author: Some("cuervolu"),"#, "").map(author),
        Ok(None)
    );
}

#[test]
fn comments_and_trailing_commas_are_plain_ron() {
    let commented = r#"// Mochi
            (
                format: 1, /* manifest format, not the game */
                cartridge: (name: "Mochi", version: "1.2.3-beta.1",),
                runtime: (language: Luau, entry: "code/main.luau",), // entry
                hardware: (model: G1, graphics: G1_4bpp,),
                compatibility: (min_glaux: "0.1.0",),
            )
        "#;
    let parsed = manifest(commented).unwrap();
    assert_eq!(
        parsed.cartridge.version,
        Version::parse("1.2.3-beta.1").unwrap()
    );
    assert_eq!(parsed.runtime.entry, "code/main.luau");
}

#[test]
fn malformed_ron_reports_the_line() {
    assert_eq!(
        manifest("(\n    format: 1,\n    cartridge: ("),
        Err(CartridgeError::Malformed {
            line: 3,
            reason: "Unexpected end of RON".into(),
        })
    );
    let json = r#"{"name": "Pong", "version": "0.1.0", "entry": "main.luau"}"#;
    assert!(matches!(
        manifest(json),
        Err(CartridgeError::Malformed { line: 1, .. })
    ));
    assert!(matches!(
        CartridgeManifest::parse(b"\xff"),
        Err(CartridgeError::Malformed { line: 1, .. })
    ));
    assert_eq!(
        mochi_with("model: G1,", "model: G1"),
        Err(CartridgeError::Malformed {
            line: 17,
            reason: "Expected comma".into(),
        })
    );
    let shown = CartridgeError::Malformed {
        line: 3,
        reason: "Expected closing `)`".into(),
    };
    assert_eq!(
        shown.to_string(),
        "cartridge.ron line 3: Expected closing `)`"
    );
}

#[test]
fn only_format_1_is_supported() {
    assert_eq!(
        mochi_with("format: 1,", "format: 2,"),
        Err(CartridgeError::UnsupportedFormat(2))
    );
    // Another format may have another shape: the format is still what
    // gets reported.
    assert_eq!(
        manifest("(format: 0, title: \"Pong\")"),
        Err(CartridgeError::UnsupportedFormat(0))
    );
    assert_eq!(
        mochi_with("format: 1,", ""),
        Err(CartridgeError::MissingField("format".into()))
    );
    assert_eq!(
        CartridgeError::UnsupportedFormat(2).to_string(),
        "cartridge.ron: unsupported format 2"
    );
}

#[test]
fn missing_and_unknown_fields_name_their_section() {
    assert_eq!(
        mochi_with(r#"version: "0.1.0","#, ""),
        Err(CartridgeError::MissingField("cartridge.version".into()))
    );
    assert_eq!(
        mochi_with(r#"entry: "code/main.luau","#, ""),
        Err(CartridgeError::MissingField("runtime.entry".into()))
    );
    assert_eq!(
        mochi_with("graphics: G1_4bpp,", ""),
        Err(CartridgeError::MissingField("hardware.graphics".into()))
    );
    assert_eq!(
        mochi_with(
            r#"author: Some("cuervolu"),"#,
            r#"auther: Some("cuervolu"),"#
        ),
        Err(CartridgeError::UnknownField("cartridge.auther".into()))
    );
    assert_eq!(
        mochi_with("format: 1,", "format: 1, api: 2,"),
        Err(CartridgeError::UnknownField("api".into()))
    );
    assert_eq!(
        CartridgeError::MissingField("runtime.entry".into()).to_string(),
        "cartridge.ron: missing runtime.entry"
    );
}

#[test]
fn unsupported_enum_values_are_glaux_errors() {
    assert_eq!(
        mochi_with("language: Luau", "language: Rhai"),
        Err(CartridgeError::UnsupportedLanguage("Rhai".into()))
    );
    assert_eq!(
        mochi_with("model: G1", "model: G2"),
        Err(CartridgeError::UnsupportedModel("G2".into()))
    );
    assert_eq!(
        mochi_with("graphics: G1_4bpp", "graphics: G1_2bpp"),
        Err(CartridgeError::UnsupportedGraphics("G1_2bpp".into()))
    );
    assert_eq!(
        CartridgeError::UnsupportedLanguage("Rhai".into()).to_string(),
        "cartridge.ron: unsupported language Rhai"
    );
}

#[test]
fn name_must_not_be_blank() {
    for name in ["", "   ", "\t\n"] {
        assert_eq!(
            manifest(&manifest_ron(name, "main.luau")),
            Err(CartridgeError::EmptyName),
            "{name:?}"
        );
    }
    assert_eq!(
        manifest(&manifest_ron(" Pong ", "main.luau")).map(|m| m.cartridge.name),
        Ok(" Pong ".into())
    );
}

#[test]
fn versions_must_be_semver() {
    for version in ["", "1", "1.0", "v1.0.0", "1.0.0.0", "01.0.0"] {
        assert_eq!(
            mochi_with(r#"version: "0.1.0""#, &format!("version: {version:?}")),
            Err(CartridgeError::InvalidVersion),
            "{version:?}"
        );
        assert_eq!(
            mochi_with(r#"min_glaux: "0.1.0""#, &format!("min_glaux: {version:?}")),
            Err(CartridgeError::InvalidMinGlaux),
            "{version:?}"
        );
    }
}

#[test]
fn min_glaux_must_not_exceed_the_running_glaux() {
    let parsed = mochi_with(r#"min_glaux: "0.1.0""#, r#"min_glaux: "0.2.0""#).unwrap();
    assert_eq!(parsed.check_compatible(&Version::new(0, 2, 0)), Ok(()));
    assert_eq!(parsed.check_compatible(&Version::new(1, 0, 0)), Ok(()));
    assert_eq!(
        parsed.check_compatible(&Version::new(0, 1, 9)),
        Err(CartridgeError::RequiresGlaux {
            required: Version::new(0, 2, 0),
            running: Version::new(0, 1, 9),
        })
    );
    assert_eq!(
        parsed
            .check_compatible(&Version::new(0, 1, 9))
            .unwrap_err()
            .to_string(),
        "Cartridge requires Glaux 0.2.0 (this is 0.1.9)"
    );
}

#[test]
fn entry_must_be_a_portable_relative_path() {
    for entry in [
        "",
        "/main.luau",
        "//main.luau",
        "/code/main.luau",
        "../main.luau",
        "code/../../main.luau",
        "a/../main.luau",
        "src/..",
        "./main.luau",
        "a/./main.luau",
        "a//main.luau",
        "src/",
        r"src\main.luau",
        r"\main.luau",
        "C:main.luau",
        "C:/main.luau",
        "main.luau:stream",
        "a\0.luau",
    ] {
        assert_eq!(
            with_entry(&entry.escape_default().to_string()),
            Err(CartridgeError::InvalidEntryPath),
            "{entry:?}"
        );
    }
}

#[test]
fn entry_must_be_a_luau_file() {
    for entry in [
        "main.lua",
        "main.LUAU",
        "main",
        ".luau",
        "code/.luau",
        "main.luau.txt",
    ] {
        assert_eq!(
            with_entry(entry),
            Err(CartridgeError::EntryNotLuau),
            "{entry:?}"
        );
    }
    for entry in [
        "main.luau",
        "code/main.luau",
        "code/game/main.luau",
        "a.b.luau",
        "..luau",
    ] {
        assert_eq!(with_entry(entry).map(|m| m.runtime.entry), Ok(entry.into()));
    }
}

fn load(storage: &CartridgeStorage, path: &str) -> Result<CartridgeManifest, CartridgeError> {
    LoadedCartridge::load(storage, path).map(|loaded| loaded.manifest().clone())
}

#[test]
fn loads_relative_to_the_current_directory_or_root() {
    let fixture = Fixture::new("load-ok");
    write_cartridge(&fixture.root().join("games/pong"), "Pong");
    let mut storage = fixture.storage();

    let pong = load(&storage, "/games/pong").unwrap();
    assert_eq!(pong.cartridge.name, "Pong");
    assert_eq!(pong.cartridge.version, Version::new(0, 1, 0));
    assert_eq!(pong.runtime.entry, "code/main.luau");
    storage.change_dir("games/owl").unwrap();
    assert_eq!(load(&storage, "../pong"), Ok(pong.clone()));
    assert_eq!(load(&storage, "/games/./owl/../pong/"), Ok(pong));
    assert_eq!(storage.cwd(), Ok("/games/owl".into()), "load does not move");
}

#[test]
fn entry_may_live_at_the_root_or_nested_under_code() {
    let fixture = Fixture::new("load-entry-dirs");
    for entry in ["main.luau", "code/main.luau", "code/game/main.luau"] {
        let dir = fixture.root().join("pong");
        let _ = fs::remove_dir_all(&dir);
        write_manifest(&dir, &manifest_ron("Pong", entry));
        fs::create_dir_all(dir.join(entry).parent().unwrap()).unwrap();
        fs::write(dir.join(entry), "").unwrap();
        assert_eq!(
            load(&fixture.storage(), "pong").map(|m| m.runtime.entry),
            Ok(entry.into())
        );
    }
}

#[test]
fn a_cartridge_for_a_newer_glaux_does_not_load() {
    let fixture = Fixture::new("load-min-glaux");
    let dir = fixture.root().join("pong");
    write_cartridge(&dir, "Pong");
    let newer = manifest_ron("Pong", "code/main.luau")
        .replace(r#"min_glaux: "0.1.0""#, r#"min_glaux: "99.0.0""#);
    write_manifest(&dir, &newer);
    assert!(matches!(
        load(&fixture.storage(), "pong"),
        Err(CartridgeError::RequiresGlaux { required, .. }) if required == Version::new(99, 0, 0)
    ));
}

#[test]
fn entry_source_reads_the_current_entry() {
    let fixture = Fixture::new("load-entry-source");
    let dir = fixture.root().join("pong");
    write_cartridge(&dir, "Pong");
    let entry = dir.join("code/main.luau");
    fs::write(&entry, "print(1)").unwrap();
    let cartridge = LoadedCartridge::load(&fixture.storage(), "pong").unwrap();
    assert_eq!(cartridge.entry_source(), Ok(b"print(1)".to_vec()));

    fs::write(&entry, "print(2)").unwrap();
    assert_eq!(cartridge.entry_source(), Ok(b"print(2)".to_vec()));

    fs::remove_file(&entry).unwrap();
    assert_eq!(
        cartridge.entry_source(),
        Err(CartridgeError::EntryNotFound("code/main.luau".into()))
    );
}

/// Loads module `name` of `cartridge` as the Luau runtime would.
fn load_module(cartridge: &LoadedCartridge, name: &str) -> Result<Vec<u8>, ModuleLoadError> {
    let path = glaux_modules::ModuleName::parse(name).unwrap().path("luau");
    cartridge.modules().load(&path)
}

#[test]
fn modules_are_read_fresh_from_code() {
    let fixture = Fixture::new("modules-code");
    let dir = fixture.root().join("pong");
    write_cartridge(&dir, "Pong");
    fs::create_dir_all(dir.join("code/game")).unwrap();
    fs::write(dir.join("code/game/enemies.luau"), "return 1").unwrap();
    let cartridge = LoadedCartridge::load(&fixture.storage(), "pong").unwrap();

    assert_eq!(
        load_module(&cartridge, "game.enemies"),
        Ok(b"return 1".to_vec())
    );
    assert_eq!(load_module(&cartridge, "main"), Ok(Vec::new()));
    fs::write(dir.join("code/game/enemies.luau"), "return 2").unwrap();
    assert_eq!(
        load_module(&cartridge, "game.enemies"),
        Ok(b"return 2".to_vec())
    );
    assert_eq!(
        load_module(&cartridge, "nope"),
        Err(ModuleLoadError::NotFound)
    );
    assert_eq!(
        load_module(&cartridge, "game"),
        Err(ModuleLoadError::NotFound)
    );
    fs::create_dir(dir.join("code/dir.luau")).unwrap();
    assert_eq!(
        load_module(&cartridge, "dir"),
        Err(ModuleLoadError::Unreadable)
    );
}

#[test]
fn modules_cannot_name_files_outside_code() {
    let fixture = Fixture::new("modules-outside-code");
    let dir = fixture.root().join("pong");
    write_cartridge(&dir, "Pong");
    // Files a module path could only reach by leaving `code/`.
    fs::write(dir.join("secret.luau"), "").unwrap();
    fs::write(fixture.root().join("games/owl/main.luau"), "").unwrap();
    let cartridge = LoadedCartridge::load(&fixture.storage(), "pong").unwrap();
    assert_eq!(
        load_module(&cartridge, "secret"),
        Err(ModuleLoadError::NotFound)
    );
}

#[test]
fn a_cartridge_without_code_has_no_modules() {
    let fixture = Fixture::new("modules-no-code");
    let dir = fixture.root().join("pong");
    write_manifest(&dir, &manifest_ron("Pong", "main.luau"));
    fs::write(dir.join("main.luau"), "").unwrap();
    let cartridge = LoadedCartridge::load(&fixture.storage(), "pong").unwrap();
    assert_eq!(
        load_module(&cartridge, "main"),
        Err(ModuleLoadError::NotFound)
    );

    fs::write(dir.join("code"), "").unwrap();
    assert_eq!(
        load_module(&cartridge, "main"),
        Err(ModuleLoadError::NotFound)
    );
}

#[test]
fn one_cartridge_cannot_load_another_cartridges_modules() {
    let fixture = Fixture::new("modules-other-cartridge");
    let games = fixture.root().join("games");
    write_cartridge(&games.join("a"), "A");
    write_cartridge(&games.join("b"), "B");
    fs::write(games.join("b/code/only_b.luau"), "").unwrap();
    let a = LoadedCartridge::load(&fixture.storage(), "games/a").unwrap();
    let b = LoadedCartridge::load(&fixture.storage(), "games/b").unwrap();
    assert_eq!(load_module(&a, "only_b"), Err(ModuleLoadError::NotFound));
    assert_eq!(load_module(&b, "only_b"), Ok(Vec::new()));
}

#[test]
fn object_tiles_are_optional_and_read_fresh() {
    let fixture = Fixture::new("load-obj-tiles");
    let dir = fixture.root().join("pong");
    write_cartridge(&dir, "Pong");
    let cartridge = LoadedCartridge::load(&fixture.storage(), "pong").unwrap();
    assert_eq!(cartridge.read_obj_tiles(), Ok(None));

    fs::write(dir.join("sprites.bin"), [0x12; 32]).unwrap();
    assert_eq!(cartridge.read_obj_tiles(), Ok(Some(vec![0x12; 32])));
    fs::write(dir.join("sprites.bin"), [0x34; 64]).unwrap();
    assert_eq!(cartridge.read_obj_tiles(), Ok(Some(vec![0x34; 64])));
}

#[test]
fn palette_assets_are_optional_and_read_fresh() {
    let fixture = Fixture::new("load-palette-assets");
    let dir = fixture.root().join("pong");
    write_cartridge(&dir, "Pong");
    let cartridge = LoadedCartridge::load(&fixture.storage(), "pong").unwrap();
    type Reader = fn(&LoadedCartridge) -> Result<Option<Vec<u8>>, CartridgeError>;
    let readers: [(&str, Reader); 3] = [
        ("objattr.bin", LoadedCartridge::read_obj_attributes),
        ("obp.bin", LoadedCartridge::read_obj_palettes),
        ("palette.bin", LoadedCartridge::read_master_palette),
    ];
    for (name, read) in readers {
        assert_eq!(read(&cartridge), Ok(None), "{name}");
        fs::write(dir.join(name), [1, 2, 3]).unwrap();
        assert_eq!(read(&cartridge), Ok(Some(vec![1, 2, 3])), "{name}");
    }
}

#[test]
fn palette_assets_must_be_regular_files() {
    let fixture = Fixture::new("load-palette-assets-file");
    let dir = fixture.root().join("pong");
    write_cartridge(&dir, "Pong");
    for name in ["objattr.bin", "obp.bin", "palette.bin"] {
        fs::create_dir(dir.join(name)).unwrap();
    }
    let cartridge = LoadedCartridge::load(&fixture.storage(), "pong").unwrap();
    assert_eq!(
        cartridge.read_obj_attributes(),
        Err(CartridgeError::ObjAttributesNotAFile)
    );
    assert_eq!(
        cartridge.read_obj_palettes(),
        Err(CartridgeError::ObjPalettesNotAFile)
    );
    assert_eq!(
        cartridge.read_master_palette(),
        Err(CartridgeError::PaletteNotAFile)
    );
}

#[test]
fn object_tiles_must_be_a_regular_readable_file() {
    let fixture = Fixture::new("load-obj-tiles-file");
    let dir = fixture.root().join("pong");
    write_cartridge(&dir, "Pong");
    fs::create_dir(dir.join("sprites.bin")).unwrap();
    let cartridge = LoadedCartridge::load(&fixture.storage(), "pong").unwrap();

    assert_eq!(
        cartridge.read_obj_tiles(),
        Err(CartridgeError::ObjTilesNotAFile)
    );
    assert_eq!(
        CartridgeError::ObjTilesNotAFile.to_string(),
        "sprites.bin is not a file"
    );
}

#[test]
fn cartridge_directory_must_exist() {
    let fixture = Fixture::new("load-dir");
    let storage = fixture.storage();
    assert_eq!(
        load(&storage, "games/nope"),
        Err(CartridgeError::Fs(FsError::NotFound("/games/nope".into())))
    );
    assert_eq!(
        load(&storage, "b.txt"),
        Err(CartridgeError::Fs(FsError::NotADirectory("/b.txt".into())))
    );
    assert_eq!(
        load(&CartridgeStorage::Unavailable, "games"),
        Err(CartridgeError::Fs(FsError::Unavailable))
    );
    assert_eq!(
        CartridgeError::Fs(FsError::Unavailable).to_string(),
        "Cartridge storage unavailable"
    );
}

#[test]
fn manifest_must_be_a_file() {
    let fixture = Fixture::new("load-manifest");
    let storage = fixture.storage();
    assert_eq!(
        load(&storage, "games/owl"),
        Err(CartridgeError::MissingManifest("/games/owl".into()))
    );
    assert_eq!(
        CartridgeError::MissingManifest("/games/owl".into()).to_string(),
        "No cartridge.ron in /games/owl"
    );
    fs::create_dir(fixture.root().join("games/owl/cartridge.ron")).unwrap();
    assert_eq!(
        load(&storage, "games/owl"),
        Err(CartridgeError::ManifestNotAFile)
    );
}

#[test]
fn a_legacy_cart_json_alone_is_rejected() {
    let fixture = Fixture::new("load-legacy");
    let dir = fixture.root().join("pong");
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join("cart.json"),
        r#"{"name": "Pong", "version": "0.1.0", "entry": "main.luau"}"#,
    )
    .unwrap();
    fs::write(dir.join("main.luau"), "").unwrap();
    assert_eq!(
        load(&fixture.storage(), "pong"),
        Err(CartridgeError::LegacyManifest)
    );

    // A leftover cart.json is ignored once cartridge.ron exists.
    write_cartridge(&dir, "Pong");
    assert!(load(&fixture.storage(), "pong").is_ok());
}

#[test]
fn manifest_errors_surface() {
    let fixture = Fixture::new("load-bad-manifest");
    let dir = fixture.root().join("games/owl");
    write_manifest(&dir, "(");
    assert!(matches!(
        load(&fixture.storage(), "games/owl"),
        Err(CartridgeError::Malformed { .. })
    ));
    write_manifest(
        &dir,
        &manifest_ron("Owl", "code/main.luau").replace(r#""0.1.0", author"#, r#""1", author"#),
    );
    assert_eq!(
        load(&fixture.storage(), "games/owl"),
        Err(CartridgeError::InvalidVersion)
    );
    write_manifest(&dir, &manifest_ron("Owl", "/code/main.luau"));
    assert_eq!(
        load(&fixture.storage(), "games/owl"),
        Err(CartridgeError::InvalidEntryPath)
    );
    write_manifest(&dir, &manifest_ron("Owl", "../main.luau"));
    assert_eq!(
        load(&fixture.storage(), "games/owl"),
        Err(CartridgeError::InvalidEntryPath)
    );
}

#[test]
fn entry_must_exist_as_a_regular_file() {
    let fixture = Fixture::new("load-entry");
    let dir = fixture.root().join("games/owl");
    write_manifest(&dir, &manifest_ron("Owl", "code/main.luau"));
    let storage = fixture.storage();
    let missing = Err(CartridgeError::EntryNotFound("code/main.luau".into()));
    assert_eq!(load(&storage, "games/owl"), missing);

    // A file where a directory is expected.
    fs::write(dir.join("code"), "").unwrap();
    assert_eq!(load(&storage, "games/owl"), missing);

    fs::remove_file(dir.join("code")).unwrap();
    fs::create_dir_all(dir.join("code/main.luau")).unwrap();
    assert_eq!(
        load(&storage, "games/owl"),
        Err(CartridgeError::EntryNotAFile("code/main.luau".into()))
    );
    assert_eq!(
        CartridgeError::EntryNotAFile("code/main.luau".into()).to_string(),
        "Entry is not a file: code/main.luau"
    );
}

#[test]
fn shipped_examples_load_with_their_assets() {
    let examples = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples");
    let root = fs::canonicalize(examples).unwrap();
    let storage = CartridgeStorage::Available(CartridgeFs::new(root));
    for example in ["sprites", "tile-viewer", "signal-lost", "palettes"] {
        let cartridge = LoadedCartridge::load(&storage, example).unwrap();
        assert_eq!(
            cartridge.manifest().runtime.entry,
            "code/main.luau",
            "{example}"
        );
        assert!(!cartridge.entry_source().unwrap().is_empty(), "{example}");
        assert!(cartridge.read_obj_tiles().unwrap().is_some(), "{example}");
    }
}

#[cfg(unix)]
mod symlinks {
    use super::*;
    use pretty_assertions::assert_eq;
    use std::os::unix::fs::symlink;

    fn entry_not_found() -> Result<CartridgeManifest, CartridgeError> {
        Err(CartridgeError::EntryNotFound("main.luau".into()))
    }

    #[test]
    fn entry_links_leaving_the_cartridge_are_not_found() {
        let fixture = Fixture::new("load-link-entry");
        let root = fixture.root();
        write_cartridge(&root.join("games/other"), "Other");
        fs::write(fixture.outside().join("main.luau"), "").unwrap();
        let pong = root.join("games/pong");
        write_manifest(&pong, &manifest_ron("Pong", "main.luau"));
        let storage = fixture.storage();

        let targets = [
            // Outside storage, absolute and relative.
            fixture.outside().join("main.luau"),
            "../../../../outside/main.luau".into(),
            // Inside storage, but in a sibling cartridge.
            root.join("games/other/code/main.luau"),
            "../other/code/main.luau".into(),
        ];
        for target in targets {
            let _ = fs::remove_file(pong.join("main.luau"));
            symlink(&target, pong.join("main.luau")).unwrap();
            assert_eq!(
                load(&storage, "games/pong"),
                entry_not_found(),
                "{target:?}"
            );
        }
    }

    #[test]
    fn entry_through_a_linked_directory_leaving_the_cartridge_is_not_found() {
        let fixture = Fixture::new("load-link-entry-dir");
        let root = fixture.root();
        write_cartridge(&root.join("games/other"), "Other");
        let pong = root.join("games/pong");
        write_manifest(&pong, &manifest_ron("Pong", "code/main.luau"));
        symlink("../other/code", pong.join("code")).unwrap();
        assert_eq!(
            load(&fixture.storage(), "games/pong"),
            Err(CartridgeError::EntryNotFound("code/main.luau".into()))
        );
    }

    #[test]
    fn manifest_links_leaving_the_cartridge_are_missing() {
        let fixture = Fixture::new("load-link-manifest");
        let root = fixture.root();
        write_cartridge(&root.join("games/other"), "Other");
        write_manifest(&fixture.outside(), &manifest_ron("X", "main.luau"));
        let pong = root.join("games/pong");
        fs::create_dir_all(&pong).unwrap();
        fs::write(pong.join("main.luau"), "").unwrap();
        let storage = fixture.storage();

        for target in [
            fixture.outside().join("cartridge.ron"),
            "../other/cartridge.ron".into(),
        ] {
            let _ = fs::remove_file(pong.join("cartridge.ron"));
            symlink(&target, pong.join("cartridge.ron")).unwrap();
            assert_eq!(
                load(&storage, "games/pong"),
                Err(CartridgeError::MissingManifest("/games/pong".into())),
                "{target:?}"
            );
        }
    }

    #[test]
    fn links_inside_the_cartridge_are_followed() {
        let fixture = Fixture::new("load-link-inside");
        let pong = fixture.root().join("games/pong");
        write_manifest(&pong, &manifest_ron("Pong", "main.luau"));
        fs::create_dir_all(pong.join("code")).unwrap();
        fs::write(pong.join("code/game.luau"), "").unwrap();
        symlink("code/game.luau", pong.join("main.luau")).unwrap();
        assert!(load(&fixture.storage(), "games/pong").is_ok());
    }

    #[test]
    fn a_linked_cartridge_is_confined_to_its_target() {
        let fixture = Fixture::new("load-link-cartridge");
        let root = fixture.root();
        write_cartridge(&root.join("games/pong"), "Pong");
        symlink("games/pong", root.join("pong-link")).unwrap();
        let storage = fixture.storage();
        assert_eq!(
            load(&storage, "pong-link").map(|m| m.cartridge.name),
            Ok("Pong".into())
        );

        // From the target's point of view `games/` is outside, even though
        // the link that reached it sits at the storage root.
        let entry = root.join("games/pong/code/main.luau");
        fs::remove_file(&entry).unwrap();
        fs::write(root.join("main.luau"), "").unwrap();
        symlink("../../../main.luau", &entry).unwrap();
        assert_eq!(
            load(&storage, "pong-link"),
            Err(CartridgeError::EntryNotFound("code/main.luau".into()))
        );
    }

    #[test]
    fn module_links_must_stay_inside_code() {
        let fixture = Fixture::new("modules-links");
        let dir = fixture.root().join("pong");
        write_cartridge(&dir, "Pong");
        let code = dir.join("code");
        fs::create_dir_all(dir.join("assets")).unwrap();
        fs::write(dir.join("assets/x.luau"), "").unwrap();
        fs::write(code.join("real.luau"), "return 1").unwrap();
        symlink("real.luau", code.join("inner.luau")).unwrap();
        symlink("../assets/x.luau", code.join("asset.luau")).unwrap();
        symlink("../cartridge.ron", code.join("manifest.luau")).unwrap();
        symlink(dir.join("cartridge.ron"), code.join("manifest_abs.luau")).unwrap();
        fs::write(fixture.outside().join("out.luau"), "").unwrap();
        symlink(fixture.outside().join("out.luau"), code.join("out.luau")).unwrap();
        let cartridge = LoadedCartridge::load(&fixture.storage(), "pong").unwrap();

        assert_eq!(load_module(&cartridge, "inner"), Ok(b"return 1".to_vec()));
        for name in ["asset", "manifest", "manifest_abs", "out"] {
            assert_eq!(
                load_module(&cartridge, name),
                Err(ModuleLoadError::NotFound),
                "{name}"
            );
        }
    }

    #[test]
    fn a_linked_code_directory_has_no_modules() {
        let fixture = Fixture::new("modules-code-link");
        let games = fixture.root().join("games");
        write_cartridge(&games.join("a"), "A");
        write_cartridge(&games.join("b"), "B");
        let a = games.join("a");
        fs::rename(a.join("code"), a.join("src")).unwrap();
        symlink("../b/code", a.join("code")).unwrap();
        write_manifest(&a, &manifest_ron("A", "src/main.luau"));
        let cartridge = LoadedCartridge::load(&fixture.storage(), "games/a").unwrap();
        assert_eq!(
            load_module(&cartridge, "main"),
            Err(ModuleLoadError::NotFound)
        );

        fs::remove_file(a.join("code")).unwrap();
        symlink("src", a.join("code")).unwrap();
        assert_eq!(
            load_module(&cartridge, "main"),
            Err(ModuleLoadError::NotFound)
        );
    }

    #[test]
    fn cartridge_links_leaving_storage_are_not_found() {
        let fixture = Fixture::new("load-link-out");
        write_cartridge(&fixture.outside(), "Out");
        symlink(fixture.outside(), fixture.root().join("out")).unwrap();
        assert_eq!(
            load(&fixture.storage(), "out"),
            Err(CartridgeError::Fs(FsError::NotFound("/out".into())))
        );
    }
}
