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
