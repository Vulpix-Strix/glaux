use super::*;
use pretty_assertions::assert_eq;

fn bank(n: u8) -> ObjBank {
    ObjBank::new(n).unwrap()
}

#[test]
fn every_entry_starts_zero_and_disabled() {
    let oam = Oam::new();
    assert!(oam.as_bytes().iter().all(|&byte| byte == 0));
    for index in 0..OAM_ENTRIES {
        let entry = oam.entry(index).unwrap();
        assert_eq!(entry, OamEntry::default());
        assert!(!entry.enabled);
    }
}

#[test]
fn an_entry_is_packed_into_the_approved_layout() {
    let mut oam = Oam::new();
    let entry = OamEntry {
        x: -2,
        y: 300,
        tile: 0x3FF,
        bank: bank(5),
        flip_x: true,
        flip_y: true,
        priority: 2,
        size: 6,
        enabled: true,
    };
    assert!(oam.set_entry(1, entry));
    // X, Y, tile, then bank 5 | flip X | flip Y | priority 2, then size 6
    // with the enabled bit.
    assert_eq!(
        oam.as_bytes()[8..16],
        [0xFE, 0xFF, 0x2C, 0x01, 0xFF, 0x03, 0x5D, 0x0E]
    );
    assert_eq!(oam.entry(1), Some(entry));
    assert_eq!(oam.entry(0), Some(OamEntry::default()));
}

#[test]
fn fields_out_of_range_stay_inside_their_own_bits() {
    let mut oam = Oam::new();
    // A size code above 7 must not reach the enabled bit.
    oam.set_entry(
        0,
        OamEntry {
            size: 0xFF,
            ..OamEntry::default()
        },
    );
    assert_eq!(oam.as_bytes()[7], 0b0000_0111);
    assert!(!oam.entry(0).unwrap().enabled);

    // A priority above 3 must not reach the flips or the reserved bit.
    oam.set_entry(
        0,
        OamEntry {
            priority: 0xFF,
            ..OamEntry::default()
        },
    );
    assert_eq!(oam.as_bytes()[6], 0b0110_0000);

    // A tile above 1023 must not reach the reserved bits.
    oam.set_entry(
        0,
        OamEntry {
            tile: 0xFFFF,
            ..OamEntry::default()
        },
    );
    assert_eq!(oam.as_bytes()[4..6], [0xFF, 0x03]);
}

#[test]
fn size_code_seven_is_stored_as_given() {
    let mut oam = Oam::new();
    oam.set_entry(
        0,
        OamEntry {
            size: 7,
            enabled: true,
            ..OamEntry::default()
        },
    );
    assert_eq!(oam.as_bytes()[7], 0x0F);
    assert_eq!(oam.entry(0).unwrap().size, 7);
}

#[test]
fn size_codes_select_the_approved_dimensions() {
    let tiles: Vec<_> = (0..=8)
        .map(|size| {
            OamEntry {
                size,
                ..OamEntry::default()
            }
            .tiles()
        })
        .collect();
    // 8x8, 16x16, 32x32, 8x16, 16x8, 16x32 and 32x16 pixels, then the
    // reserved code 7 and a value that does not fit the field.
    assert_eq!(
        tiles,
        [
            Some((1, 1)),
            Some((2, 2)),
            Some((4, 4)),
            Some((1, 2)),
            Some((2, 1)),
            Some((2, 4)),
            Some((4, 2)),
            None,
            None,
        ]
    );
}

#[test]
fn raw_bytes_lose_the_reserved_bits_of_their_position() {
    let mut oam = Oam::new();
    for index in [0, OAM_ENTRIES - 1] {
        for offset in 0..OAM_ENTRY_BYTES {
            assert!(oam.set_byte(index * OAM_ENTRY_BYTES + offset, 0xFF));
        }
        let start = index * OAM_ENTRY_BYTES;
        assert_eq!(
            oam.as_bytes()[start..start + OAM_ENTRY_BYTES],
            [0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x03, 0x7F, 0x0F]
        );
    }
}

#[test]
fn entries_past_the_table_are_refused_without_touching_it() {
    let mut oam = Oam::new();
    let entry = OamEntry {
        x: 1,
        y: 2,
        tile: 3,
        enabled: true,
        ..OamEntry::default()
    };
    assert!(oam.set_entry(OAM_ENTRIES - 1, entry));
    let before = oam.clone();

    // The last index whose offset would still fit a multiplication but not
    // the addition of an entry's length.
    for index in [OAM_ENTRIES, usize::MAX / OAM_ENTRY_BYTES, usize::MAX] {
        assert_eq!(oam.entry(index), None, "{index:#x}");
        assert!(!oam.set_entry(index, entry), "{index:#x}");
    }
    assert!(!oam.set_byte(OAM_BYTES, 1));
    // A failed comparison would print the whole table.
    assert!(oam == before, "the table was modified");

    assert_eq!(oam.entry(OAM_ENTRIES - 1), Some(entry));
}

mod constants {
    use super::*;
    use pretty_assertions::assert_eq;
    use proptest::prelude::*;

    /// The eight bytes of an entry, built only from the field constants.
    fn packed(entry: OamEntry) -> [u8; OAM_ENTRY_BYTES] {
        let mut bytes = [0u8; OAM_ENTRY_BYTES];
        bytes[FIELD_X..][..2].copy_from_slice(&entry.x.to_le_bytes());
        bytes[FIELD_Y..][..2].copy_from_slice(&entry.y.to_le_bytes());
        bytes[FIELD_TILE..][..2].copy_from_slice(&entry.tile.to_le_bytes());
        let mut attributes = entry.bank.get() | entry.priority << ATTR_PRIORITY_SHIFT;
        if entry.flip_x {
            attributes |= ATTR_FLIP_X;
        }
        if entry.flip_y {
            attributes |= ATTR_FLIP_Y;
        }
        bytes[FIELD_ATTRIBUTES] = attributes;
        bytes[FIELD_SIZE] = entry.size | if entry.enabled { ENABLED } else { 0 };
        bytes
    }

    #[test]
    fn the_constants_keep_the_approved_layout() {
        assert_eq!(
            [FIELD_X, FIELD_Y, FIELD_TILE, FIELD_ATTRIBUTES, FIELD_SIZE],
            [0, 2, 4, 6, 7]
        );
        assert_eq!(TILE_MASK, 0x03FF);
        assert_eq!(ATTR_BANK_MASK, 0b111);
        assert_eq!(
            [
                ATTR_FLIP_X,
                ATTR_FLIP_Y,
                ATTR_PRIORITY_SHIFT,
                ATTR_PRIORITY_MASK
            ],
            [0x08, 0x10, 5, 0b11]
        );
        assert_eq!([SIZE_MASK, ENABLED], [0b111, 0x08]);
    }

    #[test]
    fn the_size_codes_name_the_approved_dimensions() {
        let tiles = |size| {
            OamEntry {
                size,
                ..OamEntry::default()
            }
            .tiles()
        };
        assert_eq!(tiles(SIZE_8X8), Some((1, 1)));
        assert_eq!(tiles(SIZE_16X16), Some((2, 2)));
        assert_eq!(tiles(SIZE_32X32), Some((4, 4)));
        assert_eq!(tiles(SIZE_8X16), Some((1, 2)));
        assert_eq!(tiles(SIZE_16X8), Some((2, 1)));
        assert_eq!(tiles(SIZE_16X32), Some((2, 4)));
        assert_eq!(tiles(SIZE_32X16), Some((4, 2)));
        assert_eq!(tiles(SIZE_RESERVED), None);
    }

    fn entries() -> impl Strategy<Value = OamEntry> {
        (
            any::<i16>(),
            any::<i16>(),
            0..=TILE_MASK,
            0..=ATTR_BANK_MASK,
            any::<bool>(),
            any::<bool>(),
            0..=ATTR_PRIORITY_MASK,
            0..=SIZE_MASK,
            any::<bool>(),
        )
            .prop_map(|(x, y, tile, b, flip_x, flip_y, priority, size, enabled)| {
                OamEntry {
                    x,
                    y,
                    tile,
                    bank: bank(b),
                    flip_x,
                    flip_y,
                    priority,
                    size,
                    enabled,
                }
            })
    }

    proptest! {
        #[test]
        fn an_entry_built_from_the_constants_decodes_to_its_fields(entry in entries()) {
            let mut oam = Oam::new();
            for (offset, byte) in packed(entry).into_iter().enumerate() {
                prop_assert!(oam.set_byte(OAM_ENTRY_BYTES + offset, byte));
            }
            prop_assert_eq!(oam.entry(1), Some(entry));
        }

        #[test]
        fn an_encoded_entry_is_the_one_built_from_the_constants(entry in entries()) {
            let mut oam = Oam::new();
            prop_assert!(oam.set_entry(2, entry));
            prop_assert_eq!(&oam.as_bytes()[2 * 8..3 * 8], &packed(entry));
        }
    }
}
