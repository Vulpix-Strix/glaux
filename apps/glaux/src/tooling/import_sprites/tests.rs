use std::io::BufWriter;

use super::*;
use crate::cartridge::fixture::write_cartridge;
use crate::cartridge_fs::fixture::Fixture;
use crate::tooling::import_publish::{RECORD, STAGING};
use glaux_machine::obj_palette::ObjPalettes;
use pretty_assertions::assert_eq;

fn rgba_png(path: &Path, width: u32, height: u32, rgba: &[u8]) {
    let file = BufWriter::new(File::create(path).unwrap());
    let mut encoder = png::Encoder::new(file, width, height);
    encoder.set_color(ColorType::Rgba);
    encoder.set_depth(BitDepth::Eight);
    encoder
        .write_header()
        .unwrap()
        .write_image_data(rgba)
        .unwrap();
}

/// A 16x8 sheet: the left tile in 3 colors, the right tile in 3 others.
fn two_tile_sheet() -> Vec<u8> {
    let mut rgba = Vec::new();
    for y in 0..8u8 {
        for x in 0..16u8 {
            let shade = (x % 8 + y) % 3;
            rgba.extend(match (x < 8, shade) {
                (_, 0) => [0, 0, 0, 0],
                (true, n) => [200, 10 * n, 10, 255],
                (false, n) => [10, 10 * n, 200, 255],
            });
        }
    }
    rgba
}

struct Cart {
    _fixture: Fixture,
    dir: PathBuf,
}

fn cartridge(name: &str) -> Cart {
    let fixture = Fixture::new(name);
    let dir = fixture.root().join("art");
    write_cartridge(&dir, "Art");
    rgba_png(&dir.join("sprites.png"), 16, 8, &two_tile_sheet());
    Cart {
        _fixture: fixture,
        dir,
    }
}

fn options(cart: &Cart) -> ImportOptions {
    ImportOptions {
        input: cart.dir.join("sprites.png"),
        ..ImportOptions::default()
    }
}

fn rom() -> SystemRom {
    crate::rom::load().unwrap()
}

fn written(dir: &Path) -> Vec<Option<Vec<u8>>> {
    ["sprites.bin", "objattr.bin", "obp.bin", PALETTE, RECORD]
        .iter()
        .map(|name| fs::read(dir.join(name)).ok())
        .collect()
}

#[test]
fn imports_a_png_into_its_cartridge_and_reimports_identically() {
    let cart = cartridge("import-png");
    let outcome = import(&options(&cart), &rom()).unwrap();
    assert!(outcome.written);
    assert_eq!(outcome.summary.colors, 4);
    assert_eq!(outcome.summary.banks_used, 1);
    assert_eq!(outcome.previous, None);
    let first = written(&cart.dir);
    assert!(first.iter().all(Option::is_some));
    assert!(!cart.dir.join(STAGING).exists());
    let record = String::from_utf8(first[4].clone().unwrap()).unwrap();
    assert!(record.contains("source: \"sprites.png\""), "{record}");

    let again = import(&options(&cart), &rom()).unwrap();
    assert_eq!(again.previous.as_deref(), Some("sprites.png"));
    assert_eq!(written(&cart.dir), first);
}

#[test]
fn reimporting_the_example_reproduces_its_committed_assets() {
    let example = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/palettes");
    let fixture = Fixture::new("import-example");
    let dir = fixture.root().join("palettes");
    fs::create_dir_all(&dir).unwrap();
    for name in ["cartridge.ron", "sprites.png", "sprites.sprites.ron"] {
        fs::copy(example.join(name), dir.join(name)).unwrap();
    }
    let input = dir.join("sprites.png");
    // The example was imported while the ROM's default banks were linear,
    // and its unused banks keep those. Explicit obp.bin files are not
    // rewritten when the ROM defaults change.
    let rom = rom().with_obj_palettes(ObjPalettes::linear());
    import(
        &ImportOptions {
            input,
            ..ImportOptions::default()
        },
        &rom,
    )
    .unwrap();
    assert_eq!(written(&dir), written(&example));
}

#[test]
fn a_dry_run_touches_nothing() {
    let cart = cartridge("import-dry-run");
    let outcome = import(
        &ImportOptions {
            dry_run: true,
            ..options(&cart)
        },
        &rom(),
    )
    .unwrap();
    assert!(!outcome.written);
    assert_eq!(written(&cart.dir), vec![None; 5]);
}

#[test]
fn converted_sprites_are_not_overwritten_without_replace() {
    let cart = cartridge("import-foreign");
    fs::write(cart.dir.join("sprites.bin"), [0x11; 32]).unwrap();
    let err = import(&options(&cart), &rom()).unwrap_err().to_string();
    assert!(err.contains("sprites.bin was not written"), "{err}");
    assert_eq!(fs::read(cart.dir.join("sprites.bin")).unwrap(), [0x11; 32]);
    assert!(!cart.dir.join("obp.bin").exists());

    let replaced = ImportOptions {
        replace: true,
        ..options(&cart)
    };
    assert!(import(&replaced, &rom()).unwrap().written);
    assert_ne!(fs::read(cart.dir.join("sprites.bin")).unwrap(), [0x11; 32]);
}

#[test]
fn the_default_sidecar_groups_tiles() {
    let cart = cartridge("import-groups");
    fs::write(
        cart.dir.join("sprites.sprites.ron"),
        r#"(groups: [(name: "pair", tile: (0, 0), size: (2, 1))])"#,
    )
    .unwrap();
    let outcome = import(&options(&cart), &rom()).unwrap();
    assert_eq!(
        outcome.summary.group_banks,
        [(
            "pair".to_string(),
            glaux_machine::obj_palette::ObjBank::ZERO
        )]
    );
    let record = fs::read_to_string(cart.dir.join(RECORD)).unwrap();
    assert!(
        record.contains("groups: Some(\"sprites.sprites.ron\")"),
        "{record}"
    );
}

#[test]
fn the_png_directory_must_be_a_cartridge() {
    let fixture = Fixture::new("import-not-a-cartridge");
    let input = fixture.root().join("sprites.png");
    rgba_png(&input, 16, 8, &two_tile_sheet());
    let err = import(
        &ImportOptions {
            input,
            ..ImportOptions::default()
        },
        &rom(),
    )
    .unwrap_err()
    .to_string();
    assert!(err.contains("is not a cartridge directory"), "{err}");
}

#[test]
fn hex_colors_parse_with_or_without_a_hash() {
    assert_eq!(parse_hex_color("#ff00Ff"), Ok(Rgb::from_hex(0xff00ff)));
    assert_eq!(parse_hex_color("123456"), Ok(Rgb::from_hex(0x123456)));
    assert!(parse_hex_color("12345").is_err());
    assert!(parse_hex_color("+12345").is_err());
}
