//! A background tilemap window in VRAM.

use crate::tilemap::{TILE_ENTRY_BYTES, TILE_WINDOW_SIDE, TileEntry, TileLayer};

/// Size of one window in bytes.
pub const TILE_WINDOW_BYTES: usize = TILE_WINDOW_SIDE * TILE_WINDOW_SIDE * TILE_ENTRY_BYTES;

/// The 64 x 64 entries a background layer reads, stored as little-endian
/// `u16` values in row-major order.
///
/// Entry `(x, y)` starts at byte `2 * (y * 64 + x)`. The window holds raw
/// bytes, so anything written through the bus is accepted and only the
/// reserved bit 15 of each entry is dropped. A window starts all zero and is
/// never filled from the cartridge's maps.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TileWindow {
    bytes: Box<[u8; TILE_WINDOW_BYTES]>,
}

impl TileWindow {
    pub fn new() -> Self {
        Self {
            bytes: Box::new([0; TILE_WINDOW_BYTES]),
        }
    }

    fn offset(x: usize, y: usize) -> Option<usize> {
        (x < TILE_WINDOW_SIDE && y < TILE_WINDOW_SIDE)
            .then(|| (y * TILE_WINDOW_SIDE + x) * TILE_ENTRY_BYTES)
    }

    /// The entry at `(x, y)`, or `None` outside the window.
    pub fn entry(&self, x: usize, y: usize) -> Option<TileEntry> {
        let offset = Self::offset(x, y)?;
        let raw = u16::from_le_bytes([self.bytes[offset], self.bytes[offset + 1]]);
        // Bit 15 is never stored, so the entry is always valid.
        TileEntry::from_raw(raw).ok()
    }

    /// Writes the entry at `(x, y)`. Returns `false` outside the window.
    pub fn set_entry(&mut self, x: usize, y: usize, entry: TileEntry) -> bool {
        let Some(offset) = Self::offset(x, y) else {
            return false;
        };
        self.bytes[offset..offset + TILE_ENTRY_BYTES].copy_from_slice(&entry.raw().to_le_bytes());
        true
    }

    /// Writes byte `offset` of the window. The high byte of each entry loses
    /// the reserved bit 15. Returns `false` past the end.
    pub fn set_byte(&mut self, offset: usize, value: u8) -> bool {
        let Some(byte) = self.bytes.get_mut(offset) else {
            return false;
        };
        *byte = if offset % TILE_ENTRY_BYTES == TILE_ENTRY_BYTES - 1 {
            value & (TileEntry::VALID_BITS >> 8) as u8
        } else {
            value
        };
        true
    }

    /// Copies a `width` x `height` region of `source` into the window.
    ///
    /// The region starts at world cell `(x, y)` of the map. Each world cell
    /// lands on the physical cell `(x mod 64, y mod 64)`, with a Euclidean
    /// modulo so negative coordinates wrap the same way as positive ones.
    /// Cells outside `source` are written as empty entries and the map itself
    /// never repeats. A side longer than the window is cut to 64 cells, because
    /// the rest would overwrite the cells just written.
    pub fn transfer(&mut self, source: &TileLayer, x: i32, y: i32, width: usize, height: usize) {
        // Widened so that adding a column or row to an extreme origin cannot
        // overflow.
        let (x, y) = (i64::from(x), i64::from(y));
        let side = TILE_WINDOW_SIDE as i64;
        for row in 0..height.min(TILE_WINDOW_SIDE) {
            let world_y = y + row as i64;
            for column in 0..width.min(TILE_WINDOW_SIDE) {
                let world_x = x + column as i64;
                let entry = usize::try_from(world_x)
                    .ok()
                    .zip(usize::try_from(world_y).ok())
                    .and_then(|(map_x, map_y)| source.get(map_x, map_y))
                    .unwrap_or_default();
                let window_x = world_x.rem_euclid(side) as usize;
                let window_y = world_y.rem_euclid(side) as usize;
                self.set_entry(window_x, window_y, entry);
            }
        }
    }

    pub fn as_bytes(&self) -> &[u8; TILE_WINDOW_BYTES] {
        &self.bytes
    }
}

impl Default for TileWindow {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests;
