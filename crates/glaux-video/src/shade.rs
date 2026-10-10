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
mod tests;
