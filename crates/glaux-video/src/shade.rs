//! Palette-index tables that approximate a brightness change within a fixed
//! palette.

use glaux_machine::palette::{PALETTE_LEN, Palette, Rgb};

/// For every palette index, the index of the palette entry nearest to that
/// entry's color with each channel scaled by `keep_percent`.
///
/// Scaled channels are rounded down and capped at 255. "Nearest" is the
/// smallest squared RGB distance, with ties going to the lowest index. The
/// result is only as close as the palette allows: it is not guaranteed to be
/// perceptually darker or lighter than the original entry.
pub fn shade_table(palette: &Palette, keep_percent: u8) -> [u8; PALETTE_LEN] {
    let scale = |channel: u8| (u32::from(channel) * u32::from(keep_percent) / 100).min(255);
    let entries = palette.entries();
    let mut table = [0; PALETTE_LEN];
    for (slot, color) in table.iter_mut().zip(entries) {
        let target = [scale(color.r), scale(color.g), scale(color.b)];
        *slot = nearest(entries, target);
    }
    table
}

/// Index of the entry of `palette` nearest to `color`: the smallest squared
/// RGB distance, with ties going to the lowest index.
pub fn nearest_index(palette: &Palette, color: Rgb) -> u8 {
    nearest(
        palette.entries(),
        [color.r, color.g, color.b].map(u32::from),
    )
}

/// Index of the first entry with the smallest squared distance to `target`.
fn nearest(entries: &[Rgb; PALETTE_LEN], target: [u32; 3]) -> u8 {
    let distance = |color: &Rgb| {
        [color.r, color.g, color.b]
            .iter()
            .zip(target)
            .map(|(&channel, target)| u32::from(channel).abs_diff(target).pow(2))
            .sum::<u32>()
    };
    // `min_by_key` keeps the first of equal minima, so ties go to the lowest
    // index. Every index of a 256-entry palette fits in `u8`.
    entries
        .iter()
        .enumerate()
        .min_by_key(|(_, color)| distance(color))
        .map_or(0, |(index, _)| index as u8)
}

#[cfg(test)]
mod tests {
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
}
