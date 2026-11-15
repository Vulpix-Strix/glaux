//! The pixel a map shows, by the same rules the compositor follows.
//!
//! It reads the map itself, so maps up to 256 x 256 tiles show whole. Layer
//! priorities are the ones the registers have at power-on.

use glaux_machine::bg::{BgTileBank, BgVram};
use glaux_machine::registers::GraphicsRegisters;
use glaux_machine::tilemap::{BG_LAYERS, BG_TILE_CAPACITY, BgLayer, TileEntry, TileLayer};
use glaux_video::compose::background_depth;

use super::view::TILE;

#[derive(Debug, Clone, Copy)]
pub(super) struct Composer<'a> {
    layers: [&'a TileLayer; BG_LAYERS],
    shown: [bool; BG_LAYERS],
    /// Priority registers at power-on.
    priorities: [u8; BG_LAYERS],
}

#[derive(Debug, Clone, Copy)]
pub(super) struct Cell {
    entries: [Option<(TileEntry, u8)>; BG_LAYERS],
}

impl<'a> Composer<'a> {
    pub(super) fn new(layers: [&'a TileLayer; BG_LAYERS], shown: [bool; BG_LAYERS]) -> Self {
        let registers = GraphicsRegisters::new();
        Self {
            layers,
            shown,
            priorities: BgLayer::ALL.map(|layer| registers.bg_priority(layer)),
        }
    }

    pub(super) fn cell(&self, x: usize, y: usize) -> Cell {
        let entries = BgLayer::ALL.map(|layer| {
            let index = layer.index();
            if !self.shown[index] {
                return None;
            }
            let entry = self.layers[index].get(x, y)?;
            (entry.tile() != 0).then(|| {
                let depth = background_depth(self.priorities[index], entry.priority(), layer);
                (entry, depth)
            })
        });
        Cell { entries }
    }

    #[cfg(test)]
    pub(super) fn pixel(&self, bg: &BgVram, x: usize, y: usize) -> Option<u8> {
        let tile = TILE as usize;
        self.cell(x / tile, y / tile).pixel(bg, x % tile, y % tile)
    }
}

impl Cell {
    /// Tile 0 draws nothing, as in a map.
    pub(super) fn single(entry: TileEntry) -> Self {
        let mut entries = [None; BG_LAYERS];
        if entry.tile() != 0 {
            entries[0] = Some((entry, 0));
        }
        Self { entries }
    }

    pub(super) fn is_empty(&self) -> bool {
        self.entries.iter().all(Option::is_none)
    }

    pub(super) fn pixel(&self, bg: &BgVram, x: usize, y: usize) -> Option<u8> {
        let side = TILE as usize;
        let mut shown: Option<(u8, u8)> = None;
        for (entry, depth) in self.entries.iter().flatten() {
            if shown.is_some_and(|(_, best)| *depth >= best) {
                continue;
            }
            let source_x = if entry.flip_x() { side - 1 - x } else { x };
            let source_y = if entry.flip_y() { side - 1 - y } else { y };
            let Some(local) = bg
                .tiles()
                .pixel(usize::from(entry.tile()), source_x, source_y)
            else {
                continue;
            };
            if let Some(master) = bg.palettes().resolve(entry.bank(), local) {
                shown = Some((master, *depth));
            }
        }
        shown.map(|(master, _)| master)
    }

    /// The frontmost entry whose tile draws something, as the master of its
    /// tile's most common color and its depth.
    pub(super) fn front_color(&self, bg: &BgVram, colors: &mut TileColors<'_>) -> Option<(u8, u8)> {
        let mut shown: Option<(u8, u8)> = None;
        for (entry, depth) in self.entries.iter().flatten() {
            if shown.is_some_and(|(_, best)| *depth >= best) {
                continue;
            }
            let Some(local) = colors.get(usize::from(entry.tile())) else {
                continue;
            };
            if let Some(master) = bg.palettes().resolve(entry.bank(), local) {
                shown = Some((master, *depth));
            }
        }
        shown
    }
}

/// The most common opaque color of each tile, counted the first time it is
/// asked for.
pub(super) struct TileColors<'a> {
    tiles: &'a BgTileBank,
    colors: [Option<Option<u8>>; BG_TILE_CAPACITY],
}

impl<'a> TileColors<'a> {
    pub(super) fn new(tiles: &'a BgTileBank) -> Self {
        Self {
            tiles,
            colors: [None; BG_TILE_CAPACITY],
        }
    }

    /// The local color, the lowest of the most common ones on a tie, or
    /// `None` for a tile that draws nothing.
    pub(super) fn get(&mut self, tile: usize) -> Option<u8> {
        let tiles = self.tiles;
        *self.colors.get_mut(tile)?.get_or_insert_with(|| {
            let side = TILE as usize;
            let mut counts = [0u8; 16];
            for y in 0..side {
                for x in 0..side {
                    if let Some(local) = tiles.pixel(tile, x, y).filter(|&local| local != 0) {
                        counts[usize::from(local) & 15] += 1;
                    }
                }
            }
            let (local, count) = counts
                .iter()
                .enumerate()
                .rev()
                .max_by_key(|(_, count)| **count)?;
            (*count > 0).then_some(local as u8)
        })
    }
}

#[cfg(test)]
mod tests;
