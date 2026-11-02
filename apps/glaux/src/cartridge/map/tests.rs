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
