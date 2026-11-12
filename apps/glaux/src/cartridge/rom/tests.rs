use std::fs;
use std::path::Path;

use super::*;
use crate::cartridge::fixture::{manifest_ron, write_cartridge, write_manifest};
use crate::cartridge::{CartridgeManifest, LoadedCartridge};
use crate::cartridge_fs::fixture::Fixture;
use pretty_assertions::assert_eq;

/// A cartridge named `Pong` whose manifest declares `resources`, with an
/// empty entry at `code/main.luau`.
fn loaded(name: &str, resources: &str) -> (Fixture, std::path::PathBuf, LoadedCartridge) {
    let fixture = Fixture::new(name);
    let dir = fixture.root().join("pong");
    write_cartridge(&dir, "Pong");
    if !resources.is_empty() {
        let ron = manifest_ron("Pong", "code/main.luau").replace(
            "min_glaux: \"0.1.0\"),",
            &format!("min_glaux: \"0.1.0\"),\n    resources: {resources},"),
        );
        write_manifest(&dir, &ron);
    }
    let cartridge = LoadedCartridge::load(&fixture.storage(), "pong").unwrap();
    (fixture, dir, cartridge)
}

/// Writes `size` zero bytes at `path` below `dir`, creating directories.
fn write(dir: &Path, path: &str, size: usize) {
    let file = dir.join(path);
    fs::create_dir_all(file.parent().unwrap()).unwrap();
    fs::write(file, vec![0; size]).unwrap();
}

fn paths(selection: &RomSelection) -> Vec<&str> {
    selection.files.iter().map(|f| f.path.as_str()).collect()
}

fn category_of(selection: &RomSelection, path: &str) -> Option<RomCategory> {
    selection
        .files
        .iter()
        .find(|f| f.path == path)
        .map(|f| f.category)
}

fn manifest_len(dir: &Path) -> u64 {
    fs::metadata(dir.join("cartridge.ron")).unwrap().len()
}

#[test]
fn selects_native_files_and_resources() {
    let (_fixture, dir, cartridge) = loaded("rom-select", r#"["data"]"#);
    write(&dir, "code/util.luau", 3);
    write(&dir, "code/sub/deep.luau", 4);
    write(&dir, "sprites.bin", 5);
    write(&dir, "palette.bin", 768);
    write(&dir, "maps/overworld/map.ron", 7);
    write(&dir, "maps/overworld/bg1.bin", 8);
    write(&dir, "assets/hit.wav", 9);
    write(&dir, "assets/ui/icon.png", 10);
    write(&dir, "data/levels.json", 11);

    let selection = cartridge.rom_selection();

    assert_eq!(
        paths(&selection),
        [
            "assets/hit.wav",
            "assets/ui/icon.png",
            "cartridge.ron",
            "code/main.luau",
            "code/sub/deep.luau",
            "code/util.luau",
            "data/levels.json",
            "maps/overworld/bg1.bin",
            "maps/overworld/map.ron",
            "palette.bin",
            "sprites.bin",
        ]
    );
    assert_eq!(selection.issues, []);
    assert_eq!(
        category_of(&selection, "cartridge.ron"),
        Some(RomCategory::Manifest)
    );
    assert_eq!(
        category_of(&selection, "code/sub/deep.luau"),
        Some(RomCategory::Code)
    );
    assert_eq!(
        category_of(&selection, "palette.bin"),
        Some(RomCategory::Graphics)
    );
    assert_eq!(
        category_of(&selection, "maps/overworld/bg1.bin"),
        Some(RomCategory::Maps)
    );
    assert_eq!(
        category_of(&selection, "data/levels.json"),
        Some(RomCategory::Resources)
    );
}

#[test]
fn leaves_out_authoring_and_development_files() {
    let (_fixture, dir, cartridge) = loaded("rom-exclude", "");
    for file in [
        "README.md",
        "sprites.png",
        "sprites.import.ron",
        "sprites.sprites.ron",
        "tools/build.py",
        ".git/config",
        ".gitignore",
        "saves/slot1.sav",
        "notes.txt",
        "code/notes.md",
        "code/other.rhai",
        "maps/overworld/notes.md",
        "maps/Not A Map/map.ron",
        "maps/readme.txt",
    ] {
        write(&dir, file, 100);
    }

    let selection = cartridge.rom_selection();

    assert_eq!(paths(&selection), ["cartridge.ron", "code/main.luau"]);
    assert_eq!(selection.issues, []);
}

#[test]
fn resource_directories_take_any_extension_but_no_dotfiles() {
    let (_fixture, dir, cartridge) = loaded("rom-any-ext", r#"["resources"]"#);
    for file in [
        "assets/a.png",
        "assets/b.json",
        "assets/c.ron",
        "assets/d.wav",
        "resources/e.luau",
        "resources/noext",
        "assets/.DS_Store",
        "assets/.cache/x.bin",
        "resources/.hidden",
    ] {
        write(&dir, file, 1);
    }

    let selection = cartridge.rom_selection();

    assert_eq!(
        paths(&selection),
        [
            "assets/a.png",
            "assets/b.json",
            "assets/c.ron",
            "assets/d.wav",
            "cartridge.ron",
            "code/main.luau",
            "resources/e.luau",
            "resources/noext",
        ]
    );
}

#[test]
fn authoring_files_inside_a_resource_directory_are_included() {
    let (_fixture, dir, cartridge) = loaded("rom-authoring-in-assets", "");
    write(&dir, "assets/sprites.png", 20);
    write(&dir, "assets/README.md", 30);

    let selection = cartridge.rom_selection();

    assert_eq!(
        paths(&selection),
        [
            "assets/README.md",
            "assets/sprites.png",
            "cartridge.ron",
            "code/main.luau"
        ]
    );
}

#[test]
fn a_missing_declared_directory_costs_nothing() {
    let (_fixture, dir, cartridge) = loaded("rom-missing-dir", r#"["data"]"#);

    let selection = cartridge.rom_selection();

    assert_eq!(selection.issues, []);
    assert_eq!(selection.usage().used, manifest_len(&dir));
}

#[test]
fn an_entry_outside_code_is_selected_once() {
    let fixture = Fixture::new("rom-entry");
    let dir = fixture.root().join("pong");
    write_manifest(&dir, &manifest_ron("Pong", "game/main.luau"));
    write(&dir, "game/main.luau", 12);
    let cartridge = LoadedCartridge::load(&fixture.storage(), "pong").unwrap();

    assert_eq!(
        paths(&cartridge.rom_selection()),
        ["cartridge.ron", "game/main.luau"]
    );

    // The entry lies in `code/` too, and is still counted once.
    let (_fixture, dir, cartridge) = loaded("rom-entry-in-code", "");
    write(&dir, "code/main.luau", 12);
    let selection = cartridge.rom_selection();
    assert_eq!(paths(&selection), ["cartridge.ron", "code/main.luau"]);
    assert_eq!(selection.usage().used, manifest_len(&dir) + 12);
}

#[test]
fn counts_original_sizes_and_breaks_them_down_by_category() {
    let (_fixture, dir, cartridge) = loaded("rom-usage", r#"["data"]"#);
    write(&dir, "code/main.luau", 100);
    write(&dir, "sprites.bin", 1000);
    write(&dir, "obp.bin", 128);
    write(&dir, "maps/m/map.ron", 50);
    // One 256x256 layer is 128 KiB, so three are 384 KiB.
    for layer in ["bg0", "bg1", "bg2"] {
        write(&dir, &format!("maps/m/{layer}.bin"), 256 * 256 * 2);
    }
    write(&dir, "assets/a.bin", 7);
    write(&dir, "data/b.bin", 3);

    let usage = cartridge.rom_selection().usage();

    let index = RomCategory::index;
    assert_eq!(
        usage.by_category[index(RomCategory::Manifest)],
        manifest_len(&dir)
    );
    assert_eq!(usage.by_category[index(RomCategory::Code)], 100);
    assert_eq!(usage.by_category[index(RomCategory::Graphics)], 1128);
    assert_eq!(usage.by_category[index(RomCategory::Maps)], 50 + 384 * 1024);
    assert_eq!(usage.by_category[index(RomCategory::Resources)], 10);
    assert_eq!(usage.used, usage.by_category.iter().sum::<u64>());
    assert!(!usage.incomplete());
}

/// Pads `assets/pad.bin` so the selection is exactly `target` bytes.
fn pad_to(dir: &Path, cartridge: &LoadedCartridge, target: u64) {
    let used = cartridge.rom_selection().usage().used;
    write(
        dir,
        "assets/pad.bin",
        usize::try_from(target - used).unwrap(),
    );
}

#[test]
fn the_budget_is_exactly_one_mebibyte() {
    let (_fixture, dir, cartridge) = loaded("rom-budget", "");
    pad_to(&dir, &cartridge, CART_ROM_BUDGET);

    let usage = cartridge.rom_selection().usage();
    assert_eq!(usage.used, 1_048_576);
    assert_eq!(usage.check(), Ok(()));
    assert_eq!(usage.available(), 0);
    assert_eq!(usage.excess(), 0);

    write(
        &dir,
        "assets/pad.bin",
        usize::try_from(CART_ROM_BUDGET - manifest_len(&dir) + 1).unwrap(),
    );
    let usage = cartridge.rom_selection().usage();
    assert_eq!(usage.used, 1_048_577);
    assert_eq!(
        usage.check(),
        Err(RomError::OverBudget {
            used: 1_048_577,
            limit: 1_048_576
        })
    );
    assert_eq!(usage.available(), 0);
    assert_eq!(usage.excess(), 1);
}

#[test]
fn one_byte_below_the_budget_leaves_one_byte() {
    let (_fixture, dir, cartridge) = loaded("rom-budget-below", "");
    pad_to(&dir, &cartridge, CART_ROM_BUDGET - 1);

    let usage = cartridge.rom_selection().usage();

    assert_eq!(usage.check(), Ok(()));
    assert_eq!(usage.available(), 1);
}

#[test]
fn an_over_budget_cartridge_still_loads_and_reports_its_files() {
    let (fixture, dir, cartridge) = loaded("rom-over", "");
    pad_to(&dir, &cartridge, 2 * CART_ROM_BUDGET);

    // Development is not blocked: the cartridge loads and its files read.
    let reloaded = LoadedCartridge::load(&fixture.storage(), "pong").unwrap();
    assert_eq!(reloaded.entry_source().unwrap(), b"");
    assert_eq!(reloaded.rom_selection().usage().excess(), CART_ROM_BUDGET);
}

#[test]
fn compression_is_not_considered() {
    let (_fixture, dir, cartridge) = loaded("rom-compression", "");
    // A highly compressible file counts in full.
    write(&dir, "assets/zeros.bin", 500_000);

    assert_eq!(
        cartridge.rom_selection().usage().used,
        manifest_len(&dir) + 500_000
    );
}

#[test]
fn an_incomplete_selection_is_never_within_the_budget() {
    let usage = RomUsage {
        used: 10,
        issues: vec![RomIssue::FileLimit(4096)],
        ..RomUsage::default()
    };
    assert!(usage.incomplete());
    assert!(usage.fits_known());
    assert_eq!(
        usage.check(),
        Err(RomError::IncompleteSelection {
            issues: vec![RomIssue::FileLimit(4096)]
        })
    );

    // Already over the budget: the incompleteness is still what is reported.
    let usage = RomUsage {
        used: CART_ROM_BUDGET + 1,
        ..usage
    };
    assert_eq!(usage.excess(), 1);
    assert!(matches!(
        usage.check(),
        Err(RomError::IncompleteSelection { .. })
    ));
}

#[test]
fn more_files_than_the_scan_limit_make_the_selection_incomplete() {
    let (_fixture, dir, cartridge) = loaded("rom-file-limit", "");
    for i in 0..ROM_LIMITS.max_files + 5 {
        write(&dir, &format!("assets/f{i:05}.bin"), 1);
    }

    let selection = cartridge.rom_selection();

    assert_eq!(selection.files.len(), ROM_LIMITS.max_files);
    assert_eq!(
        selection.issues,
        [RomIssue::FileLimit(ROM_LIMITS.max_files)]
    );
    let usage = selection.usage();
    assert!(usage.used > 0, "the known subtotal is kept");
    assert!(matches!(
        usage.check(),
        Err(RomError::IncompleteSelection { .. })
    ));
}

#[test]
fn directories_below_the_depth_limit_make_the_selection_incomplete() {
    let (_fixture, dir, cartridge) = loaded("rom-depth", "");
    let nested = (0..ROM_LIMITS.max_depth)
        .map(|i| format!("d{i}"))
        .collect::<Vec<_>>()
        .join("/");
    write(&dir, &format!("assets/{nested}/deep.bin"), 1);

    let selection = cartridge.rom_selection();

    assert!(
        selection
            .issues
            .iter()
            .any(|issue| matches!(issue, RomIssue::TooDeep(_))),
        "{:?}",
        selection.issues
    );
}

#[cfg(unix)]
mod links {
    use std::os::unix::fs::symlink;

    use pretty_assertions::assert_eq;

    use super::*;

    fn issues(cartridge: &LoadedCartridge) -> Vec<RomIssue> {
        cartridge.rom_selection().issues
    }

    #[test]
    fn a_link_to_a_file_inside_the_cartridge_is_not_selected_and_is_reported() {
        let (_fixture, dir, cartridge) = loaded("rom-link-file", "");
        write(&dir, "assets/real.bin", 10);
        symlink(dir.join("assets/real.bin"), dir.join("assets/alias.bin")).unwrap();

        let selection = cartridge.rom_selection();

        assert_eq!(
            paths(&selection),
            ["assets/real.bin", "cartridge.ron", "code/main.luau"]
        );
        assert_eq!(
            selection.issues,
            [RomIssue::Symlink("assets/alias.bin".into())]
        );
        assert!(matches!(
            selection.usage().check(),
            Err(RomError::IncompleteSelection { .. })
        ));
    }

    #[test]
    fn links_to_directories_and_outside_the_cartridge_are_reported() {
        let (fixture, dir, cartridge) = loaded("rom-link-dir", r#"["data"]"#);
        write(&dir, "assets/real/x.bin", 10);
        symlink(dir.join("assets/real"), dir.join("assets/dirlink")).unwrap();
        symlink(fixture.outside().join("secret"), dir.join("assets/out.bin")).unwrap();
        symlink(dir.join("nowhere"), dir.join("assets/dangling.bin")).unwrap();
        symlink(dir.join("assets/loop"), dir.join("assets/loop")).unwrap();
        write(&dir, "real-data/y.bin", 4);
        symlink(dir.join("real-data"), dir.join("data")).unwrap();

        let selection = cartridge.rom_selection();

        assert_eq!(
            paths(&selection),
            ["assets/real/x.bin", "cartridge.ron", "code/main.luau"]
        );
        let mut issues = selection.issues;
        issues.sort_by_key(|issue| issue.to_string());
        assert_eq!(
            issues,
            [
                RomIssue::Symlink("assets/dangling.bin".into()),
                RomIssue::Symlink("assets/dirlink".into()),
                RomIssue::Symlink("assets/loop".into()),
                RomIssue::Symlink("assets/out.bin".into()),
                RomIssue::Symlink("data".into()),
            ]
        );
    }

    #[test]
    fn linked_roots_and_native_files_are_reported() {
        let (_fixture, dir, cartridge) = loaded("rom-link-roots", "");
        write(&dir, "real-code/util.luau", 5);
        write(&dir, "real-assets/a.bin", 5);
        write(&dir, "real-maps/m/map.ron", 5);
        write(&dir, "real-sprites", 5);
        fs::remove_dir_all(dir.join("code")).unwrap();
        symlink(dir.join("real-code"), dir.join("code")).unwrap();
        symlink(dir.join("real-assets"), dir.join("assets")).unwrap();
        symlink(dir.join("real-maps"), dir.join("maps")).unwrap();
        symlink(dir.join("real-sprites"), dir.join("sprites.bin")).unwrap();

        let selection = cartridge.rom_selection();

        assert_eq!(paths(&selection), ["cartridge.ron"]);
        for path in ["code", "assets", "maps", "sprites.bin", "code/main.luau"] {
            assert!(
                selection.issues.contains(&RomIssue::Symlink(path.into())),
                "{path}: {:?}",
                selection.issues
            );
        }
    }

    #[test]
    fn a_link_to_a_map_directory_is_reported() {
        let (_fixture, dir, cartridge) = loaded("rom-link-map", "");
        write(&dir, "real/map.ron", 5);
        fs::create_dir_all(dir.join("maps")).unwrap();
        symlink(dir.join("real"), dir.join("maps/linked")).unwrap();

        assert_eq!(
            issues(&cartridge),
            [RomIssue::Symlink("maps/linked".into())]
        );
    }

    #[test]
    fn links_outside_the_selected_paths_are_ignored() {
        let (fixture, dir, cartridge) = loaded("rom-link-ignored", "");
        fs::create_dir_all(dir.join("tools")).unwrap();
        symlink(fixture.outside(), dir.join("tools/outside")).unwrap();
        symlink(dir.join("code"), dir.join("shortcut")).unwrap();

        assert_eq!(issues(&cartridge), []);
    }
}

#[test]
fn resource_dir_names_are_plain_unreserved_directory_names() {
    for good in ["data", "resources", "level-1", "My Data"] {
        assert_eq!(ResourceDir::parse(good).unwrap().as_str(), good);
    }
    for bad in [
        "", "/abs", "/", "..", ".", "a/..", "../x", "a/b", "a\\b", "c:", ".git", ".hidden", "code",
        "maps", "assets", "Assets", "CODE", "a\0b",
    ] {
        assert_eq!(
            ResourceDir::parse(bad),
            Err(CartridgeError::InvalidResourceDir(bad.to_owned())),
            "{bad:?}"
        );
    }
}

fn parse_with(resources: &str) -> Result<CartridgeManifest, CartridgeError> {
    let ron = manifest_ron("Pong", "code/main.luau").replace(
        "min_glaux: \"0.1.0\"),",
        &format!("min_glaux: \"0.1.0\"),\n    resources: {resources},"),
    );
    CartridgeManifest::parse(ron.as_bytes())
}

#[test]
fn the_manifest_reads_resource_directories() {
    assert_eq!(
        parse_with(r#"["data", "extra"]"#).unwrap().resources,
        [
            ResourceDir::parse("data").unwrap(),
            ResourceDir::parse("extra").unwrap()
        ]
    );
    assert_eq!(
        CartridgeManifest::parse(manifest_ron("Pong", "code/main.luau").as_bytes())
            .unwrap()
            .resources,
        []
    );
}

#[test]
fn the_manifest_rejects_repeated_and_escaping_resource_directories() {
    for bad in [
        r#"["data", "data"]"#,
        r#"["data", "DATA"]"#,
        r#"["assets"]"#,
        r#"["../outside"]"#,
        r#"["/etc"]"#,
        r#"["a/b"]"#,
    ] {
        assert!(
            matches!(parse_with(bad), Err(CartridgeError::InvalidResourceDir(_))),
            "{bad}: {:?}",
            parse_with(bad)
        );
    }
}

#[test]
fn a_selection_never_lists_a_path_twice() {
    let (_fixture, dir, cartridge) = loaded("rom-unique", r#"["data"]"#);
    write(&dir, "data/a.bin", 1);
    write(&dir, "assets/a.bin", 1);

    let selection = cartridge.rom_selection();
    let mut seen = paths(&selection);
    let len = seen.len();
    seen.sort_unstable();
    seen.dedup();

    assert_eq!(seen.len(), len);
}

/// A cartridge whose selected directories hold only left-out entries, as
/// many as `count`, still stops the scan and cannot be exported.
fn assert_entry_limit(selection: RomSelection, limit: usize) {
    assert!(
        selection.issues.contains(&RomIssue::EntryLimit(limit)),
        "{:?}",
        selection.issues
    );
    assert!(matches!(
        selection.usage().check(),
        Err(RomError::IncompleteSelection { .. })
    ));
}

#[test]
fn directories_full_of_left_out_entries_stop_the_scan() {
    let limits = ScanLimits {
        max_entries: 50,
        ..ROM_LIMITS
    };
    let (_fixture, dir, cartridge) = loaded("rom-entry-limit-code", "");
    for i in 0..60 {
        write(&dir, &format!("code/note{i:02}.md"), 1);
    }
    assert_entry_limit(cartridge.rom_selection_within(limits), 50);

    // Dotfiles in a resource directory are left out too, and count.
    let (_fixture, dir, cartridge) = loaded("rom-entry-limit-dots", "");
    for i in 0..60 {
        write(&dir, &format!("assets/.cache{i:02}"), 1);
    }
    assert_entry_limit(cartridge.rom_selection_within(limits), 50);

    // So do the entries of `maps/` that are not maps.
    let (_fixture, dir, cartridge) = loaded("rom-entry-limit-maps", "");
    for i in 0..60 {
        write(&dir, &format!("maps/file{i:02}.txt"), 1);
    }
    assert_entry_limit(cartridge.rom_selection_within(limits), 50);
}

#[test]
fn the_entry_limit_is_shared_by_all_directories() {
    let limits = ScanLimits {
        max_entries: 50,
        ..ROM_LIMITS
    };
    let (_fixture, dir, cartridge) = loaded("rom-entry-limit-shared", r#"["data"]"#);
    for i in 0..30 {
        write(&dir, &format!("assets/.a{i:02}"), 1);
        write(&dir, &format!("data/.b{i:02}"), 1);
    }

    assert_entry_limit(cartridge.rom_selection_within(limits), 50);
}

#[test]
fn a_directory_exactly_at_the_entry_limit_is_complete() {
    let limits = ScanLimits {
        max_entries: 50,
        ..ROM_LIMITS
    };
    let (_fixture, dir, cartridge) = loaded("rom-entry-limit-exact", "");
    // `code/` holds `main.luau` and 49 notes.
    for i in 0..49 {
        write(&dir, &format!("code/note{i:02}.md"), 1);
    }

    let selection = cartridge.rom_selection_within(limits);

    assert_eq!(selection.issues, []);
}

#[cfg(unix)]
#[test]
fn a_link_in_an_ancestor_of_the_entry_is_reported_not_followed() {
    use std::os::unix::fs::symlink;

    let fixture = Fixture::new("rom-link-entry-ancestor");
    let dir = fixture.root().join("pong");
    write_manifest(&dir, &manifest_ron("Pong", "game/main.luau"));
    write(&dir, "real/main.luau", 12);
    symlink(dir.join("real"), dir.join("game")).unwrap();
    // The cartridge loads, because loading follows links inside it.
    let cartridge = LoadedCartridge::load(&fixture.storage(), "pong").unwrap();

    let selection = cartridge.rom_selection();

    assert_eq!(paths(&selection), ["cartridge.ron"]);
    assert_eq!(
        selection.issues,
        [RomIssue::Symlink("game/main.luau".into())]
    );
}
