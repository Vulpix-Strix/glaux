use super::*;
use pretty_assertions::assert_eq;
use proptest::prelude::*;

#[test]
fn rgb888_round_trips_in_channel_order() {
    let palette = Palette::provisional();
    let bytes = palette.to_rgb888();
    assert_eq!(bytes[..3], [0x47, 0x2d, 0x3c]);
    assert_eq!(Palette::from_rgb888(&bytes), Ok(palette));
}

#[test]
fn set_replaces_one_entry_and_encodes_at_its_offset() {
    let mut palette = Palette::provisional();
    let red = Rgb::from_hex(0xe6482e);
    assert_eq!(palette.set(42, red), DEBUG_MAGENTA);
    assert_eq!(palette.set(255, red), DEBUG_MAGENTA);
    let bytes = palette.to_rgb888();
    assert_eq!(bytes[42 * 3..43 * 3], [0xe6, 0x48, 0x2e]);
    assert_eq!(bytes[255 * 3..], [0xe6, 0x48, 0x2e]);
    assert_eq!(bytes[41 * 3..42 * 3], [0xff, 0x00, 0xff]);
}

#[test]
fn rgb888_rejects_wrong_lengths() {
    for length in [0, PALETTE_RGB888_BYTES - 1, PALETTE_RGB888_BYTES + 1] {
        assert_eq!(
            Palette::from_rgb888(&vec![0; length]),
            Err(PaletteBytesError { length })
        );
    }
}

#[test]
fn first_32_entries_are_zughy_32() {
    let palette = Palette::provisional();
    assert_eq!(palette.entries()[..32], ZUGHY_32);
    assert_eq!(
        palette.get(0),
        Rgb {
            r: 0x47,
            g: 0x2d,
            b: 0x3c
        }
    );
    assert_eq!(
        palette.get(31),
        Rgb {
            r: 0x4f,
            g: 0x54,
            b: 0x6b
        }
    );
}

#[test]
fn unused_entries_are_debug_magenta() {
    let palette = Palette::provisional();
    assert_eq!(
        DEBUG_MAGENTA,
        Rgb {
            r: 255,
            g: 0,
            b: 255
        }
    );
    for index in 32..=255u8 {
        assert_eq!(palette.get(index), DEBUG_MAGENTA, "entry {index}");
    }
}

proptest! {
    // Any 768 bytes are a palette: entry `i` takes bytes `3i..3i + 3` as
    // red, green and blue, and writing it back gives the same bytes.
    #[test]
    fn any_rgb888_bytes_load_in_channel_order_and_round_trip(
        bytes in proptest::collection::vec(any::<u8>(), PALETTE_RGB888_BYTES),
    ) {
        let palette = Palette::from_rgb888(&bytes).unwrap();
        for (index, rgb) in bytes.chunks(3).enumerate() {
            let expected = Rgb { r: rgb[0], g: rgb[1], b: rgb[2] };
            prop_assert_eq!(palette.get(index as u8), expected);
        }
        prop_assert_eq!(&palette.to_rgb888()[..], &bytes[..]);
    }
}
