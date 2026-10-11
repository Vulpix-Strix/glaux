use glaux_machine::sprite::ObjTileBank;

use super::*;
use crate::cartridge_fs::fixture::Fixture;
use fixture::rgba_png;
use pretty_assertions::assert_eq;

const MAGENTA: [u8; 4] = [255, 0, 255, 255];

fn rom() -> glaux_machine::system_rom::SystemRom {
    crate::rom::load().unwrap()
}

/// An 8x8 sheet on an opaque magenta background with a 2x2 square of two
/// colors in the middle.
fn keyed_sheet(fixture: &Fixture) -> SheetSource {
    let mut rgba = Vec::new();
    for y in 0..8 {
        for x in 0..8 {
            rgba.extend(match (x, y) {
                (3, 3 | 4) => [10, 20, 30, 255],
                (4, 3 | 4) => [40, 50, 60, 255],
                _ => MAGENTA,
            });
        }
    }
    let path = fixture.root().join("keyed.png");
    rgba_png(&path, 8, 8, &rgba);
    SheetSource::read(&path, None).unwrap()
}

#[test]
fn the_top_left_key_makes_the_background_transparent() {
    let fixture = Fixture::new("sprite-import-key");
    let source = keyed_sheet(&fixture);
    let rom = rom();
    let fit = |key| {
        source
            .fit(key, rom.palette(), rom.palette(), rom.obj_palettes())
            .unwrap()
    };

    let opaque = fit(None);
    assert_eq!(opaque.summary.colors, 3);
    let tiles = ObjTileBank::from_bytes(&opaque.tiles).unwrap();
    assert_ne!(tiles.pixel(0, 0, 0), Some(0));

    let keyed = fit(source.image.top_left());
    assert_eq!(keyed.summary.colors, 2);
    let tiles = ObjTileBank::from_bytes(&keyed.tiles).unwrap();
    assert_eq!(tiles.pixel(0, 0, 0), Some(0));
    assert_ne!(tiles.pixel(0, 3, 3), Some(0));
}

#[test]
fn an_explicit_sidecar_must_exist() {
    let fixture = Fixture::new("sprite-import-missing-groups");
    let path = fixture.root().join("sheet.png");
    rgba_png(&path, 8, 8, &[0; 8 * 8 * 4]);
    let missing = fixture.root().join("nope.sprites.ron");
    let err = SheetSource::read(&path, Some(&missing)).unwrap_err();
    assert!(err.to_string().contains("does not exist"), "{err}");
}
