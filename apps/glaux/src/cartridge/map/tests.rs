use std::fs;

use super::*;
use crate::cartridge::fixture::write_cartridge;
use crate::cartridge_fs::fixture::Fixture;
use pretty_assertions::assert_eq;

const OVERWORLD: &str = r#"(
    format: 1,
    name: "Overworld",
    width: 128,
    height: 64,
    layers: (bg0: Some("ground"), bg2: Some("sky")),
)"#;

fn meta(ron: &str) -> Result<MapMeta, MapError> {
    MapMeta::parse(ron.as_bytes())
}

fn with(from: &str, to: &str) -> Result<MapMeta, MapError> {
    assert!(OVERWORLD.contains(from), "{from:?} not in the template");
    meta(&OVERWORLD.replacen(from, to, 1))
}

fn loaded(name: &str) -> (Fixture, std::path::PathBuf, LoadedCartridge) {
    let fixture = Fixture::new(name);
    let dir = fixture.root().join("pong");
    write_cartridge(&dir, "Pong");
    let cartridge = LoadedCartridge::load(&fixture.storage(), "pong").unwrap();
    (fixture, dir, cartridge)
}

fn id(text: &str) -> MapId {
    MapId::parse(text).unwrap()
}

#[test]
fn parses_a_map_with_optional_layer_names() {
    assert_eq!(
        meta(OVERWORLD),
        Ok(MapMeta {
            name: "Overworld".into(),
            width: 128,
            height: 64,
            layer_names: [Some("ground".into()), None, Some("sky".into())],
        })
    );
}

#[test]
fn layer_names_may_be_left_out_entirely() {
    let parsed = meta(r#"(format: 1, name: "A", width: 1, height: 1)"#).unwrap();
    assert_eq!(parsed.layer_names, [None, None, None]);
}

#[test]
fn only_format_1_is_supported() {
    assert_eq!(
        with("format: 1", "format: 2"),
        Err(MapError::UnsupportedFormat(2))
    );
}

#[test]
fn missing_and_unknown_fields_are_named() {
    assert_eq!(
        with("    width: 128,\n", ""),
        Err(MapError::MissingField("width".into()))
    );
    assert_eq!(
        with("height: 64,", "height: 64, depth: 1,"),
        Err(MapError::UnknownField("depth".into()))
    );
    assert_eq!(
        with("bg2:", "bg3:"),
        Err(MapError::UnknownField("layers.bg3".into()))
    );
}

#[test]
fn malformed_ron_reports_the_line() {
    assert!(matches!(
        meta("(format: 1,\n name: 5)"),
        Err(MapError::Malformed { line: 2, .. })
    ));
}

#[test]
fn names_must_not_be_blank() {
    assert_eq!(with("\"Overworld\"", "\"  \""), Err(MapError::BlankName));
    assert_eq!(
        with("\"sky\"", "\"\""),
        Err(MapError::BlankLayerName(BgLayer::Bg2))
    );
}

#[test]
fn sizes_run_from_1_to_256() {
    for (w, h) in [(0, 1), (1, 0), (257, 1), (1, 257)] {
        let ron = format!(r#"(format: 1, name: "A", width: {w}, height: {h})"#);
        assert_eq!(
            meta(&ron),
            Err(MapError::InvalidSize {
                width: w,
                height: h
            })
        );
    }
    for (w, h) in [(1, 1), (256, 256)] {
        let ron = format!(r#"(format: 1, name: "A", width: {w}, height: {h})"#);
        assert!(meta(&ron).is_ok(), "{w}x{h}");
    }
}

#[test]
fn written_metadata_reads_back_the_same() {
    let original = meta(OVERWORLD).unwrap();
    let bytes = original.to_bytes();
    assert_eq!(MapMeta::parse(&bytes), Ok(original.clone()));
    assert_eq!(original.to_bytes(), bytes);
}

#[test]
fn ids_are_lowercase_slugs() {
    for ok in ["a", "overworld", "level-1", "a_b", "0", &"a".repeat(32)] {
        assert!(MapId::parse(ok).is_ok(), "{ok}");
    }
    for bad in [
        "",
        "-a",
        "_a",
        "A",
        "a b",
        "a/b",
        "..",
        "a.b",
        "é",
        &"a".repeat(33),
    ] {
        assert!(MapId::parse(bad).is_err(), "{bad:?}");
    }
}

#[test]
fn a_cartridge_without_maps_lists_none() {
    let (_fixture, _dir, cartridge) = loaded("map-list-none");
    assert_eq!(cartridge.list_maps(), Ok(vec![]));
}

#[test]
fn lists_map_directories_by_id_and_skips_the_rest() {
    let (_fixture, dir, cartridge) = loaded("map-list");
    for name in ["zeta", "alpha", "Bad Name"] {
        fs::create_dir_all(dir.join("maps").join(name)).unwrap();
    }
    fs::write(dir.join("maps/notes.txt"), "").unwrap();
    assert_eq!(cartridge.list_maps(), Ok(vec![id("alpha"), id("zeta")]));
}

#[test]
fn reads_a_map_with_missing_layers_and_fresh_bytes() {
    let (_fixture, dir, cartridge) = loaded("map-read");
    let map = id("overworld");
    assert_eq!(
        cartridge.read_map(&map),
        Err(MapError::MissingMeta(map.clone()))
    );
    let root = dir.join("maps/overworld");
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("map.ron"), OVERWORLD).unwrap();
    fs::write(root.join("bg1.bin"), [1, 2]).unwrap();
    let files = cartridge.read_map(&map).unwrap();
    assert_eq!(files.meta, OVERWORLD.as_bytes());
    assert_eq!(files.layers, [None, Some(vec![1, 2]), None]);
    fs::write(root.join("bg1.bin"), [3]).unwrap();
    assert_eq!(
        cartridge.read_map_file(&map, MapFile::Layer(BgLayer::Bg1)),
        Ok(Some(vec![3]))
    );
}

#[test]
fn a_map_file_that_is_a_directory_is_an_error() {
    let (_fixture, dir, cartridge) = loaded("map-read-dir");
    fs::create_dir_all(dir.join("maps/a/bg0.bin")).unwrap();
    assert_eq!(
        cartridge.read_map_file(&id("a"), MapFile::Layer(BgLayer::Bg0)),
        Err(MapError::NotAFile {
            id: id("a"),
            file: MapFile::Layer(BgLayer::Bg0)
        })
    );
}

#[test]
fn replace_creates_only_when_absent_and_only_swaps_what_was_seen() {
    let (_fixture, dir, cartridge) = loaded("map-replace");
    fs::create_dir_all(dir.join("maps/a")).unwrap();
    let (map, file) = (id("a"), MapFile::Layer(BgLayer::Bg0));
    assert_eq!(cartridge.replace_map_file(&map, file, &[1], None), Ok(()));
    assert_eq!(
        cartridge.replace_map_file(&map, file, &[2], None),
        Err(MapWriteError::Changed(Some(vec![1])))
    );
    assert_eq!(
        cartridge.replace_map_file(&map, file, &[2], Some(&[9])),
        Err(MapWriteError::Changed(Some(vec![1])))
    );
    assert_eq!(
        cartridge.replace_map_file(&map, file, &[2], Some(&[1])),
        Ok(())
    );
    assert_eq!(fs::read(dir.join("maps/a/bg0.bin")).unwrap(), [2]);
}

#[test]
fn remove_requires_the_file_to_hold_what_was_seen() {
    let (_fixture, dir, cartridge) = loaded("map-remove");
    fs::create_dir_all(dir.join("maps/a")).unwrap();
    let (map, file) = (id("a"), MapFile::Layer(BgLayer::Bg2));
    let path = dir.join("maps/a/bg2.bin");

    assert_eq!(
        cartridge.remove_map_file(&map, file, &[1]),
        Err(MapWriteError::Changed(None))
    );
    fs::write(&path, [7]).unwrap();
    assert_eq!(
        cartridge.remove_map_file(&map, file, &[1]),
        Err(MapWriteError::Changed(Some(vec![7])))
    );
    assert!(path.exists());
    assert_eq!(cartridge.remove_map_file(&map, file, &[7]), Ok(()));
    assert!(!path.exists());
    assert!(dir.join("maps/a").exists());
}

#[test]
fn new_metadata_follows_the_same_rules_as_a_file() {
    let created = MapMeta::new("Overworld", 128, 64).unwrap();
    assert_eq!(MapMeta::parse(&created.to_bytes()), Ok(created));
    assert_eq!(MapMeta::new(" ", 1, 1), Err(MapError::BlankName));
    for (w, h) in [(0, 1), (1, 0), (257, 1), (1, 257)] {
        assert_eq!(
            MapMeta::new("A", w, h),
            Err(MapError::InvalidSize {
                width: w as u32,
                height: h as u32
            })
        );
    }
    assert!(MapMeta::new("A", 1, 1).is_ok());
    assert!(MapMeta::new("A", 256, 256).is_ok());
}

#[test]
fn creating_a_map_writes_only_its_metadata() {
    let (_fixture, dir, cartridge) = loaded("map-create");
    let meta = MapMeta::new("Overworld", 16, 8).unwrap();
    assert_eq!(cartridge.create_map(&id("overworld"), &meta), Ok(()));

    let root = dir.join("maps/overworld");
    let mut names: Vec<_> = fs::read_dir(&root)
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect();
    names.sort();
    assert_eq!(names, ["map.ron"]);
    assert_eq!(fs::read(root.join("map.ron")).unwrap(), meta.to_bytes());
    assert_eq!(cartridge.list_maps(), Ok(vec![id("overworld")]));
}

#[test]
fn creating_a_map_never_replaces_one() {
    let (_fixture, dir, cartridge) = loaded("map-create-twice");
    let meta = MapMeta::new("A", 1, 1).unwrap();
    cartridge.create_map(&id("a"), &meta).unwrap();
    fs::write(dir.join("maps/a/bg0.bin"), [0, 0]).unwrap();

    let other = MapMeta::new("B", 2, 2).unwrap();
    assert_eq!(
        cartridge.create_map(&id("a"), &other),
        Err(MapCreateError::AlreadyExists(id("a")))
    );
    assert_eq!(
        fs::read(dir.join("maps/a/map.ron")).unwrap(),
        meta.to_bytes()
    );
    assert!(dir.join("maps/a/bg0.bin").exists());
}

#[test]
fn creating_a_map_where_maps_is_a_file_fails() {
    let (_fixture, dir, cartridge) = loaded("map-create-blocked");
    fs::write(dir.join("maps"), "").unwrap();
    let meta = MapMeta::new("A", 1, 1).unwrap();
    assert!(matches!(
        cartridge.create_map(&id("a"), &meta),
        Err(MapCreateError::Fs(_))
    ));
}

#[test]
fn summaries_keep_maps_that_cannot_be_opened() {
    let (_fixture, dir, cartridge) = loaded("map-summaries");
    let good = MapMeta::new("Good", 4, 3).unwrap();
    cartridge.create_map(&id("good"), &good).unwrap();
    fs::create_dir_all(dir.join("maps/empty")).unwrap();
    fs::create_dir_all(dir.join("maps/broken")).unwrap();
    fs::write(dir.join("maps/broken/map.ron"), "(").unwrap();

    let summaries = cartridge.map_summaries().unwrap();
    let ids: Vec<_> = summaries.iter().map(|s| s.id.clone()).collect();
    assert_eq!(ids, [id("broken"), id("empty"), id("good")]);
    assert!(matches!(summaries[0].meta, Err(MapError::Malformed { .. })));
    assert_eq!(summaries[1].meta, Err(MapError::MissingMeta(id("empty"))));
    assert_eq!(summaries[2].meta, Ok(good));
}

/// Creates map `a` with a layer file and returns what `read_map` reads from it.
fn stored_map(name: &str) -> (Fixture, std::path::PathBuf, LoadedCartridge, MapFiles) {
    let (fixture, dir, cartridge) = loaded(name);
    let meta = MapMeta::new("A", 1, 1).unwrap();
    cartridge.create_map(&id("a"), &meta).unwrap();
    fs::write(dir.join("maps/a/bg1.bin"), [5, 0]).unwrap();
    let seen = cartridge.read_map(&id("a")).unwrap();
    (fixture, dir, cartridge, seen)
}

#[test]
fn deleting_a_map_removes_its_files_and_directory() {
    let (_fixture, dir, cartridge, seen) = stored_map("map-delete");
    fs::create_dir_all(dir.join("maps/b")).unwrap();
    assert_eq!(cartridge.delete_map(&id("a"), &seen), Ok(()));
    assert!(!dir.join("maps/a").exists());
    assert!(dir.join("maps/b").exists());
}

#[test]
fn deleting_a_missing_map_says_so() {
    let (_fixture, _dir, cartridge, seen) = stored_map("map-delete-missing");
    assert_eq!(
        cartridge.delete_map(&id("zzz"), &seen),
        Err(MapDeleteError::Missing(id("zzz")))
    );
}

#[test]
fn a_changed_file_stops_the_delete_before_anything_is_removed() {
    let (_fixture, dir, cartridge, seen) = stored_map("map-delete-changed");
    fs::write(dir.join("maps/a/bg1.bin"), [6, 0]).unwrap();
    assert_eq!(
        cartridge.delete_map(&id("a"), &seen),
        Err(MapDeleteError::Changed {
            id: id("a"),
            file: MapFile::Layer(BgLayer::Bg1),
            current: Some(vec![6, 0]),
        })
    );
    for file in ["map.ron", "bg1.bin"] {
        assert!(dir.join("maps/a").join(file).exists(), "{file}");
    }
}

#[test]
fn a_layer_created_since_it_was_read_counts_as_a_change() {
    let (_fixture, dir, cartridge, seen) = stored_map("map-delete-created");
    fs::write(dir.join("maps/a/bg0.bin"), [1, 0]).unwrap();
    assert_eq!(
        cartridge.delete_map(&id("a"), &seen),
        Err(MapDeleteError::Changed {
            id: id("a"),
            file: MapFile::Layer(BgLayer::Bg0),
            current: Some(vec![1, 0]),
        })
    );
    assert!(dir.join("maps/a/map.ron").exists());
}

#[test]
fn a_file_removed_since_it_was_read_counts_as_a_change() {
    let (_fixture, dir, cartridge, seen) = stored_map("map-delete-gone");
    fs::remove_file(dir.join("maps/a/bg1.bin")).unwrap();
    assert_eq!(
        cartridge.delete_map(&id("a"), &seen),
        Err(MapDeleteError::Changed {
            id: id("a"),
            file: MapFile::Layer(BgLayer::Bg1),
            current: None,
        })
    );
    assert!(dir.join("maps/a/map.ron").exists());
}

#[test]
fn files_of_other_origin_stop_the_delete_untouched() {
    let (_fixture, dir, cartridge, seen) = stored_map("map-delete-unknown");
    fs::write(dir.join("maps/a/notes.txt"), "keep").unwrap();
    fs::create_dir(dir.join("maps/a/sub")).unwrap();
    assert_eq!(
        cartridge.delete_map(&id("a"), &seen),
        Err(MapDeleteError::UnknownEntries {
            id: id("a"),
            names: vec!["notes.txt".into(), "sub".into()],
        })
    );
    for file in ["map.ron", "bg1.bin", "notes.txt"] {
        assert!(dir.join("maps/a").join(file).exists(), "{file}");
    }
}

#[test]
fn a_file_that_cannot_be_removed_stops_the_delete_before_anything_is_removed() {
    let (_fixture, dir, cartridge, seen) = stored_map("map-delete-read-only");
    fs::write(dir.join("maps/a/bg2.bin"), [1, 0]).unwrap();
    let seen = MapFiles {
        layers: [None, seen.layers[1].clone(), Some(vec![1, 0])],
        ..seen
    };
    // Layers are removed from the last one, so the locked file is the
    // first one the removal would touch after bg2.
    let locked = dir.join("maps/a/bg1.bin");
    let mut permissions = fs::metadata(&locked).unwrap().permissions();
    permissions.set_readonly(true);
    fs::set_permissions(&locked, permissions).unwrap();

    assert_eq!(
        cartridge.delete_map(&id("a"), &seen),
        Err(MapDeleteError::Map(MapError::Fs(FsError::ReadOnly(
            "/maps/a/bg1.bin".into()
        ))))
    );
    for file in ["map.ron", "bg1.bin", "bg2.bin"] {
        assert!(dir.join("maps/a").join(file).exists(), "{file}");
    }
}

#[test]
fn staging_leftovers_do_not_block_a_delete() {
    let (_fixture, dir, cartridge, seen) = stored_map("map-delete-staging");
    fs::write(dir.join("maps/a/.glaux-staging-map.ron-1"), "half").unwrap();
    assert_eq!(cartridge.delete_map(&id("a"), &seen), Ok(()));
    assert!(!dir.join("maps/a").exists());
}
