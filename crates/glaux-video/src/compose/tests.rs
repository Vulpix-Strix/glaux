use super::*;
use crate::draw::{ObjPaletteChoice, ObjSprite, draw_obj_sprite};
use glaux_machine::bg::BgVram;
use glaux_machine::framebuffer::Framebuffer;
use glaux_machine::memory_map::{
    BACKDROP_COLOR, BG_PRIORITY, BG_REGISTER_BLOCKS, BG_SCROLL_X, BG_SCROLL_Y, DISPLAY_ENABLE,
};
use glaux_machine::oam::OamEntry;
use glaux_machine::palette::Palette;
use glaux_machine::sprite::ObjVram;
use glaux_machine::tilemap::{BgBank, TileEntry};
use proptest::prelude::*;

const BACKDROP: u8 = 7;
const TILE_BYTES: usize = 32;

fn hardware() -> CartridgeHardware {
    let mut hardware =
        CartridgeHardware::new(Palette::default(), ObjVram::default(), BgVram::default());
    hardware.write(BACKDROP_COLOR, BACKDROP).unwrap();
    hardware
}

fn entry(tile: u16, flip_x: bool, flip_y: bool, boost: bool, bank: u8) -> TileEntry {
    TileEntry::new(tile, flip_x, flip_y, boost, BgBank::new(bank).unwrap()).unwrap()
}

fn plain(tile: u16) -> TileEntry {
    entry(tile, false, false, false, 0)
}

/// Makes every pixel of `tile` the tile-local index `local`.
fn solid(hardware: &mut CartridgeHardware, tile: usize, local: u8) {
    for y in 0..TILE_SIDE {
        for x in 0..TILE_SIDE {
            hardware.bg.tiles_mut().set_pixel(tile, x, y, local);
        }
    }
}

fn fill_layer(hardware: &mut CartridgeHardware, layer: BgLayer, entry: TileEntry) {
    for y in 0..TILE_WINDOW_SIDE {
        for x in 0..TILE_WINDOW_SIDE {
            hardware.tilemaps[layer.index()].set_entry(x, y, entry);
        }
    }
}

fn enable(hardware: &mut CartridgeHardware, bits: u8) {
    hardware.write(DISPLAY_ENABLE, bits).unwrap();
}

fn set_scroll(hardware: &mut CartridgeHardware, layer: BgLayer, x: u16, y: u16) {
    let block = BG_REGISTER_BLOCKS[layer.index()];
    hardware.write16(block + BG_SCROLL_X, x).unwrap();
    hardware.write16(block + BG_SCROLL_Y, y).unwrap();
}

fn set_priority(hardware: &mut CartridgeHardware, layer: BgLayer, priority: u8) {
    let block = BG_REGISTER_BLOCKS[layer.index()];
    hardware.write(block + BG_PRIORITY, priority).unwrap();
}

/// Clears to the backdrop as the system does, then composes.
fn compose(hardware: &mut CartridgeHardware) {
    hardware
        .framebuffer
        .clear(hardware.registers.backdrop_color());
    compose_layers(hardware);
}

fn pixel_at(hardware: &CartridgeHardware, x: usize, y: usize) -> u8 {
    hardware.framebuffer.as_bytes()[y * WIDTH + x]
}

/// Asserts the framebuffer equals `expected`, naming the first few
/// differences instead of printing 76 800 bytes.
fn assert_frame(hardware: &CartridgeHardware, expected: &[u8]) {
    let wrong: Vec<_> = hardware
        .framebuffer
        .as_bytes()
        .iter()
        .zip(expected)
        .enumerate()
        .filter(|(_, (actual, expected))| actual != expected)
        .map(|(i, (actual, expected))| {
            format!("({}, {}): {actual} != {expected}", i % WIDTH, i / WIDTH)
        })
        .collect();
    assert!(
        wrong.is_empty(),
        "{} pixels differ, first: {:?}",
        wrong.len(),
        &wrong[..wrong.len().min(5)]
    );
}

/// Width and height in pixels of each size code, as the specification lists
/// them. `None` is the reserved code.
fn object_pixels(size: u8) -> Option<(usize, usize)> {
    match size {
        0 => Some((8, 8)),
        1 => Some((16, 16)),
        2 => Some((32, 32)),
        3 => Some((8, 16)),
        4 => Some((16, 8)),
        5 => Some((16, 32)),
        6 => Some((32, 16)),
        _ => None,
    }
}

/// The master index of the object `entry` at screen position `(x, y)`, or
/// `None` where it is absent or transparent. The whole object is mirrored by
/// its flips, so the pixel is looked up in the mirrored rectangle, and its
/// tile is `first + column + 16 * row` unless that leaves the sheet row or
/// the bank.
fn object_pixel(hardware: &CartridgeHardware, entry: &OamEntry, x: usize, y: usize) -> Option<u8> {
    let (width, height) = object_pixels(entry.size)?;
    let local_x = i32::try_from(x).ok()? - i32::from(entry.x);
    let local_y = i32::try_from(y).ok()? - i32::from(entry.y);
    if !(0..width as i32).contains(&local_x) || !(0..height as i32).contains(&local_y) {
        return None;
    }
    let (mut local_x, mut local_y) = (local_x as usize, local_y as usize);
    if entry.flip_x {
        local_x = width - 1 - local_x;
    }
    if entry.flip_y {
        local_y = height - 1 - local_y;
    }
    let (column, row) = (local_x / 8, local_y / 8);
    let first = usize::from(entry.tile);
    if first % 16 + column >= 16 {
        return None;
    }
    let tile = first + column + 16 * row;
    if tile >= 1024 {
        return None;
    }
    let byte =
        hardware.obj.tiles().as_bytes()[tile * TILE_BYTES + (local_y % 8) * 4 + (local_x % 8) / 2];
    let local = if local_x % 2 == 0 {
        byte >> 4
    } else {
        byte & 0x0f
    };
    (local != 0).then(|| {
        hardware.obj.palettes().as_bytes()[usize::from(entry.bank.get()) * 16 + usize::from(local)]
    })
}

/// A direct restatement of the specification, one pixel at a time and
/// without sharing code with the compositor. Winners are chosen by comparing
/// `(effective priority, class, tie)`, where the class is 0 for a boosted
/// tile, 1 for an object and 2 for a normal tile. The tie is the OAM index
/// for objects and the layer's distance from BG2 for tiles.
fn reference(hardware: &CartridgeHardware) -> Vec<u8> {
    let registers = &hardware.registers;
    let tiles = hardware.bg.tiles().as_bytes();
    let palettes = hardware.bg.palettes().as_bytes();
    let objects: Vec<(usize, OamEntry)> = (0..128)
        .map(|index| (index, hardware.oam.entry(index).unwrap()))
        .filter(|(_, entry)| entry.enabled && registers.display_enable() & 0b1000 != 0)
        .collect();
    let mut frame = vec![registers.backdrop_color(); WIDTH * HEIGHT];
    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            let mut best: Option<((u8, u8, usize), u8)> = None;
            for layer in BgLayer::ALL {
                if registers.display_enable() & (1 << layer.index()) == 0 {
                    continue;
                }
                let map_x = (x + usize::from(registers.bg_scroll_x(layer))) % 512;
                let map_y = (y + usize::from(registers.bg_scroll_y(layer))) % 512;
                let cell = hardware.tilemaps[layer.index()]
                    .entry(map_x / 8, map_y / 8)
                    .unwrap();
                if cell.tile() == 0 {
                    continue;
                }
                let (mut px, mut py) = (map_x % 8, map_y % 8);
                if cell.flip_x() {
                    px = 7 - px;
                }
                if cell.flip_y() {
                    py = 7 - py;
                }
                let byte = tiles[usize::from(cell.tile()) * TILE_BYTES + py * 4 + px / 2];
                let local = if px % 2 == 0 { byte >> 4 } else { byte & 0x0f };
                if local == 0 {
                    continue;
                }
                let color = palettes[usize::from(cell.bank().get()) * 16 + usize::from(local)];
                let effective = registers
                    .bg_priority(layer)
                    .saturating_sub(u8::from(cell.priority()));
                let class = if cell.priority() { 0 } else { 2 };
                let key = (effective, class, 2 - layer.index());
                if best.is_none_or(|(best_key, _)| key < best_key) {
                    best = Some((key, color));
                }
            }
            for (index, entry) in &objects {
                if let Some(color) = object_pixel(hardware, entry, x, y) {
                    let key = (entry.priority, 1, *index);
                    if best.is_none_or(|(best_key, _)| key < best_key) {
                        best = Some((key, color));
                    }
                }
            }
            if let Some((_, color)) = best {
                frame[y * WIDTH + x] = color;
            }
        }
    }
    frame
}

/// Three full-screen layers in solid colors 1, 2 and 3 for BG0, BG1 and BG2,
/// with the given priorities.
fn solid_layers(priorities: [u8; 3]) -> CartridgeHardware {
    let mut hardware = hardware();
    for layer in BgLayer::ALL {
        let tile = layer.index() + 1;
        solid(&mut hardware, tile, 1);
        let bank = layer.index() as u8 + 1;
        let color = layer.index() as u8 + 1;
        hardware
            .bg
            .palettes_mut()
            .set(BgBank::new(bank).unwrap(), 1, color);
        fill_layer(
            &mut hardware,
            layer,
            entry(tile as u16, false, false, false, bank),
        );
        set_priority(&mut hardware, layer, priorities[layer.index()]);
    }
    enable(&mut hardware, 0b0111);
    hardware
}

#[test]
fn nothing_is_drawn_while_the_background_bits_are_clear() {
    let mut hardware = solid_layers([0, 1, 2]);
    enable(&mut hardware, 0b1000);
    compose(&mut hardware);
    assert_frame(&hardware, &[BACKDROP; WIDTH * HEIGHT]);
}

#[test]
fn each_layer_has_its_own_enable_bit() {
    for layer in BgLayer::ALL {
        let mut hardware = solid_layers([0, 1, 2]);
        enable(&mut hardware, 1 << layer.index());
        compose(&mut hardware);
        assert_eq!(pixel_at(&hardware, 10, 10), layer.index() as u8 + 1);
        assert_frame(&hardware, &reference(&hardware));
    }
}

#[test]
fn tile_zero_is_transparent_whatever_its_entry_says() {
    let mut hardware = hardware();
    solid(&mut hardware, 0, 5);
    fill_layer(&mut hardware, BgLayer::Bg0, entry(0, true, true, true, 3));
    enable(&mut hardware, 0b001);
    compose(&mut hardware);
    assert_frame(&hardware, &[BACKDROP; WIDTH * HEIGHT]);
}

#[test]
fn local_index_zero_shows_what_is_behind() {
    let mut hardware = hardware();
    // Tile 1 is opaque on its left half only.
    for y in 0..8 {
        for x in 0..4 {
            hardware.bg.tiles_mut().set_pixel(1, x, y, 4);
        }
    }
    solid(&mut hardware, 2, 9);
    fill_layer(&mut hardware, BgLayer::Bg0, plain(1));
    fill_layer(&mut hardware, BgLayer::Bg1, plain(2));
    set_priority(&mut hardware, BgLayer::Bg0, 0);
    enable(&mut hardware, 0b011);
    compose(&mut hardware);
    assert_eq!(pixel_at(&hardware, 3, 0), 4);
    assert_eq!(pixel_at(&hardware, 4, 0), 9);
    assert_eq!(pixel_at(&hardware, 12, 7), 9);
}

#[test]
fn a_non_zero_local_index_mapped_to_master_zero_is_opaque() {
    let mut hardware = hardware();
    solid(&mut hardware, 1, 1);
    solid(&mut hardware, 2, 2);
    // BG0 local 1 resolves to master 0. BG1 local 2 resolves to master 2.
    hardware.bg.palettes_mut().set(BgBank::ZERO, 1, 0);
    fill_layer(&mut hardware, BgLayer::Bg0, plain(1));
    fill_layer(&mut hardware, BgLayer::Bg1, plain(2));
    set_priority(&mut hardware, BgLayer::Bg0, 0);
    set_priority(&mut hardware, BgLayer::Bg1, 3);
    enable(&mut hardware, 0b011);
    compose(&mut hardware);
    // Master 0 covers both the backdrop (7) and the layer behind (2).
    assert_eq!(pixel_at(&hardware, 0, 0), 0);
    assert_eq!(pixel_at(&hardware, 200, 100), 0);

    // The transparent local index stays transparent although it is the one
    // that would resolve to 0.
    solid(&mut hardware, 1, 0);
    compose(&mut hardware);
    assert_eq!(pixel_at(&hardware, 0, 0), 2);
}

#[test]
fn each_palette_bank_resolves_its_own_entries() {
    let mut hardware = hardware();
    solid(&mut hardware, 1, 6);
    for bank in 0..4u8 {
        hardware
            .bg
            .palettes_mut()
            .set(BgBank::new(bank).unwrap(), 6, 100 + bank);
        hardware.tilemaps[0].set_entry(usize::from(bank), 0, entry(1, false, false, false, bank));
    }
    enable(&mut hardware, 0b001);
    compose(&mut hardware);
    for bank in 0..4usize {
        assert_eq!(pixel_at(&hardware, bank * 8 + 3, 2), 100 + bank as u8);
    }
}

/// Local indices that differ between most mirrored positions, so a mirroring
/// mistake shows.
fn pattern(x: usize, y: usize) -> u8 {
    ((x * 3 + y * 5) % 15) as u8 + 1
}

#[test]
fn flips_mirror_the_tile_on_their_own_axis() {
    for (flip_x, flip_y) in [(false, false), (true, false), (false, true), (true, true)] {
        let mut hardware = hardware();
        for y in 0..8 {
            for x in 0..8 {
                hardware.bg.tiles_mut().set_pixel(1, x, y, pattern(x, y));
            }
        }
        hardware.tilemaps[0].set_entry(0, 0, entry(1, flip_x, flip_y, false, 0));
        enable(&mut hardware, 0b001);
        compose(&mut hardware);
        for y in 0..8 {
            for x in 0..8 {
                let source_x = if flip_x { 7 - x } else { x };
                let source_y = if flip_y { 7 - y } else { y };
                // Bank 0 is linear, so local index and master index agree.
                assert_eq!(
                    pixel_at(&hardware, x, y),
                    pattern(source_x, source_y),
                    "flip x {flip_x}, flip y {flip_y}, at ({x}, {y})"
                );
            }
        }
    }
}

/// Every cell holds a different tile, each with its own local index.
fn varied_map(hardware: &mut CartridgeHardware, layer: BgLayer) {
    for tile in 1..16usize {
        solid(hardware, tile, tile as u8);
    }
    for y in 0..TILE_WINDOW_SIDE {
        for x in 0..TILE_WINDOW_SIDE {
            let tile = ((x * 5 + y * 3) % 15 + 1) as u16;
            hardware.tilemaps[layer.index()].set_entry(x, y, plain(tile));
        }
    }
}

#[test]
fn scroll_wraps_every_512_pixels_and_keeps_the_register() {
    let mut hardware = hardware();
    varied_map(&mut hardware, BgLayer::Bg0);
    enable(&mut hardware, 0b001);

    set_scroll(&mut hardware, BgLayer::Bg0, 88, 88);
    compose(&mut hardware);
    let wrapped = hardware.framebuffer.clone();

    set_scroll(&mut hardware, BgLayer::Bg0, 600, 600);
    compose(&mut hardware);
    assert_eq!(hardware.framebuffer, wrapped);
    assert_eq!(hardware.registers.bg_scroll_x(BgLayer::Bg0), 600);
    assert_eq!(hardware.registers.bg_scroll_y(BgLayer::Bg0), 600);
}

#[test]
fn scrolled_maps_match_the_reference_across_the_wrap() {
    for (x, y) in [(1, 0), (7, 5), (504, 500), (511, 511), (65535, 65535)] {
        let mut hardware = hardware();
        varied_map(&mut hardware, BgLayer::Bg0);
        enable(&mut hardware, 0b001);
        set_scroll(&mut hardware, BgLayer::Bg0, x, y);
        compose(&mut hardware);
        assert_frame(&hardware, &reference(&hardware));
    }
}

#[test]
fn the_lower_priority_number_is_in_front() {
    let hardware = &mut solid_layers([0, 1, 2]);
    compose(hardware);
    assert_eq!(pixel_at(hardware, 0, 0), 1);

    let hardware = &mut solid_layers([3, 0, 2]);
    compose(hardware);
    assert_eq!(pixel_at(hardware, 0, 0), 2);

    let hardware = &mut solid_layers([3, 2, 1]);
    compose(hardware);
    assert_eq!(pixel_at(hardware, 0, 0), 3);
}

#[test]
fn equal_priorities_favour_the_higher_layer_number() {
    let mut hardware = solid_layers([1, 1, 1]);
    compose(&mut hardware);
    assert_eq!(pixel_at(&hardware, 0, 0), 3);
    enable(&mut hardware, 0b011);
    compose(&mut hardware);
    assert_eq!(pixel_at(&hardware, 0, 0), 2);
    enable(&mut hardware, 0b001);
    compose(&mut hardware);
    assert_eq!(pixel_at(&hardware, 0, 0), 1);
}

fn boost(hardware: &mut CartridgeHardware, layer: BgLayer) {
    let tile = layer.index() as u16 + 1;
    let bank = layer.index() as u8 + 1;
    fill_layer(hardware, layer, entry(tile, false, false, true, bank));
}

#[test]
fn boost_moves_a_tile_one_level_forward() {
    // BG0 at priority 2 boosted ties BG1 at priority 1, and the boost wins.
    let mut hardware = solid_layers([2, 1, 3]);
    boost(&mut hardware, BgLayer::Bg0);
    compose(&mut hardware);
    assert_eq!(pixel_at(&hardware, 0, 0), 1);

    // One level only: priority 3 boosted is still behind priority 1.
    let mut hardware = solid_layers([3, 1, 3]);
    boost(&mut hardware, BgLayer::Bg0);
    compose(&mut hardware);
    assert_eq!(pixel_at(&hardware, 0, 0), 2);
}

#[test]
fn boost_wins_ties_against_higher_layers() {
    // BG2 normally beats BG0 at the same priority, but not a boosted BG0
    // whose effective priority matches.
    let mut hardware = solid_layers([1, 3, 1]);
    compose(&mut hardware);
    assert_eq!(pixel_at(&hardware, 0, 0), 3);
    let mut hardware = solid_layers([2, 3, 1]);
    boost(&mut hardware, BgLayer::Bg0);
    compose(&mut hardware);
    assert_eq!(pixel_at(&hardware, 0, 0), 1);
}

#[test]
fn boost_saturates_at_priority_zero() {
    // Both reach effective priority 0, the boosted one wins the tie.
    let mut hardware = solid_layers([0, 3, 0]);
    boost(&mut hardware, BgLayer::Bg0);
    compose(&mut hardware);
    assert_eq!(pixel_at(&hardware, 0, 0), 1);

    // Boosted BG0 and BG2 at priority 0: BG2 wins.
    boost(&mut hardware, BgLayer::Bg2);
    compose(&mut hardware);
    assert_eq!(pixel_at(&hardware, 0, 0), 3);
}

#[test]
fn depths_order_boost_before_objects_before_normal_tiles() {
    let boosted: Vec<u8> = [BgLayer::Bg2, BgLayer::Bg1, BgLayer::Bg0]
        .map(|layer| background_depth(0, true, layer))
        .to_vec();
    let normal: Vec<u8> = [BgLayer::Bg2, BgLayer::Bg1, BgLayer::Bg0]
        .map(|layer| background_depth(0, false, layer))
        .to_vec();
    assert!(boosted.is_sorted_by(|a, b| a < b));
    assert!(normal.is_sorted_by(|a, b| a < b));
    assert!(boosted[2] < OBJECT_RANK && OBJECT_RANK < normal[0]);
    // Boost at priority 1 lands on level 0 and sorts with the boosted tiles
    // there, ahead of objects and of normal tiles at priority 0.
    assert!(background_depth(1, true, BgLayer::Bg0) < OBJECT_RANK);
    // Every rank of a level sorts ahead of the next level.
    assert!(normal[2] < background_depth(1, false, BgLayer::Bg2));
    assert!(background_depth(1, true, BgLayer::Bg2) < background_depth(1, false, BgLayer::Bg2));
}

fn object_bank(bank: u8) -> ObjBank {
    ObjBank::new(bank).unwrap()
}

fn set_object(hardware: &mut CartridgeHardware, index: usize, entry: OamEntry) {
    assert!(hardware.oam.set_entry(index, entry));
}

/// An enabled object of the given size code with everything else zero.
fn object_at(x: i16, y: i16, tile: u16, size: u8) -> OamEntry {
    OamEntry {
        x,
        y,
        tile,
        size,
        enabled: true,
        ..OamEntry::default()
    }
}

fn set_object_tile(hardware: &mut CartridgeHardware, tile: usize, local: u8) {
    for y in 0..TILE_SIDE {
        for x in 0..TILE_SIDE {
            hardware.obj.tiles_mut().set_pixel(tile, x, y, local);
        }
    }
}

fn fill_object_tiles(hardware: &mut CartridgeHardware, local: u8) {
    for tile in 0..1024 {
        set_object_tile(hardware, tile, local);
    }
}

/// How many pixels differ from the backdrop.
fn drawn(hardware: &CartridgeHardware) -> usize {
    hardware
        .framebuffer
        .as_bytes()
        .iter()
        .filter(|&&index| index != BACKDROP)
        .count()
}

#[test]
fn every_size_code_covers_its_rectangle() {
    for size in 0..=6 {
        let mut hardware = hardware();
        fill_object_tiles(&mut hardware, 3);
        set_object(&mut hardware, 0, object_at(10, 20, 17, size));
        enable(&mut hardware, 0b1000);
        compose(&mut hardware);

        let (width, height) = object_pixels(size).unwrap();
        assert_eq!(drawn(&hardware), width * height, "size {size}");
        assert_eq!(pixel_at(&hardware, 10, 20), 3, "size {size}");
        assert_eq!(
            pixel_at(&hardware, 9 + width, 19 + height),
            3,
            "size {size}"
        );
        assert_eq!(pixel_at(&hardware, 10 + width, 20), BACKDROP, "size {size}");
        assert_eq!(
            pixel_at(&hardware, 10, 20 + height),
            BACKDROP,
            "size {size}"
        );
    }
}

#[test]
fn reserved_size_and_disabled_objects_draw_nothing() {
    for (size, enabled) in [(7, true), (7, false), (0, false)] {
        let mut hardware = hardware();
        fill_object_tiles(&mut hardware, 3);
        let entry = OamEntry {
            enabled,
            ..object_at(10, 10, 0, size)
        };
        set_object(&mut hardware, 0, entry);
        enable(&mut hardware, 0b1000);
        compose(&mut hardware);
        assert_eq!(drawn(&hardware), 0, "size {size}, enabled {enabled}");
    }
}

#[test]
fn the_object_bit_gates_objects_and_leaves_the_layers_alone() {
    let mut hardware = hardware();
    solid(&mut hardware, 1, 1);
    fill_layer(&mut hardware, BgLayer::Bg0, plain(1));
    fill_object_tiles(&mut hardware, 9);
    set_priority(&mut hardware, BgLayer::Bg0, 3);
    set_object(&mut hardware, 0, object_at(0, 0, 0, 0));

    enable(&mut hardware, 0b0001);
    compose(&mut hardware);
    assert_eq!(pixel_at(&hardware, 0, 0), 1);

    enable(&mut hardware, 0b1001);
    compose(&mut hardware);
    assert_eq!(pixel_at(&hardware, 0, 0), 9);
    assert_eq!(pixel_at(&hardware, 8, 0), 1);

    enable(&mut hardware, 0b1000);
    compose(&mut hardware);
    assert_eq!(pixel_at(&hardware, 0, 0), 9);
    assert_eq!(pixel_at(&hardware, 8, 0), BACKDROP);
}

#[test]
fn objects_clip_at_every_edge_and_never_wrap() {
    // (x, y, size code, pixels that show)
    let cases = [
        (-4, -4, 0, 16),
        (316, 236, 0, 16),
        (-8, 0, 0, 0),
        (320, 0, 0, 0),
        (0, 240, 0, 0),
        (0, -8, 0, 0),
        (-31, 10, 2, 32),
        (319, 10, 2, 32),
        (i16::MAX, i16::MAX, 2, 0),
        (i16::MIN, 0, 2, 0),
        (i16::MIN, i16::MIN, 2, 0),
        (0, i16::MAX, 0, 0),
        (i16::MAX, 0, 0, 0),
    ];
    for (x, y, size, shown) in cases {
        let mut hardware = hardware();
        fill_object_tiles(&mut hardware, 3);
        set_object(&mut hardware, 0, object_at(x, y, 0, size));
        enable(&mut hardware, 0b1000);
        compose(&mut hardware);
        assert_eq!(drawn(&hardware), shown, "at ({x}, {y}), size {size}");
    }
}

#[test]
fn a_multi_tile_object_reads_the_sheet_row_by_row() {
    let mut hardware = hardware();
    for (tile, local) in [(20, 1), (21, 2), (36, 3), (37, 4)] {
        set_object_tile(&mut hardware, tile, local);
    }
    set_object(&mut hardware, 0, object_at(0, 0, 20, 1));
    enable(&mut hardware, 0b1000);
    compose(&mut hardware);
    assert_eq!(pixel_at(&hardware, 0, 0), 1);
    assert_eq!(pixel_at(&hardware, 8, 0), 2);
    assert_eq!(pixel_at(&hardware, 0, 8), 3);
    assert_eq!(pixel_at(&hardware, 8, 8), 4);
}

#[test]
fn a_row_does_not_continue_on_the_next_sheet_row() {
    let mut hardware = hardware();
    // Tile 16 is where a wrapped second column would land.
    for (tile, local) in [(15, 1), (16, 2), (31, 3), (32, 4)] {
        set_object_tile(&mut hardware, tile, local);
    }
    set_object(&mut hardware, 0, object_at(0, 0, 15, 1));
    enable(&mut hardware, 0b1000);
    compose(&mut hardware);
    assert_eq!(pixel_at(&hardware, 0, 0), 1);
    assert_eq!(pixel_at(&hardware, 8, 0), BACKDROP);
    assert_eq!(pixel_at(&hardware, 0, 8), 3);
    assert_eq!(pixel_at(&hardware, 8, 8), BACKDROP);
}

#[test]
fn tiles_past_the_bank_are_transparent() {
    let mut hardware = hardware();
    set_object_tile(&mut hardware, 1023, 1);
    set_object(&mut hardware, 0, object_at(0, 0, 1023, 3));
    enable(&mut hardware, 0b1000);
    compose(&mut hardware);
    assert_eq!(pixel_at(&hardware, 0, 0), 1);
    assert_eq!(pixel_at(&hardware, 0, 8), BACKDROP);
}

/// A different local index in nearly every pixel of the 16x16 object, so a
/// mirroring mistake shows.
fn logical(x: usize, y: usize) -> u8 {
    ((x * 3 + y * 5 + x / 8 + 2 * (y / 8)) % 15) as u8 + 1
}

#[test]
fn flips_mirror_the_whole_object() {
    for (flip_x, flip_y) in [(false, false), (true, false), (false, true), (true, true)] {
        let mut hardware = hardware();
        for (quadrant, tile) in [(0, 20), (1, 21), (2, 36), (3, 37)] {
            for y in 0..8 {
                for x in 0..8 {
                    let local = logical(quadrant % 2 * 8 + x, quadrant / 2 * 8 + y);
                    hardware.obj.tiles_mut().set_pixel(tile, x, y, local);
                }
            }
        }
        let entry = OamEntry {
            flip_x,
            flip_y,
            ..object_at(30, 40, 20, 1)
        };
        set_object(&mut hardware, 0, entry);
        enable(&mut hardware, 0b1000);
        compose(&mut hardware);
        for y in 0..16 {
            for x in 0..16 {
                let source_x = if flip_x { 15 - x } else { x };
                let source_y = if flip_y { 15 - y } else { y };
                assert_eq!(
                    pixel_at(&hardware, 30 + x, 40 + y),
                    logical(source_x, source_y),
                    "flip x {flip_x}, flip y {flip_y}, at ({x}, {y})"
                );
            }
        }
    }
}

#[test]
fn objects_look_like_the_same_block_drawn_by_spr() {
    for size in 0..=6 {
        for (flip_x, flip_y) in [(false, false), (true, false), (false, true), (true, true)] {
            let mut hardware = hardware();
            for tile in 0..1024 {
                for y in 0..8 {
                    for x in 0..8 {
                        let local = ((tile * 7 + x * 3 + y * 5) % 16) as u8;
                        hardware.obj.tiles_mut().set_pixel(tile, x, y, local);
                    }
                }
            }
            for local in 1..16u8 {
                hardware
                    .obj
                    .palettes_mut()
                    .set(object_bank(2), local, 100 + local);
            }
            let entry = OamEntry {
                flip_x,
                flip_y,
                bank: object_bank(2),
                ..object_at(40, 30, 18, size)
            };
            set_object(&mut hardware, 0, entry);
            enable(&mut hardware, 0b1000);
            compose(&mut hardware);

            let (width, height) = object_pixels(size).unwrap();
            let mut expected = Framebuffer::new();
            expected.clear(BACKDROP);
            draw_obj_sprite(
                &mut expected,
                &hardware.obj,
                &ObjSprite {
                    first: 18,
                    x: 40,
                    y: 30,
                    width: width / 8,
                    height: height / 8,
                    flip_x,
                    flip_y,
                    palette: ObjPaletteChoice::Bank(object_bank(2)),
                },
            );
            assert_frame(&hardware, expected.as_bytes());
        }
    }
}

#[test]
fn the_palette_bank_comes_from_the_entry_not_from_the_tile_attributes() {
    let mut hardware = hardware();
    fill_object_tiles(&mut hardware, 1);
    for tile in 0..1024 {
        hardware.obj.attributes_mut().set_bank(tile, object_bank(5));
    }
    hardware.obj.palettes_mut().set(object_bank(3), 1, 50);
    hardware.obj.palettes_mut().set(object_bank(5), 1, 60);
    let entry = OamEntry {
        bank: object_bank(3),
        ..object_at(0, 0, 0, 1)
    };
    set_object(&mut hardware, 0, entry);
    enable(&mut hardware, 0b1000);
    compose(&mut hardware);
    assert_eq!(pixel_at(&hardware, 0, 0), 50);
    assert_eq!(pixel_at(&hardware, 15, 15), 50);
}

#[test]
fn object_local_zero_is_transparent_and_a_remap_to_master_zero_is_not() {
    let mut hardware = hardware();
    set_object_tile(&mut hardware, 0, 0);
    set_object_tile(&mut hardware, 1, 2);
    hardware.obj.palettes_mut().set(object_bank(2), 2, 0);
    let entry = OamEntry {
        bank: object_bank(2),
        ..object_at(0, 0, 0, 4)
    };
    set_object(&mut hardware, 0, entry);
    enable(&mut hardware, 0b1000);
    compose(&mut hardware);
    assert_eq!(
        pixel_at(&hardware, 0, 0),
        BACKDROP,
        "local 0 shows the backdrop"
    );
    assert_eq!(pixel_at(&hardware, 8, 0), 0, "master 0 is a color");
}

const BG_COLOR: u8 = 1;
const OBJECT_COLOR: u8 = 9;

/// The color at the top left of a full-screen BG0 and an object over it.
fn object_over_layer(bg_priority: u8, boosted: bool, object_priority: u8) -> u8 {
    let mut hardware = hardware();
    solid(&mut hardware, 1, BG_COLOR);
    fill_layer(
        &mut hardware,
        BgLayer::Bg0,
        entry(1, false, false, boosted, 0),
    );
    set_priority(&mut hardware, BgLayer::Bg0, bg_priority);
    fill_object_tiles(&mut hardware, OBJECT_COLOR);
    let entry = OamEntry {
        priority: object_priority,
        ..object_at(0, 0, 0, 0)
    };
    set_object(&mut hardware, 0, entry);
    enable(&mut hardware, 0b1001);
    compose(&mut hardware);
    pixel_at(&hardware, 0, 0)
}

#[test]
fn an_object_beats_a_normal_tile_of_the_same_priority() {
    assert_eq!(object_over_layer(1, false, 1), OBJECT_COLOR);
    assert_eq!(object_over_layer(0, false, 0), OBJECT_COLOR);
    assert_eq!(object_over_layer(3, false, 3), OBJECT_COLOR);
}

#[test]
fn a_boosted_tile_beats_an_object_of_the_same_effective_priority() {
    assert_eq!(object_over_layer(2, true, 1), BG_COLOR);
    assert_eq!(object_over_layer(3, true, 2), BG_COLOR);
    assert_eq!(object_over_layer(1, true, 0), BG_COLOR);
}

#[test]
fn priority_decides_between_objects_and_tiles_before_any_tie() {
    assert_eq!(object_over_layer(2, false, 1), OBJECT_COLOR);
    assert_eq!(object_over_layer(1, false, 2), BG_COLOR);
    assert_eq!(object_over_layer(1, true, 3), BG_COLOR);
    assert_eq!(object_over_layer(3, false, 0), OBJECT_COLOR);
}

#[test]
fn objects_of_equal_priority_favour_the_lower_index() {
    let mut hardware = hardware();
    fill_object_tiles(&mut hardware, 1);
    hardware.obj.palettes_mut().set(object_bank(1), 1, 21);
    hardware.obj.palettes_mut().set(object_bank(2), 1, 22);
    // Index 5 sits at the top left and index 2 overlaps its lower right
    // corner.
    let high = OamEntry {
        bank: object_bank(2),
        priority: 1,
        ..object_at(0, 0, 0, 0)
    };
    let low = OamEntry {
        bank: object_bank(1),
        priority: 1,
        ..object_at(4, 4, 0, 0)
    };
    set_object(&mut hardware, 5, high);
    set_object(&mut hardware, 2, low);
    enable(&mut hardware, 0b1000);
    compose(&mut hardware);
    assert_eq!(pixel_at(&hardware, 1, 1), 22);
    assert_eq!(
        pixel_at(&hardware, 6, 6),
        21,
        "the lower index wins the overlap"
    );
    assert_eq!(pixel_at(&hardware, 10, 10), 21);

    // A smaller priority number beats a smaller index.
    let in_front = OamEntry {
        priority: 0,
        ..high
    };
    set_object(&mut hardware, 5, in_front);
    compose(&mut hardware);
    assert_eq!(pixel_at(&hardware, 6, 6), 22);
}

#[test]
fn object_depths_sit_between_boosted_and_normal_tiles_of_their_priority() {
    for priority in 0..4 {
        assert!(background_depth(priority, true, BgLayer::Bg0) < object_depth(priority));
        assert!(background_depth(priority, false, BgLayer::Bg2) > object_depth(priority));
        assert!(object_depth(priority) < object_depth(priority + 1));
    }
    // A boosted tile at priority 1 has effective priority 0, so it sorts
    // ahead of an object of priority 0.
    assert!(background_depth(1, true, BgLayer::Bg0) < object_depth(0));
    assert_eq!(object_depth(2), 2 * 8 + OBJECT_RANK);
}

#[test]
fn a_full_table_of_stacked_objects_matches_the_reference() {
    let mut hardware = hardware();
    for tile in 0..1024 {
        for y in 0..8 {
            for x in 0..8 {
                let local = ((tile * 7 + x * 3 + y * 5) % 16) as u8;
                hardware.obj.tiles_mut().set_pixel(tile, x, y, local);
            }
        }
    }
    for bank in 0..8u8 {
        for local in 1..16u8 {
            hardware
                .obj
                .palettes_mut()
                .set(object_bank(bank), local, 100 + bank * 10 + local);
        }
    }
    for index in 0..128usize {
        let entry = OamEntry {
            flip_x: index % 3 == 0,
            flip_y: index % 5 == 0,
            bank: object_bank((index % 8) as u8),
            priority: (index * 5 % 4) as u8,
            ..object_at(
                100 + (index % 5) as i16,
                80 + (index % 3) as i16,
                (index * 3 % 900) as u16,
                (index % 7) as u8,
            )
        };
        set_object(&mut hardware, index, entry);
    }
    enable(&mut hardware, 0b1000);
    compose(&mut hardware);
    assert_frame(&hardware, &reference(&hardware));
}

/// Fills the hardware from `seed` with a small xorshift generator.
fn randomized(
    seed: u64,
    enable_bits: u8,
    scrolls: [(u16, u16); 3],
    priorities: [u8; 3],
) -> CartridgeHardware {
    let mut state = seed | 1;
    let mut next = move || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state
    };
    let mut hardware = hardware();
    for tile in 0..1024 {
        for y in 0..8 {
            for x in 0..8 {
                let sparse = (next() & next() & 0x0f) as u8;
                hardware.bg.tiles_mut().set_pixel(tile, x, y, sparse);
            }
        }
    }
    for bank in 0..4u8 {
        for local in 1..16 {
            let master = next() as u8;
            hardware
                .bg
                .palettes_mut()
                .set(BgBank::new(bank).unwrap(), local, master);
        }
    }
    for layer in BgLayer::ALL {
        for y in 0..TILE_WINDOW_SIDE {
            for x in 0..TILE_WINDOW_SIDE {
                let raw = (next() as u16) & TileEntry::VALID_BITS;
                let tile = if next() % 4 == 0 { 0 } else { raw };
                hardware.tilemaps[layer.index()].set_entry(
                    x,
                    y,
                    TileEntry::from_raw(tile).unwrap(),
                );
            }
        }
        let (x, y) = scrolls[layer.index()];
        set_scroll(&mut hardware, layer, x, y);
        set_priority(&mut hardware, layer, priorities[layer.index()]);
    }
    for tile in 0..1024 {
        for y in 0..8 {
            for x in 0..8 {
                let sparse = (next() & next() & 0x0f) as u8;
                hardware.obj.tiles_mut().set_pixel(tile, x, y, sparse);
            }
        }
    }
    for bank in 0..8u8 {
        for local in 1..16 {
            let master = next() as u8;
            hardware
                .obj
                .palettes_mut()
                .set(object_bank(bank), local, master);
        }
    }
    for index in 0..128 {
        let x = match next() % 16 {
            0 => i16::MIN,
            1 => i16::MAX,
            _ => (next() % 440) as i16 - 60,
        };
        let y = match next() % 16 {
            0 => i16::MIN,
            1 => i16::MAX,
            _ => (next() % 320) as i16 - 40,
        };
        let entry = OamEntry {
            x,
            y,
            tile: (next() % 1024) as u16,
            bank: object_bank((next() % 8) as u8),
            flip_x: next() % 2 == 0,
            flip_y: next() % 2 == 0,
            priority: (next() % 4) as u8,
            size: (next() % 8) as u8,
            enabled: next() % 4 != 0,
        };
        set_object(&mut hardware, index, entry);
    }
    enable(&mut hardware, enable_bits);
    hardware
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(24))]

    #[test]
    fn composition_matches_the_reference(
        seed in any::<u64>(),
        enable_bits in 0u8..16,
        scrolls in proptest::array::uniform3((any::<u16>(), any::<u16>())),
        priorities in proptest::array::uniform3(0u8..4),
    ) {
        let mut hardware = randomized(seed, enable_bits, scrolls, priorities);
        compose(&mut hardware);
        assert_frame(&hardware, &reference(&hardware));
    }
}
