//! G1 background tilemap entries and layers.
//!
//! Each cell of a background layer is one 16-bit entry, stored little-endian
//! and row-major:
//!
//! | Bits  | Meaning                             |
//! |-------|-------------------------------------|
//! | 0-9   | background tile index               |
//! | 10    | flip X                              |
//! | 11    | flip Y                              |
//! | 12    | priority                            |
//! | 13-14 | background palette (BGP) bank, 0-3  |
//! | 15    | reserved, must be 0                 |
//!
//! The all-zero entry is the empty cell. Background tile 0 is reserved as
//! transparent, so the empty cell draws nothing. That convention is separate
//! from palette index 0, which is transparent inside every tile.

/// Number of background layers (BG0 to BG2).
pub const BG_LAYERS: usize = 3;
/// Background tiles one tile bank holds, and the tile index range of an entry.
pub const BG_TILE_CAPACITY: usize = 1024;
/// Background palette banks, selected by a two-bit field.
pub const BG_PALETTE_BANKS: usize = 4;
/// Size of one tilemap entry in bytes.
pub const TILE_ENTRY_BYTES: usize = 2;
/// Entries along each side of a VRAM tilemap window.
pub const TILE_WINDOW_SIDE: usize = 64;

const TILE_MASK: u16 = 0x03FF;
const FLIP_X: u16 = 1 << 10;
const FLIP_Y: u16 = 1 << 11;
const PRIORITY: u16 = 1 << 12;
const BANK_SHIFT: u16 = 13;
const BANK_MASK: u16 = 0b11;
const RESERVED: u16 = 1 << 15;

/// One of the three fixed background layers.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum BgLayer {
    Bg0,
    Bg1,
    Bg2,
}

impl BgLayer {
    pub const ALL: [Self; BG_LAYERS] = [Self::Bg0, Self::Bg1, Self::Bg2];

    /// The layer's position, 0 for BG0.
    pub const fn index(self) -> usize {
        self as usize
    }

    /// The layer at `index`, or `None` past BG2.
    pub const fn from_index(index: usize) -> Option<Self> {
        match index {
            0 => Some(Self::Bg0),
            1 => Some(Self::Bg1),
            2 => Some(Self::Bg2),
            _ => None,
        }
    }
}

impl std::fmt::Display for BgLayer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "BG{}", self.index())
    }
}

/// A background palette bank number, always in `0..BG_PALETTE_BANKS`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct BgBank(u8);

impl BgBank {
    /// BGP0.
    pub const ZERO: Self = Self(0);

    /// Returns the bank numbered `bank`, or `None` past the last bank.
    pub const fn new(bank: u8) -> Option<Self> {
        if (bank as usize) < BG_PALETTE_BANKS {
            Some(Self(bank))
        } else {
            None
        }
    }

    /// The bank number.
    pub const fn get(self) -> u8 {
        self.0
    }
}

/// A failure to build a [`TileEntry`] from raw bits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum TileEntryError {
    #[error("tile entry {raw:#06x} sets the reserved bit 15")]
    ReservedBit { raw: u16 },
}

/// One cell of a background layer.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct TileEntry(u16);

impl TileEntry {
    /// The empty cell, which is tile 0 with no flags and bank 0.
    pub const EMPTY: Self = Self(0);

    /// The bits an entry can hold. Bit 15 is reserved.
    pub const VALID_BITS: u16 = !RESERVED;

    /// Builds an entry, or `None` if `tile` is not below
    /// [`BG_TILE_CAPACITY`].
    pub const fn new(
        tile: u16,
        flip_x: bool,
        flip_y: bool,
        priority: bool,
        bank: BgBank,
    ) -> Option<Self> {
        if tile as usize >= BG_TILE_CAPACITY {
            return None;
        }
        let mut raw = tile | ((bank.get() as u16) << BANK_SHIFT);
        if flip_x {
            raw |= FLIP_X;
        }
        if flip_y {
            raw |= FLIP_Y;
        }
        if priority {
            raw |= PRIORITY;
        }
        Some(Self(raw))
    }

    /// Reads an entry from its 16-bit value, rejecting the reserved bit.
    pub const fn from_raw(raw: u16) -> Result<Self, TileEntryError> {
        if raw & RESERVED != 0 {
            Err(TileEntryError::ReservedBit { raw })
        } else {
            Ok(Self(raw))
        }
    }

    /// The 16-bit value.
    pub const fn raw(self) -> u16 {
        self.0
    }

    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    pub const fn tile(self) -> u16 {
        self.0 & TILE_MASK
    }

    pub const fn flip_x(self) -> bool {
        self.0 & FLIP_X != 0
    }

    pub const fn flip_y(self) -> bool {
        self.0 & FLIP_Y != 0
    }

    pub const fn priority(self) -> bool {
        self.0 & PRIORITY != 0
    }

    pub const fn bank(self) -> BgBank {
        BgBank(((self.0 >> BANK_SHIFT) & BANK_MASK) as u8)
    }
}

/// A failure to read a layer from its native bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum TileLayerError {
    #[error("a {width}x{height} layer has too many cells")]
    TooLarge { width: usize, height: usize },
    #[error("a {width}x{height} layer must be exactly {expected} bytes, got {length}")]
    Length {
        width: usize,
        height: usize,
        expected: usize,
        length: usize,
    },
    #[error("cell {cell} sets the reserved bit 15 ({raw:#06x})")]
    ReservedBit { cell: usize, raw: u16 },
}

/// A rectangular grid of entries, stored row-major.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TileLayer {
    width: usize,
    height: usize,
    cells: Vec<TileEntry>,
}

impl TileLayer {
    /// A layer of empty cells, or `None` if it would have more cells than
    /// memory can address.
    pub fn empty(width: usize, height: usize) -> Option<Self> {
        let len = Self::byte_len(width, height)? / TILE_ENTRY_BYTES;
        Some(Self {
            width,
            height,
            cells: vec![TileEntry::EMPTY; len],
        })
    }

    /// Reads a layer from exactly `width * height * 2` little-endian bytes.
    pub fn from_bytes(width: usize, height: usize, bytes: &[u8]) -> Result<Self, TileLayerError> {
        let expected =
            Self::byte_len(width, height).ok_or(TileLayerError::TooLarge { width, height })?;
        if bytes.len() != expected {
            return Err(TileLayerError::Length {
                width,
                height,
                expected,
                length: bytes.len(),
            });
        }
        let (pairs, _) = bytes.as_chunks::<TILE_ENTRY_BYTES>();
        let cells = pairs
            .iter()
            .enumerate()
            .map(|(cell, &pair)| {
                let raw = u16::from_le_bytes(pair);
                TileEntry::from_raw(raw).map_err(|_| TileLayerError::ReservedBit { cell, raw })
            })
            .collect::<Result<_, _>>()?;
        Ok(Self {
            width,
            height,
            cells,
        })
    }

    /// The native bytes, with each cell little-endian and rows in order.
    pub fn to_bytes(&self) -> Vec<u8> {
        self.cells
            .iter()
            .flat_map(|cell| cell.raw().to_le_bytes())
            .collect()
    }

    pub fn width(&self) -> usize {
        self.width
    }

    pub fn height(&self) -> usize {
        self.height
    }

    /// Number of cells.
    pub fn cell_count(&self) -> usize {
        self.cells.len()
    }

    /// Whether every cell is the empty cell.
    pub fn is_blank(&self) -> bool {
        self.cells.iter().all(|cell| cell.is_empty())
    }

    /// The flat index of `(x, y)`, or `None` outside the layer.
    pub fn index_of(&self, x: usize, y: usize) -> Option<usize> {
        (x < self.width && y < self.height).then(|| y * self.width + x)
    }

    pub fn get(&self, x: usize, y: usize) -> Option<TileEntry> {
        self.index_of(x, y).map(|index| self.cells[index])
    }

    /// The cell at flat index `index`.
    pub fn get_index(&self, index: usize) -> Option<TileEntry> {
        self.cells.get(index).copied()
    }

    /// Writes `(x, y)` and returns the entry it held, or `None` outside the
    /// layer.
    pub fn set(&mut self, x: usize, y: usize, entry: TileEntry) -> Option<TileEntry> {
        let index = self.index_of(x, y)?;
        self.set_index(index, entry)
    }

    /// Writes flat index `index` and returns the entry it held, or `None`
    /// outside the layer.
    pub fn set_index(&mut self, index: usize, entry: TileEntry) -> Option<TileEntry> {
        self.cells
            .get_mut(index)
            .map(|cell| std::mem::replace(cell, entry))
    }

    fn byte_len(width: usize, height: usize) -> Option<usize> {
        width
            .checked_mul(height)
            .and_then(|cells| cells.checked_mul(TILE_ENTRY_BYTES))
    }
}

#[cfg(test)]
mod tests;
