use super::*;
use pretty_assertions::assert_eq;

/// A palette whose first entries are `colors` and the rest pure white.
fn palette(colors: &[u32]) -> Palette {
    let mut entries = [Rgb::from_hex(0xffffff); PALETTE_LEN];
    for (entry, &hex) in entries.iter_mut().zip(colors) {
        *entry = Rgb::from_hex(hex);
    }
    Palette::new(entries)
}

#[test]
fn maps_each_entry_to_the_nearest_scaled_color() {
    let palette = palette(&[0x000000, 0x404040, 0x808080, 0xc8c8c8]);
    let table = shade_table(&palette, 50);
    // 0x40 -> 32 ties 0x00 and 0x40; 0x80 -> 0x40; 0xc8 (200) -> 100,
    // nearer 0x80 (128) than 0x40 (64); white -> 127, nearest 0x80.
    assert_eq!(table[..4], [0, 0, 1, 2]);
    assert!(table[4..].iter().all(|&index| index == 2));
}

#[test]
fn scales_each_channel_separately() {
    let palette = palette(&[0x000000, 0x400000, 0x004000, 0x800000]);
    assert_eq!(shade_table(&palette, 50)[3], 1);
}

#[test]
fn ties_go_to_the_lowest_index() {
    // 0x60 at 50% is 0x30, equally far from 0x20 and 0x40.
    for colors in [
        [0x404040, 0x202020, 0x606060],
        [0x202020, 0x404040, 0x606060],
    ] {
        assert_eq!(shade_table(&palette(&colors), 50)[2], 0, "{colors:x?}");
    }
}

#[test]
fn keeping_everything_maps_distinct_entries_to_themselves() {
    let palette = palette(&[0x000000, 0x123456, 0xfedcba, 0x00ff00]);
    let table = shade_table(&palette, 100);
    assert_eq!(table[..4], [0, 1, 2, 3]);
    // The white entries all resolve to the first white.
    assert!(table[4..].iter().all(|&index| index == 4));
}

#[test]
fn nearest_index_prefers_exact_then_lowest_matches() {
    let palette = palette(&[0x000000, 0x123456, 0x123456, 0xfefefe]);
    assert_eq!(nearest_index(&palette, Rgb::from_hex(0x123456)), 1);
    assert_eq!(nearest_index(&palette, Rgb::from_hex(0x010101)), 0);
    assert_eq!(nearest_index(&palette, Rgb::from_hex(0xfdfdfd)), 3);
}

#[test]
fn scaled_channels_are_capped() {
    let palette = palette(&[0x000000, 0x808080]);
    assert_eq!(shade_table(&palette, 255)[1], 2);
}
