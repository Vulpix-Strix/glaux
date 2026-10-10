use super::*;
use pretty_assertions::assert_eq;
use proptest::prelude::*;

#[test]
fn rgb888_round_trips_in_channel_order() {
    let palette = Palette::default();
    let bytes = palette.to_rgb888();
    assert_eq!(bytes[..3], [0x47, 0x2d, 0x3c]);
    assert_eq!(Palette::from_rgb888(&bytes), Ok(palette));
}

#[test]
fn set_replaces_one_entry_and_encodes_at_its_offset() {
    let mut palette = Palette::default();
    // #ff00ff is an ordinary color, not a reserved marker.
    let magenta = Rgb::from_hex(0xff00ff);
    assert_eq!(palette.set(42, magenta), Rgb::from_hex(0x383b4a));
    assert_eq!(palette.set(255, magenta), Rgb::from_hex(0xab73e4));
    let bytes = palette.to_rgb888();
    assert_eq!(bytes[42 * 3..43 * 3], [0xff, 0x00, 0xff]);
    assert_eq!(bytes[255 * 3..], [0xff, 0x00, 0xff]);
    assert_eq!(bytes[41 * 3..42 * 3], [0x24, 0x25, 0x32]);
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
    let palette = Palette::default();
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

// Every entry is a real color: no placeholder fills the extension and no
// two indices share a color.
#[test]
fn default_entries_are_all_distinct() {
    let palette = Palette::default();
    let mut colors = palette.entries().to_vec();
    colors.sort_unstable_by_key(|c| (c.r, c.g, c.b));
    colors.dedup();
    assert_eq!(colors.len(), PALETTE_LEN);
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
