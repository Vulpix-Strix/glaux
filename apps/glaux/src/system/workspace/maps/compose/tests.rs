use super::*;
use glaux_machine::bg::{BG_PALETTES_BYTES, BgPalettes, BgTileBank};
use glaux_machine::hardware::CartridgeHardware;
use glaux_machine::obj_palette::ObjPalettes;
use glaux_machine::palette::Palette;
use glaux_machine::sprite::{ObjTileAttributes, ObjTileBank, ObjVram};
use glaux_machine::tilemap::BgBank;
use glaux_video::compose::compose_layers;
use pretty_assertions::assert_eq;

struct Lcg(u64);

impl Lcg {
    fn next(&mut self) -> u32 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        (self.0 >> 33) as u32
    }
}

/// Tiles of random pixels, and banks whose entries are all master colors
/// other than 0 so that no opaque pixel can look like an untouched one.
fn graphics(random: &mut Lcg) -> BgVram {
    let bytes: Vec<u8> = (0..1024 * 32).map(|_| random.next() as u8).collect();
    let mut palettes = [0u8; BG_PALETTES_BYTES];
    for (offset, byte) in palettes.iter_mut().enumerate() {
        if offset % 16 != 0 {
            *byte = 1 + (random.next() % 255) as u8;
        }
    }
    BgVram::new(
        BgTileBank::from_bytes(&bytes).unwrap(),
        BgPalettes::from_bytes(&palettes).unwrap(),
    )
}

fn layer(random: &mut Lcg, width: usize, height: usize) -> TileLayer {
    let mut layer = TileLayer::empty(width, height).unwrap();
    for y in 0..height {
        for x in 0..width {
            let entry = match random.next() % 4 {
                0 => TileEntry::EMPTY,
                // Tile 0 with attributes is transparent too.
                1 => TileEntry::from_raw(random.next() as u16 & 0x7c00).unwrap(),
                _ => TileEntry::from_raw(random.next() as u16 & 0x7fff).unwrap(),
            };
            layer.set(x, y, entry);
        }
    }
    layer
}

fn composed(layers: &[&TileLayer; 3], bg: &BgVram, enabled: u8) -> Vec<u8> {
    let mut hardware = CartridgeHardware::new(
        Palette::default(),
        ObjVram::new(
            ObjTileBank::new(),
            ObjTileAttributes::new(),
            ObjPalettes::linear(),
        ),
        bg.clone(),
    );
    for (window, layer) in hardware.tilemaps.iter_mut().zip(layers) {
        window.transfer(layer, 0, 0, 40, 30);
    }
    assert!(hardware.registers.set_byte(0, enabled));
    compose_layers(&mut hardware);
    hardware.framebuffer.as_bytes().to_vec()
}

#[test]
fn the_preview_draws_what_the_compositor_draws_for_every_combination_of_layers() {
    let mut random = Lcg(7);
    let bg = graphics(&mut random);
    let layers = [
        layer(&mut random, 40, 30),
        layer(&mut random, 40, 30),
        layer(&mut random, 40, 30),
    ];
    let refs = [&layers[0], &layers[1], &layers[2]];
    for enabled in 0..8u8 {
        let shown = [0, 1, 2].map(|bit| enabled & (1 << bit) != 0);
        let preview = Composer::new(refs, shown);
        let frame = composed(&refs, &bg, enabled);
        for y in 0..240 {
            for x in 0..320 {
                assert_eq!(
                    preview.pixel(&bg, x, y).unwrap_or(0),
                    frame[y * 320 + x],
                    "layers {enabled:03b} at ({x}, {y})"
                );
            }
        }
    }
}

#[test]
fn the_priority_bit_brings_a_cell_in_front_of_the_next_layer() {
    let mut tiles = BgTileBank::new();
    for tile in [1, 2] {
        for y in 0..8 {
            for x in 0..8 {
                tiles.set_pixel(tile, x, y, tile as u8);
            }
        }
    }
    let mut palettes = [0u8; BG_PALETTES_BYTES];
    palettes[1] = 101;
    palettes[2] = 102;
    let bg = BgVram::new(tiles, BgPalettes::from_bytes(&palettes).unwrap());
    let plain = |tile| TileEntry::new(tile, false, false, false, BgBank::ZERO).unwrap();
    let boosted = |tile| TileEntry::new(tile, false, false, true, BgBank::ZERO).unwrap();

    let mut low = TileLayer::empty(2, 1).unwrap();
    let mut high = TileLayer::empty(2, 1).unwrap();
    let empty = TileLayer::empty(2, 1).unwrap();
    // At power-on BG0 has priority 3, BG1 has 2 and BG2 has 1. BG0 holds
    // tile 1 and BG1 holds tile 2 in the first cell, so BG1 is in front. In
    // the second one BG0's tile has the priority bit, which lowers its
    // priority to 2, and then it wins the tie against BG1's plain tile.
    low.set(0, 0, plain(1));
    low.set(1, 0, boosted(1));
    high.set(0, 0, plain(2));
    high.set(1, 0, plain(2));
    let preview = Composer::new([&low, &high, &empty], [true; 3]);
    assert_eq!(
        preview.pixel(&bg, 0, 0),
        Some(102),
        "BG1 is in front of BG0"
    );
    assert_eq!(
        preview.pixel(&bg, 8, 0),
        Some(101),
        "the bit lifts the cell"
    );

    // Not even the bit lifts a cell over a layer two priorities in front.
    let front = Composer::new([&low, &empty, &high], [true; 3]);
    assert_eq!(front.pixel(&bg, 8, 0), Some(102), "BG2 stays in front");
}

#[test]
fn a_hidden_layer_shows_nothing_and_a_transparent_pixel_shows_the_layer_below() {
    let mut tiles = BgTileBank::new();
    tiles.set_pixel(1, 0, 0, 5);
    for x in 0..8 {
        tiles.set_pixel(2, x, 1, 6);
    }
    let mut palettes = [0u8; BG_PALETTES_BYTES];
    palettes[5] = 50;
    palettes[6] = 60;
    let bg = BgVram::new(tiles, BgPalettes::from_bytes(&palettes).unwrap());
    let plain = |tile| TileEntry::new(tile, false, false, false, BgBank::ZERO).unwrap();
    let mut front = TileLayer::empty(1, 1).unwrap();
    let mut back = TileLayer::empty(1, 1).unwrap();
    let empty = TileLayer::empty(1, 1).unwrap();
    front.set(0, 0, plain(1));
    back.set(0, 0, plain(2));
    let layers = [&back, &empty, &front];

    let both = Composer::new(layers, [true; 3]);
    assert_eq!(both.pixel(&bg, 0, 0), Some(50));
    assert_eq!(both.pixel(&bg, 1, 0), None, "transparent in both");
    assert_eq!(
        both.pixel(&bg, 3, 1),
        Some(60),
        "the back layer shows through"
    );

    let hidden = Composer::new(layers, [true, true, false]);
    assert_eq!(hidden.pixel(&bg, 0, 0), None);
    assert_eq!(hidden.pixel(&bg, 3, 1), Some(60));
}

#[test]
fn cells_off_the_map_hold_nothing() {
    let mut random = Lcg(3);
    let bg = graphics(&mut random);
    let map = layer(&mut random, 4, 3);
    let preview = Composer::new([&map, &map, &map], [true; 3]);
    assert_eq!(preview.pixel(&bg, 4 * 8, 0), None);
    assert_eq!(preview.pixel(&bg, 0, 3 * 8), None);
    assert_eq!(preview.pixel(&bg, usize::MAX / 2, 0), None);
}
