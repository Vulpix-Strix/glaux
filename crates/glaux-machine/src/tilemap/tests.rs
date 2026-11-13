use super::*;
use pretty_assertions::assert_eq;
use proptest::prelude::*;

fn bank(n: u8) -> BgBank {
    BgBank::new(n).unwrap()
}

#[test]
fn packs_every_field_into_its_bits() {
    let entry = TileEntry::new(1023, true, true, true, bank(3)).unwrap();
    assert_eq!(entry.raw(), 0x7FFF);

    let entry = TileEntry::new(5, true, false, false, bank(0)).unwrap();
    assert_eq!(entry.raw(), 0x0405);
    let entry = TileEntry::new(5, false, true, false, bank(0)).unwrap();
    assert_eq!(entry.raw(), 0x0805);
    let entry = TileEntry::new(5, false, false, true, bank(0)).unwrap();
    assert_eq!(entry.raw(), 0x1005);
    let entry = TileEntry::new(5, false, false, false, bank(2)).unwrap();
    assert_eq!(entry.raw(), 0x4005);
}

#[test]
fn reads_every_field_back() {
    let entry = TileEntry::from_raw(0b0101_0110_0000_0001).unwrap();
    assert_eq!(entry.tile(), 0b10_0000_0001);
    assert!(entry.flip_x());
    assert!(!entry.flip_y());
    assert!(entry.priority());
    assert_eq!(entry.bank(), bank(2));
}

#[test]
fn rejects_tiles_and_banks_out_of_range() {
    assert_eq!(
        TileEntry::new(1024, false, false, false, BgBank::ZERO),
        None
    );
    assert_eq!(BgBank::new(4), None);
}

#[test]
fn the_reserved_bit_is_rejected() {
    assert_eq!(
        TileEntry::from_raw(0x8000),
        Err(TileEntryError::ReservedBit { raw: 0x8000 })
    );
}

#[test]
fn only_the_all_zero_entry_is_empty() {
    assert!(TileEntry::EMPTY.is_empty());
    assert!(!TileEntry::from_raw(1).unwrap().is_empty());
    assert!(!TileEntry::from_raw(0x2000).unwrap().is_empty());
}

#[test]
fn layers_are_little_endian_and_row_major() {
    // 2x2, cells (0,0) (1,0) (0,1) (1,1).
    let bytes = [0x34, 0x12, 0x01, 0x00, 0x02, 0x00, 0xFF, 0x7F];
    let layer = TileLayer::from_bytes(2, 2, &bytes).unwrap();
    assert_eq!(layer.get(0, 0).unwrap().raw(), 0x1234);
    assert_eq!(layer.get(1, 0).unwrap().raw(), 0x0001);
    assert_eq!(layer.get(0, 1).unwrap().raw(), 0x0002);
    assert_eq!(layer.get(1, 1).unwrap().raw(), 0x7FFF);
    assert_eq!(layer.to_bytes(), bytes);
}

#[test]
fn layer_length_must_match_exactly() {
    for length in [0, 7, 9] {
        assert_eq!(
            TileLayer::from_bytes(2, 2, &vec![0; length]),
            Err(TileLayerError::Length {
                width: 2,
                height: 2,
                expected: 8,
                length
            })
        );
    }
}

#[test]
fn a_reserved_bit_names_its_cell() {
    let mut bytes = vec![0; 8];
    bytes[5] = 0x80;
    assert_eq!(
        TileLayer::from_bytes(2, 2, &bytes),
        Err(TileLayerError::ReservedBit {
            cell: 2,
            raw: 0x8000
        })
    );
}

#[test]
fn absurd_dimensions_are_an_error_not_an_overflow() {
    assert_eq!(
        TileLayer::from_bytes(usize::MAX, 2, &[]),
        Err(TileLayerError::TooLarge {
            width: usize::MAX,
            height: 2
        })
    );
    assert_eq!(TileLayer::empty(usize::MAX, 2), None);
}

#[test]
fn set_returns_the_previous_entry_and_stays_in_bounds() {
    let mut layer = TileLayer::empty(3, 2).unwrap();
    let entry = TileEntry::from_raw(9).unwrap();
    assert_eq!(layer.set(2, 1, entry), Some(TileEntry::EMPTY));
    assert_eq!(layer.set(2, 1, TileEntry::EMPTY), Some(entry));
    assert_eq!(layer.set(3, 0, entry), None);
    assert_eq!(layer.set(0, 2, entry), None);
    assert_eq!(layer.index_of(2, 1), Some(5));
    assert!(layer.is_blank());
}

#[test]
fn the_largest_layer_round_trips() {
    let layer = TileLayer::empty(256, 256).unwrap();
    assert_eq!(layer.to_bytes().len(), 131_072);
    assert_eq!(
        TileLayer::from_bytes(256, 256, &layer.to_bytes()),
        Ok(layer)
    );
}

proptest! {
    #[test]
    fn bytes_without_the_reserved_bit_round_trip(
        raws in proptest::collection::vec(0u16..0x8000, 12),
    ) {
        let bytes: Vec<u8> = raws.iter().flat_map(|raw| raw.to_le_bytes()).collect();
        let layer = TileLayer::from_bytes(4, 3, &bytes).unwrap();
        prop_assert_eq!(layer.to_bytes(), bytes);
    }

    #[test]
    fn fields_survive_packing(
        tile in 0u16..1024,
        flip_x: bool,
        flip_y: bool,
        priority: bool,
        b in 0u8..4,
    ) {
        let entry = TileEntry::new(tile, flip_x, flip_y, priority, bank(b)).unwrap();
        prop_assert_eq!(entry.tile(), tile);
        prop_assert_eq!(entry.flip_x(), flip_x);
        prop_assert_eq!(entry.flip_y(), flip_y);
        prop_assert_eq!(entry.priority(), priority);
        prop_assert_eq!(entry.bank(), bank(b));
        prop_assert_eq!(TileEntry::from_raw(entry.raw()), Ok(entry));
    }
}

mod constants {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn the_constants_keep_the_approved_layout() {
        assert_eq!(ENTRY_INDEX_MASK, 0x03FF);
        assert_eq!(
            [ENTRY_FLIP_X, ENTRY_FLIP_Y, ENTRY_PRIORITY],
            [0x0400, 0x0800, 0x1000]
        );
        assert_eq!((ENTRY_BANK_SHIFT, ENTRY_BANK_MASK), (13, 0b11));
        assert_eq!(TileEntry::VALID_BITS, 0x7FFF);
    }

    #[test]
    fn an_entry_built_from_the_constants_equals_the_constructor_and_reads_back() {
        for tile in [0, 1, 511, 1023] {
            for flags in 0..8u8 {
                for b in 0..4 {
                    let mut raw = tile | (u16::from(b) << ENTRY_BANK_SHIFT);
                    for (bit, flag) in [(0, ENTRY_FLIP_X), (1, ENTRY_FLIP_Y), (2, ENTRY_PRIORITY)] {
                        if flags & (1 << bit) != 0 {
                            raw |= flag;
                        }
                    }
                    let built = TileEntry::new(
                        tile,
                        flags & 1 != 0,
                        flags & 2 != 0,
                        flags & 4 != 0,
                        bank(b),
                    )
                    .unwrap();
                    assert_eq!(built.raw(), raw);
                    let read = TileEntry::from_raw(raw).unwrap();
                    assert_eq!(read, built);
                    assert_eq!(read.raw() & ENTRY_INDEX_MASK, tile);
                    assert_eq!(read.bank(), bank(b));
                }
            }
        }
    }

    #[test]
    fn the_reserved_bit_stays_outside_every_field_constant() {
        let fields = ENTRY_INDEX_MASK
            | ENTRY_FLIP_X
            | ENTRY_FLIP_Y
            | ENTRY_PRIORITY
            | (ENTRY_BANK_MASK << ENTRY_BANK_SHIFT);
        assert_eq!(fields, TileEntry::VALID_BITS);
        assert!(TileEntry::from_raw(1 << 15).is_err());
    }
}
