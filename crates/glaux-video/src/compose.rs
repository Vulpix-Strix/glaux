//! Composition of the background layers onto the working framebuffer.
//!
//! The compositor reads the tiles, tilemaps, background palettes and graphics
//! registers of a [`CartridgeHardware`] and writes the winning master index of
//! every pixel straight into its framebuffer. It allocates nothing.
//!
//! # Depth
//!
//! Every candidate pixel carries a one-byte depth and the smaller depth wins.
//! A background pixel's depth is `effective_priority * 8 + rank`, where the
//! effective priority is the layer priority reduced by one when the tile's
//! boost bit is set, saturating at zero.
//!
//! Within one effective priority, ranks run from front to back:
//!
//! ```text
//!   FRONT (smaller depth wins)
//!     |
//!     v
//!   +----------------------+
//!   | 0  Boosted BG2       |
//!   | 1  Boosted BG1       |
//!   | 2  Boosted BG0       |
//!   +----------------------+
//!   | 3  Objects           |
//!   +----------------------+
//!   | 4  Normal BG2        |
//!   | 5  Normal BG1        |
//!   | 6  Normal BG0        |
//!   +----------------------+
//!     |
//!     v
//!   BACK
//! ```
//!
//! Objects occupy [`OBJECT_RANK`] (rank 3).
//!
//! The diagram already orders every source, so comparing two depths settles
//! any tie. A candidate takes over a pixel only when its depth is strictly
//! smaller, which means that when two depths are equal the one considered
//! first keeps the pixel.

use glaux_machine::bg::{BgPalettes, BgTileBank};
use glaux_machine::framebuffer::WIDTH;
use glaux_machine::hardware::CartridgeHardware;
use glaux_machine::tile_window::TileWindow;
use glaux_machine::tilemap::{BG_LAYERS, BgLayer, TILE_WINDOW_SIDE};

const TILE_SIDE: usize = 8;

/// Side of the wrapping map in pixels.
const MAP_SIDE: usize = TILE_WINDOW_SIDE * TILE_SIDE;
const MAP_MASK: usize = MAP_SIDE - 1;

/// Rank values per effective priority level.
const RANKS: u8 = 8;
const NORMAL_RANK_BASE: u8 = 4;

/// The rank that objects take between boosted and normal background tiles.
///
/// Objects must be considered in index order so that the lower index wins a
/// tie, see the [module documentation](self).
pub const OBJECT_RANK: u8 = 3;

/// Depth of a pixel no layer has claimed yet.
const BEHIND_ALL: u8 = u8::MAX;

/// Bits of `DISPLAY_ENABLE` that switch BG0, BG1 and BG2 on.
const BACKGROUND_BITS: u8 = 0b0111;

/// The depth of a pixel of `layer` whose tile has the given `boost` bit,
/// where `priority` is the layer's priority register (0 in front, 3 behind).
pub fn background_depth(priority: u8, boost: bool, layer: BgLayer) -> u8 {
    let effective = priority.saturating_sub(u8::from(boost));
    // BG2 is the frontmost layer of a tie.
    let order = (BG_LAYERS - 1 - layer.index()) as u8;
    let rank = if boost {
        order
    } else {
        NORMAL_RANK_BASE + order
    };
    effective * RANKS + rank
}

/// A background layer as the registers configure it.
struct Layer<'a> {
    which: BgLayer,
    window: &'a TileWindow,
    scroll_x: usize,
    scroll_y: usize,
    priority: u8,
}

/// Draws the enabled background layers over whatever the framebuffer holds.
///
/// The framebuffer is expected to hold the backdrop already. Pixels no layer
/// claims keep their value. With none of the three background bits of
/// `DISPLAY_ENABLE` set the framebuffer is left untouched.
///
/// Scroll registers keep the value the script wrote and are reduced modulo
/// 512 here, so the maps wrap on both axes. Tile 0 and tile-local index 0 are
/// transparent. A non-zero local index is opaque even when its palette entry
/// is master index 0.
pub fn compose_backgrounds(hardware: &mut CartridgeHardware) {
    let enabled = hardware.registers.display_enable();
    if enabled & BACKGROUND_BITS == 0 {
        return;
    }
    let CartridgeHardware {
        framebuffer,
        bg,
        tilemaps,
        registers,
        ..
    } = hardware;

    let layers = BgLayer::ALL.map(|which| {
        (enabled & (1 << which.index()) != 0).then(|| Layer {
            which,
            window: &tilemaps[which.index()],
            scroll_x: usize::from(registers.bg_scroll_x(which)) & MAP_MASK,
            scroll_y: usize::from(registers.bg_scroll_y(which)),
            priority: registers.bg_priority(which),
        })
    });

    let mut depths = [BEHIND_ALL; WIDTH];
    let (rows, _) = framebuffer.as_bytes_mut().as_chunks_mut::<WIDTH>();
    for (y, row) in rows.iter_mut().enumerate() {
        depths.fill(BEHIND_ALL);
        for layer in layers.iter().flatten() {
            compose_row(layer, y, bg.tiles(), bg.palettes(), row, &mut depths);
        }
    }
}

/// Composes one framebuffer row of `layer` in tile-wide spans.
fn compose_row(
    layer: &Layer<'_>,
    y: usize,
    tiles: &BgTileBank,
    palettes: &BgPalettes,
    row: &mut [u8; WIDTH],
    depths: &mut [u8; WIDTH],
) {
    let map_y = (y + layer.scroll_y) & MAP_MASK;
    let tile_row = map_y / TILE_SIDE;
    let line = map_y % TILE_SIDE;

    let mut x = 0;
    let mut map_x = layer.scroll_x;
    while x < WIDTH {
        let offset = map_x % TILE_SIDE;
        let span = (TILE_SIDE - offset).min(WIDTH - x);
        if let Some(entry) = layer.window.entry(map_x / TILE_SIDE, tile_row)
            && entry.tile() != 0
        {
            let depth = background_depth(layer.priority, entry.priority(), layer.which);
            let source_y = if entry.flip_y() {
                TILE_SIDE - 1 - line
            } else {
                line
            };
            for step in 0..span {
                let column = x + step;
                if depth >= depths[column] {
                    continue;
                }
                let local_x = offset + step;
                let source_x = if entry.flip_x() {
                    TILE_SIDE - 1 - local_x
                } else {
                    local_x
                };
                let Some(local) = tiles.pixel(usize::from(entry.tile()), source_x, source_y) else {
                    continue;
                };
                if let Some(color) = palettes.resolve(entry.bank(), local) {
                    row[column] = color;
                    depths[column] = depth;
                }
            }
        }
        x += span;
        map_x = (map_x + span) & MAP_MASK;
    }
}

#[cfg(test)]
mod tests;
