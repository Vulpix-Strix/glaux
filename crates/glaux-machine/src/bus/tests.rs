use std::sync::OnceLock;

use super::*;
use crate::bg::BgVram;
use crate::memory_map::WORK_RAM_LEN;
use crate::obj_palette::ObjBank;
use crate::palette::{Palette, Rgb};
use crate::sprite::ObjVram;
use crate::tilemap::BgBank;
use pretty_assertions::assert_eq;
use proptest::prelude::*;

fn hardware() -> CartridgeHardware {
    CartridgeHardware::new(Palette::default(), ObjVram::default(), BgVram::default())
}

/// Hardware whose mapped bytes all hold distinctive values, set through the
/// typed accessors and not through the bus.
fn filled() -> CartridgeHardware {
    let mut hardware = hardware();
    for (index, byte) in hardware.work_ram.iter_mut().enumerate() {
        *byte = (index % 251) as u8 + 1;
    }
    for (index, byte) in hardware.framebuffer.as_bytes_mut().iter_mut().enumerate() {
        *byte = (index % 241) as u8 + 3;
    }
    for index in 0..BG_TILES.len as usize {
        hardware
            .bg
            .tiles_mut()
            .set_byte(index, (index % 239) as u8 + 5);
    }
    for index in 0..OBJ_TILES.len as usize {
        hardware
            .obj
            .tiles_mut()
            .set_byte(index, (index % 233) as u8 + 7);
    }
    for index in 0..MASTER_PALETTE.len as usize {
        hardware.palette.set_byte(index, (index % 229) as u8 + 11);
    }
    for index in 0..BG_PALETTES.len as usize {
        hardware
            .bg
            .palettes_mut()
            .set_byte(index, (index * 7 + 1) as u8);
    }
    for index in 0..OBJ_PALETTES.len as usize {
        hardware
            .obj
            .palettes_mut()
            .set_byte(index, (index * 5 + 2) as u8);
    }
    for index in 0..OBJ_ATTRIBUTES.len as usize {
        hardware
            .obj
            .attributes_mut()
            .set_byte(index, (index % 7 + 1) as u8);
    }
    hardware
}

/// A shared copy of [`filled`], built once because it is large.
fn filled_copy() -> CartridgeHardware {
    static FILLED: OnceLock<CartridgeHardware> = OnceLock::new();
    FILLED.get_or_init(filled).clone()
}

/// The bytes of the typed storage behind each region.
fn device_bytes(hardware: &CartridgeHardware) -> [(Region, Vec<u8>); 8] {
    [
        (WORK_RAM, hardware.work_ram.to_vec()),
        (FRAMEBUFFER, hardware.framebuffer.as_bytes().to_vec()),
        (BG_TILES, hardware.bg.tiles().as_bytes().to_vec()),
        (OBJ_TILES, hardware.obj.tiles().as_bytes().to_vec()),
        (MASTER_PALETTE, hardware.palette.to_rgb888().to_vec()),
        (BG_PALETTES, hardware.bg.palettes().as_bytes().to_vec()),
        (OBJ_PALETTES, hardware.obj.palettes().as_bytes().to_vec()),
        (
            OBJ_ATTRIBUTES,
            hardware.obj.attributes().as_bytes().to_vec(),
        ),
    ]
}

fn assert_unchanged(hardware: &CartridgeHardware, before: &CartridgeHardware) {
    // A failed comparison would print every byte, so only the result is shown.
    assert!(hardware == before, "the hardware was modified");
}

#[test]
fn the_regions_match_the_specified_map() {
    let expected = [
        (WORK_RAM, 0x00000, 0x0FFFF),
        (FRAMEBUFFER, 0x10000, 0x22BFF),
        (BG_TILES, 0x24000, 0x2BFFF),
        (OBJ_TILES, 0x2C000, 0x33FFF),
        (MASTER_PALETTE, 0x3A000, 0x3A2FF),
        (BG_PALETTES, 0x3A300, 0x3A33F),
        (OBJ_PALETTES, 0x3A340, 0x3A3BF),
        (OBJ_ATTRIBUTES, 0x3A400, 0x3A7FF),
    ];
    for (region, start, last) in expected {
        assert_eq!((region.start, region.last()), (start, last));
    }
    assert_eq!(WORK_RAM_LEN, 0x10000);
}

#[test]
fn work_ram_and_the_work_framebuffer_start_zeroed() {
    let hardware = hardware();
    assert!(hardware.work_ram.iter().all(|&byte| byte == 0));
    assert!(
        hardware
            .framebuffer
            .as_bytes()
            .iter()
            .all(|&byte| byte == 0)
    );
}

#[test]
fn every_mapped_address_reads_the_byte_of_its_device() {
    let hardware = filled();
    for (region, bytes) in device_bytes(&hardware) {
        assert_eq!(bytes.len(), region.len as usize);
        for (offset, &byte) in bytes.iter().enumerate() {
            assert_eq!(hardware.read(region.start + offset as u32), Ok(byte));
        }
    }
}

#[test]
fn writes_land_in_the_typed_storage_at_once() {
    let mut hardware = hardware();
    hardware.write(0x00123, 0x11).unwrap();
    hardware.write(0x10000, 0x22).unwrap();
    hardware.write(0x22BFF, 0x33).unwrap();
    hardware.write(0x24000, 0x12).unwrap();
    hardware.write(0x2C020, 0x34).unwrap();
    hardware.write(0x3A010, 0x9A).unwrap();
    hardware.write(0x3A301 + 16, 99).unwrap();
    hardware.write(0x3A340 + 17, 98).unwrap();
    hardware.write(0x3A400 + 2, 0x05).unwrap();

    assert_eq!(hardware.work_ram[0x123], 0x11);
    assert_eq!(hardware.framebuffer.as_bytes()[0], 0x22);
    assert_eq!(hardware.framebuffer.as_bytes()[framebuffer_last()], 0x33);
    assert_eq!(hardware.bg.tiles().pixel(0, 0, 0), Some(1));
    assert_eq!(hardware.bg.tiles().pixel(0, 1, 0), Some(2));
    assert_eq!(hardware.obj.tiles().pixel(1, 0, 0), Some(3));
    assert_eq!(hardware.obj.tiles().pixel(1, 1, 0), Some(4));
    assert_eq!(hardware.palette.get(5).g, 0x9A);
    assert_eq!(
        hardware.bg.palettes().resolve(BgBank::new(1).unwrap(), 1),
        Some(99)
    );
    assert_eq!(
        hardware.obj.palettes().resolve(ObjBank::new(1).unwrap(), 1),
        Some(98)
    );
    assert_eq!(hardware.obj.attributes().bank(2), ObjBank::new(5).unwrap());
}

fn framebuffer_last() -> usize {
    FRAMEBUFFER.len as usize - 1
}

#[test]
fn changes_to_the_typed_storage_are_read_at_once() {
    let mut hardware = hardware();
    hardware.palette.set(7, Rgb { r: 1, g: 2, b: 3 });
    hardware.obj.tiles_mut().set_pixel(0, 0, 0, 9);
    hardware
        .bg
        .palettes_mut()
        .set(BgBank::new(2).unwrap(), 3, 77);
    hardware.framebuffer.set_pixel(0, 0, 200);

    assert_eq!(hardware.read16(0x3A000 + 21), Ok(0x0201));
    assert_eq!(hardware.read(0x3A000 + 23), Ok(3));
    assert_eq!(hardware.read(0x2C000), Ok(0x90));
    assert_eq!(hardware.read(0x3A300 + 32 + 3), Ok(77));
    assert_eq!(hardware.read(0x10000), Ok(200));
}

#[test]
fn contiguous_regions_hand_over_at_their_boundaries() {
    let hardware = filled();
    let bytes = device_bytes(&hardware);
    let pairs = [(0, 1), (2, 3), (4, 5), (5, 6)];
    for (before, after) in pairs {
        let (previous, previous_bytes) = &bytes[before];
        let (next, next_bytes) = &bytes[after];
        assert_eq!(previous.end(), next.start);
        assert_eq!(
            hardware.read(previous.last()),
            Ok(*previous_bytes.last().unwrap())
        );
        assert_eq!(hardware.read(next.start), Ok(next_bytes[0]));
    }
}

#[test]
fn addresses_around_the_gaps_are_unmapped_and_their_neighbours_are_not() {
    let hardware = filled();
    let mapped_before = [0x22BFF, 0x33FFF, 0x3A3BF, 0x3A7FF];
    let mapped_after = [0x24000, 0x3A400, 0x2C000];
    for address in mapped_before.into_iter().chain(mapped_after) {
        assert_ne!(
            hardware.read(address),
            Ok(0),
            "{address:#x} should hold data"
        );
    }

    let unmapped = [
        0x22C00, 0x23FFF, // Between the framebuffer and the BG tiles.
        0x34000, 0x39FFF, // Tilemap windows, mapped in a later stage.
        0x3A3C0, 0x3A3FF, // Between OBP and the attributes.
        0x3A800, 0x3AFFF, // The rest of the palette page.
        0x3B000, 0x3C000, 0x3D000, 0xFFFFF,
    ];
    for address in unmapped {
        let mut hardware = filled();
        assert_eq!(hardware.read(address), Ok(0), "{address:#x}");
        hardware.write(address, 0xFF).unwrap();
        assert_eq!(hardware.read(address), Ok(0), "{address:#x}");
        assert_unchanged(&hardware, &filled_copy());
    }
}

#[test]
fn the_address_space_ends_at_the_twenty_bit_limit() {
    let mut hardware = hardware();
    assert_eq!(hardware.read(0xFFFFF), Ok(0));
    assert_eq!(hardware.write(0xFFFFF, 1), Ok(()));
    for address in [0x100000, 0x100001, u32::MAX - 1, u32::MAX] {
        let error = BusError::OutOfRange { address, length: 1 };
        assert_eq!(hardware.read(address), Err(error));
        assert_eq!(hardware.write(address, 1), Err(error));
    }
}

#[test]
fn sixteen_and_thirty_two_bit_accesses_are_little_endian() {
    let mut hardware = hardware();
    hardware.write32(0x100, 0x1122_3344).unwrap();
    assert_eq!(
        [0x100, 0x101, 0x102, 0x103].map(|address| hardware.read(address).unwrap()),
        [0x44, 0x33, 0x22, 0x11]
    );
    assert_eq!(hardware.read16(0x100), Ok(0x3344));
    assert_eq!(hardware.read16(0x102), Ok(0x1122));
    assert_eq!(hardware.read32(0x100), Ok(0x1122_3344));

    hardware.write16(0x200, 0xABCD).unwrap();
    assert_eq!(hardware.work_ram[0x200], 0xCD);
    assert_eq!(hardware.work_ram[0x201], 0xAB);
}

#[test]
fn multi_byte_accesses_cross_regions_byte_by_byte() {
    let mut hardware = hardware();
    hardware.write16(0x0FFFF, 0x1234).unwrap();
    assert_eq!(hardware.work_ram[0xFFFF], 0x34);
    assert_eq!(hardware.framebuffer.as_bytes()[0], 0x12);
    assert_eq!(hardware.read16(0x0FFFF), Ok(0x1234));

    hardware.write32(0x0FFFE, 0x1234_5678).unwrap();
    assert_eq!(hardware.work_ram[0xFFFE], 0x78);
    assert_eq!(hardware.work_ram[0xFFFF], 0x56);
    assert_eq!(hardware.framebuffer.as_bytes()[0], 0x34);
    assert_eq!(hardware.framebuffer.as_bytes()[1], 0x12);
}

#[test]
fn multi_byte_accesses_into_a_gap_keep_only_the_mapped_bytes() {
    let mut hardware = hardware();
    hardware.write16(0x22BFF, 0xBBAA).unwrap();
    assert_eq!(hardware.framebuffer.as_bytes()[framebuffer_last()], 0xAA);
    assert_eq!(hardware.read16(0x22BFF), Ok(0x00AA));
}

#[test]
fn multi_byte_accesses_past_the_end_fail_without_writing() {
    let mut hardware = filled();
    let before = filled_copy();
    assert_eq!(
        hardware.read16(0xFFFFF),
        Err(BusError::OutOfRange {
            address: 0xFFFFF,
            length: 2
        })
    );
    assert!(hardware.read32(0xFFFFD).is_err());
    assert!(hardware.write16(0xFFFFF, 0xFFFF).is_err());
    assert!(hardware.write32(0xFFFFD, u32::MAX).is_err());
    assert!(hardware.write32(u32::MAX, 0).is_err());
    assert_unchanged(&hardware, &before);

    assert_eq!(hardware.write32(0xFFFFC, u32::MAX), Ok(()));
}

#[test]
fn reserved_bits_of_the_object_attributes_read_as_zero() {
    let mut hardware = hardware();
    hardware.write(0x3A400, 0xFF).unwrap();
    assert_eq!(hardware.read(0x3A400), Ok(0x07));
    assert_eq!(hardware.obj.attributes().bank(0), ObjBank::new(7).unwrap());
    hardware.write(0x3A401, 0b1111_1000).unwrap();
    assert_eq!(hardware.read(0x3A401), Ok(0));
}

#[test]
fn entry_zero_of_every_palette_bank_stays_zero() {
    let mut hardware = hardware();
    for bank in 0..4 {
        let address = 0x3A300 + bank * 16;
        hardware.write(address, 0x55).unwrap();
        assert_eq!(hardware.read(address), Ok(0));
        hardware.write(address + 1, 0x55).unwrap();
        assert_eq!(hardware.read(address + 1), Ok(0x55));
        hardware.write(address, 0).unwrap();
        assert_eq!(hardware.read(address), Ok(0));
    }
    for bank in 0..8 {
        let address = 0x3A340 + bank * 16;
        hardware.write(address, 0x55).unwrap();
        assert_eq!(hardware.read(address), Ok(0));
        hardware.write(address + 15, 0x55).unwrap();
        assert_eq!(hardware.read(address + 15), Ok(0x55));
    }
}

#[test]
fn copying_overlapping_ranges_matches_memmove() {
    let mut forward = hardware();
    for (offset, byte) in (1..=8).enumerate() {
        forward.write(offset as u32, byte).unwrap();
    }
    let mut backward = forward.clone();

    forward.copy(2, 0, 6).unwrap();
    assert_eq!(forward.work_ram[..8], [1, 2, 1, 2, 3, 4, 5, 6]);

    backward.copy(0, 2, 6).unwrap();
    assert_eq!(backward.work_ram[..8], [3, 4, 5, 6, 7, 8, 7, 8]);
}

#[test]
fn blocks_cross_regions_and_unmapped_gaps() {
    let before = filled_copy();
    let mut hardware = before.clone();

    hardware.fill(0xFFFC, 0xAB, 8).unwrap();
    assert!(hardware.work_ram[0xFFFC..].iter().all(|&byte| byte == 0xAB));
    assert!(
        hardware.framebuffer.as_bytes()[..4]
            .iter()
            .all(|&byte| byte == 0xAB)
    );
    assert_eq!(
        hardware.framebuffer.as_bytes()[4],
        before.framebuffer.as_bytes()[4]
    );

    hardware.fill(0x22BFE, 0xCD, 4).unwrap();
    assert_eq!(
        hardware.framebuffer.as_bytes()[framebuffer_last() - 1..],
        [0xCD, 0xCD]
    );
    assert_eq!(hardware.read(0x22C00), Ok(0));
    assert_eq!(hardware.read(0x22C01), Ok(0));
}

#[test]
fn copying_from_a_gap_clears_the_destination() {
    let before = filled_copy();
    let mut hardware = before.clone();
    hardware.copy(0x24000, 0x22BFE, 4).unwrap();
    let framebuffer = before.framebuffer.as_bytes();
    assert_eq!(
        hardware.bg.tiles().as_bytes()[..4],
        [
            framebuffer[framebuffer_last() - 1],
            framebuffer[framebuffer_last()],
            0,
            0
        ]
    );
}

#[test]
fn copying_across_the_tile_banks_reads_both() {
    let mut hardware = filled();
    hardware.copy(0x100, 0x2BFFE, 4).unwrap();
    let before = filled_copy();
    let bg = before.bg.tiles().as_bytes();
    let obj = before.obj.tiles().as_bytes();
    assert_eq!(
        hardware.work_ram[0x100..0x104],
        [bg[bg.len() - 2], bg[bg.len() - 1], obj[0], obj[1]]
    );
}

#[test]
fn filling_across_the_palette_page_obeys_each_device() {
    let before = filled_copy();
    let mut hardware = before.clone();
    // From the end of the master palette, through BGP, OBP and the gap after
    // them, into the first attributes.
    hardware.fill(0x3A2FE, 0xFF, 0x10E).unwrap();

    assert_eq!(hardware.palette.to_rgb888()[766..], [0xFF, 0xFF]);
    for (offset, &byte) in hardware.bg.palettes().as_bytes().iter().enumerate() {
        assert_eq!(
            byte,
            if offset % 16 == 0 { 0 } else { 0xFF },
            "BGP {offset}"
        );
    }
    for (offset, &byte) in hardware.obj.palettes().as_bytes().iter().enumerate() {
        assert_eq!(
            byte,
            if offset % 16 == 0 { 0 } else { 0xFF },
            "OBP {offset}"
        );
    }
    assert_eq!(hardware.read(0x3A3C0), Ok(0));
    assert_eq!(hardware.obj.attributes().as_bytes()[..12], [7; 12]);
    assert_eq!(
        hardware.obj.attributes().as_bytes()[12],
        before.obj.attributes().as_bytes()[12]
    );
}

#[test]
fn copying_into_the_conditioned_regions_obeys_each_device() {
    let mut hardware = hardware();
    hardware.fill(0x200, 0xFF, 0x40).unwrap();
    hardware.copy(0x3A400, 0x200, 0x40).unwrap();
    hardware.copy(0x3A300, 0x200, 0x30).unwrap();

    assert!(
        hardware.obj.attributes().as_bytes()[..0x40]
            .iter()
            .all(|&byte| byte == 7)
    );
    for (offset, &byte) in hardware.bg.palettes().as_bytes()[..0x30].iter().enumerate() {
        assert_eq!(
            byte,
            if offset % 16 == 0 { 0 } else { 0xFF },
            "BGP {offset}"
        );
    }

    hardware.copy(0x300, 0x3A400, 0x40).unwrap();
    assert!(
        hardware.work_ram[0x300..0x340]
            .iter()
            .all(|&byte| byte == 7)
    );
}

#[test]
fn empty_blocks_need_valid_addresses() {
    let before = filled_copy();
    let mut hardware = before.clone();
    assert_eq!(hardware.fill(0, 1, 0), Ok(()));
    assert_eq!(hardware.fill(0xFFFFF, 1, 0), Ok(()));
    assert_eq!(hardware.copy(0xFFFFF, 0, 0), Ok(()));
    assert_eq!(hardware.copy(0, 0xFFFFF, 0), Ok(()));
    assert_unchanged(&hardware, &before);

    let at_limit = Err(BusError::OutOfRange {
        address: 0x100000,
        length: 0,
    });
    assert_eq!(hardware.fill(0x100000, 1, 0), at_limit);
    assert_eq!(hardware.copy(0x100000, 0, 0), at_limit);
    assert_eq!(hardware.copy(0, 0x100000, 0), at_limit);
    assert!(hardware.fill(u32::MAX, 1, 0).is_err());
    assert_unchanged(&hardware, &before);
}

#[test]
fn ranges_near_the_top_of_u32_do_not_wrap_into_the_address_space() {
    let before = filled_copy();
    let mut hardware = before.clone();
    // Each of these would wrap to a small end address if the sum overflowed.
    assert!(hardware.fill(u32::MAX, 1, 1).is_err());
    assert!(hardware.fill(u32::MAX - 1, 1, 4).is_err());
    assert!(hardware.fill(0x100, 1, u32::MAX).is_err());
    assert!(hardware.fill(0x100, 1, u32::MAX - 0xFF).is_err());
    assert!(hardware.fill(0, 1, u32::MAX).is_err());
    assert!(hardware.copy(0x100, 0, u32::MAX).is_err());
    assert!(hardware.copy(0, 0x100, u32::MAX).is_err());
    assert!(hardware.copy(u32::MAX, 0x100, 2).is_err());
    assert_unchanged(&hardware, &before);
}

#[test]
fn a_block_with_an_invalid_range_writes_nothing() {
    let before = filled_copy();
    let mut hardware = before.clone();
    // Each destination is valid and mapped, so a partial write would show.
    assert!(hardware.copy(0x100, 0xFFFFF, 2).is_err());
    assert!(hardware.copy(0x100, 0xFFFF0, 0x20).is_err());
    assert!(hardware.copy(0x3A400, 0x100000, 1).is_err());
    assert!(hardware.fill(0x100, 0xEE, 0x100000).is_err());
    assert_unchanged(&hardware, &before);
}

proptest! {
    // The range check agrees with arithmetic that cannot wrap.
    #[test]
    fn range_checks_do_not_wrap(
        start in prop_oneof![any::<u32>(), 0xFFFF0u32..0x100010, (u32::MAX - 32)..=u32::MAX],
        length in prop_oneof![any::<u32>(), 0u32..0x40, (u32::MAX - 32)..=u32::MAX],
    ) {
        let valid = u64::from(start) < u64::from(ADDRESS_SPACE_LEN)
            && u64::from(start) + u64::from(length) <= u64::from(ADDRESS_SPACE_LEN);
        prop_assert_eq!(check_range(start, length).is_ok(), valid);
    }

    #[test]
    fn multi_byte_values_round_trip_in_little_endian(
        address in 0u32..0xFFFC,
        value in any::<u32>(),
    ) {
        let mut hardware = hardware();
        hardware.write32(address, value).unwrap();
        prop_assert_eq!(hardware.read32(address), Ok(value));
        prop_assert_eq!(hardware.read16(address), Ok(value as u16));
        for (offset, byte) in value.to_le_bytes().into_iter().enumerate() {
            prop_assert_eq!(hardware.read(address + offset as u32), Ok(byte));
        }
    }

    // Work RAM against `Vec::copy_within`, both directions of overlap.
    #[test]
    fn copy_matches_copy_within_in_work_ram(
        initial in proptest::collection::vec(any::<u8>(), 256),
        source in 0usize..256,
        destination in 0usize..256,
        length in 0usize..256,
    ) {
        let length = length.min(256 - source.max(destination));
        let mut hardware = hardware();
        hardware.work_ram[..256].copy_from_slice(&initial);
        hardware.copy(destination as u32, source as u32, length as u32).unwrap();

        let mut model = initial;
        model.copy_within(source..source + length, destination);
        prop_assert_eq!(&hardware.work_ram[..256], &model[..]);
    }

    // Over regions with different rules, a block equals reading the whole
    // source first and then writing it byte by byte.
    #[test]
    fn copy_equals_reading_the_source_first(
        source in block_start(),
        destination in block_start(),
        length in 0u32..0x300,
    ) {
        let mut actual = filled_copy();
        actual.copy(destination, source, length).unwrap();

        let mut model = filled_copy();
        let bytes: Vec<u8> = (0..length).map(|i| model.read(source + i).unwrap()).collect();
        for (i, byte) in bytes.into_iter().enumerate() {
            model.write(destination + i as u32, byte).unwrap();
        }
        prop_assert!(actual == model);
    }

    #[test]
    fn fill_equals_single_writes(
        destination in block_start(),
        value in any::<u8>(),
        length in 0u32..0x300,
    ) {
        let mut actual = filled_copy();
        actual.fill(destination, value, length).unwrap();

        let mut model = filled_copy();
        for i in 0..length {
            model.write(destination + i, value).unwrap();
        }
        prop_assert!(actual == model);
    }
}

/// Starts of blocks near the seams between regions, gaps and the palette page.
fn block_start() -> impl Strategy<Value = u32> {
    prop_oneof![
        0xFF00u32..0x10100,
        0x22B80u32..0x24080,
        0x2BF80u32..0x2C080,
        0x3A280u32..0x3A900,
    ]
}
