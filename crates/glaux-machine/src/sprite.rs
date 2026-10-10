//! Native G1 object-sprite tile encoding.

use crate::obj_palette::{ObjBank, ObjPalettes};

/// Width of one object tile in pixels.
pub const OBJ_TILE_WIDTH: usize = 8;
/// Height of one object tile in pixels.
pub const OBJ_TILE_HEIGHT: usize = 8;
/// Encoded size of one 4 bpp object tile.
pub const OBJ_TILE_BYTES: usize = 32;
/// Maximum number of object tiles addressable by G1.
pub const OBJ_TILE_CAPACITY: usize = 1024;
/// Size of the complete native object-tile bank in bytes.
pub const OBJ_TILE_BANK_BYTES: usize = OBJ_TILE_CAPACITY * OBJ_TILE_BYTES;
/// Columns of the logical G1 object-tile grid.
///
/// Tile `n` sits at column `n % OBJ_GRID_COLUMNS` and row
/// `n / OBJ_GRID_COLUMNS`. Converted sheets are placed on this grid, and
/// multi-tile sprites are composed from it.
pub const OBJ_GRID_COLUMNS: usize = 16;
/// Rows of the logical G1 object-tile grid.
pub const OBJ_GRID_ROWS: usize = OBJ_TILE_CAPACITY / OBJ_GRID_COLUMNS;

/// Returns the tile `column` tiles right of and `row` tiles below `first`
/// on the object-tile grid.
///
/// `None` means the position falls past the right edge of `first`'s grid
/// row or below the last grid row: compositions never wrap across rows.
pub fn obj_grid_tile(first: usize, column: usize, row: usize) -> Option<usize> {
    let grid_column = (first % OBJ_GRID_COLUMNS).checked_add(column)?;
    let grid_row = (first / OBJ_GRID_COLUMNS).checked_add(row)?;
    (grid_column < OBJ_GRID_COLUMNS && grid_row < OBJ_GRID_ROWS)
        .then(|| grid_row * OBJ_GRID_COLUMNS + grid_column)
}

/// A failure to construct an object-tile bank from native G1 bytes.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ObjTileBankError {
    #[error("object tile data length must be a multiple of {OBJ_TILE_BYTES} bytes, got {length}")]
    Misaligned { length: usize },
    #[error("object tile data is too large: got {length} bytes, maximum is {OBJ_TILE_BANK_BYTES}")]
    TooLarge { length: usize },
}

/// The native G1 object-tile bank.
///
/// The bank stores 1,024 consecutive 8x8, 4 bpp tiles in their encoded
/// byte representation. Bytes beyond loaded tile data remain zeroed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ObjTileBank {
    bytes: Box<[u8; OBJ_TILE_BANK_BYTES]>,
}

impl ObjTileBank {
    /// Creates an object-tile bank containing only transparent pixels.
    pub fn new() -> Self {
        Self {
            bytes: Box::new([0; OBJ_TILE_BANK_BYTES]),
        }
    }

    /// Loads consecutive native object tiles at the beginning of a new bank.
    ///
    /// `bytes` must contain whole tiles and fit within the 32 KiB bank.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, ObjTileBankError> {
        if bytes.len() > OBJ_TILE_BANK_BYTES {
            return Err(ObjTileBankError::TooLarge {
                length: bytes.len(),
            });
        }
        if !bytes.len().is_multiple_of(OBJ_TILE_BYTES) {
            return Err(ObjTileBankError::Misaligned {
                length: bytes.len(),
            });
        }

        let mut bank = Self::new();
        bank.bytes[..bytes.len()].copy_from_slice(bytes);
        Ok(bank)
    }

    /// Returns one tile-local palette index.
    ///
    /// `None` means the tile index or local coordinate is invalid. A valid
    /// transparent pixel is returned as `Some(0)`.
    pub fn pixel(&self, tile_index: usize, x: usize, y: usize) -> Option<u8> {
        if tile_index >= OBJ_TILE_CAPACITY || x >= OBJ_TILE_WIDTH || y >= OBJ_TILE_HEIGHT {
            return None;
        }
        let byte = self.bytes[tile_index * OBJ_TILE_BYTES + y * (OBJ_TILE_WIDTH / 2) + x / 2];
        Some(if x.is_multiple_of(2) {
            byte >> 4
        } else {
            byte & 0x0f
        })
    }

    /// Writes one tile-local palette index and returns the index it
    /// replaced.
    ///
    /// `None` means the tile index, local coordinate or palette index
    /// (above 15) is invalid; nothing is written then.
    pub fn set_pixel(&mut self, tile_index: usize, x: usize, y: usize, index: u8) -> Option<u8> {
        if index > 0x0f {
            return None;
        }
        let previous = self.pixel(tile_index, x, y)?;
        let byte = &mut self.bytes[tile_index * OBJ_TILE_BYTES + y * (OBJ_TILE_WIDTH / 2) + x / 2];
        *byte = if x.is_multiple_of(2) {
            (*byte & 0x0f) | (index << 4)
        } else {
            (*byte & 0xf0) | index
        };
        Some(previous)
    }

    /// Returns the complete native byte representation of the bank.
    pub fn as_bytes(&self) -> &[u8; OBJ_TILE_BANK_BYTES] {
        &self.bytes
    }
}

impl Default for ObjTileBank {
    fn default() -> Self {
        Self::new()
    }
}

/// Size of the complete native object-tile attribute table in bytes.
pub const OBJ_TILE_ATTRIBUTES_BYTES: usize = OBJ_TILE_CAPACITY;

/// Attribute bits that select a tile's default object palette bank.
const ATTRIBUTE_BANK_MASK: u8 = 0b0000_0111;

/// A failure to construct object-tile attributes from native G1 bytes.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ObjTileAttributesError {
    #[error(
        "object tile attribute data is too large: got {length} bytes, maximum is {OBJ_TILE_ATTRIBUTES_BYTES}"
    )]
    TooLarge { length: usize },
    #[error(
        "object tile {tile} attribute {value:#04x} sets reserved bits; only bits 0-2 are defined"
    )]
    ReservedBits { tile: usize, value: u8 },
}

/// Per-tile attributes of the object-tile bank: one byte per tile.
///
/// Bits 0–2 select the object palette bank a tile is drawn with when the
/// draw call does not choose one. Bits 3–7 are reserved and always 0.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ObjTileAttributes {
    bytes: Box<[u8; OBJ_TILE_ATTRIBUTES_BYTES]>,
}

impl ObjTileAttributes {
    /// Attributes with every tile defaulting to OBP0.
    pub fn new() -> Self {
        Self {
            bytes: Box::new([0; OBJ_TILE_ATTRIBUTES_BYTES]),
        }
    }

    /// Loads attributes for the first `bytes.len()` tiles; the remaining
    /// tiles default to OBP0.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, ObjTileAttributesError> {
        if bytes.len() > OBJ_TILE_ATTRIBUTES_BYTES {
            return Err(ObjTileAttributesError::TooLarge {
                length: bytes.len(),
            });
        }
        if let Some((tile, &value)) = bytes
            .iter()
            .enumerate()
            .find(|&(_, &value)| value & !ATTRIBUTE_BANK_MASK != 0)
        {
            return Err(ObjTileAttributesError::ReservedBits { tile, value });
        }
        let mut attributes = Self::new();
        attributes.bytes[..bytes.len()].copy_from_slice(bytes);
        Ok(attributes)
    }

    /// The default palette bank of `tile`; OBP0 for tiles past the bank.
    pub fn bank(&self, tile: usize) -> ObjBank {
        self.bytes
            .get(tile)
            .and_then(|&value| ObjBank::new(value & ATTRIBUTE_BANK_MASK))
            .unwrap_or(ObjBank::ZERO)
    }

    /// Returns the complete native byte representation of the attributes.
    pub fn as_bytes(&self) -> &[u8; OBJ_TILE_ATTRIBUTES_BYTES] {
        &self.bytes
    }
}

impl Default for ObjTileAttributes {
    fn default() -> Self {
        Self::new()
    }
}

/// The object-related video state of a cartridge: tiles, their attributes
/// and the object palette banks they resolve through.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ObjVram {
    tiles: ObjTileBank,
    attributes: ObjTileAttributes,
    palettes: ObjPalettes,
}

impl ObjVram {
    pub fn new(tiles: ObjTileBank, attributes: ObjTileAttributes, palettes: ObjPalettes) -> Self {
        Self {
            tiles,
            attributes,
            palettes,
        }
    }

    pub fn tiles(&self) -> &ObjTileBank {
        &self.tiles
    }

    /// The tile bank, for writing pixels. Attributes and palettes have no
    /// mutable access.
    pub fn tiles_mut(&mut self) -> &mut ObjTileBank {
        &mut self.tiles
    }

    pub fn attributes(&self) -> &ObjTileAttributes {
        &self.attributes
    }

    pub fn palettes(&self) -> &ObjPalettes {
        &self.palettes
    }
}

/// A failure to encode indexed pixels as native G1 object tiles.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ObjTileEncodingError {
    #[error("sprite sheet dimensions must be divisible by 8, got {width}x{height}")]
    InvalidDimensions { width: usize, height: usize },
    #[error("sprite sheet is {width} pixels wide; the G1 object-tile grid is at most {maximum}")]
    SheetTooWide { width: usize, maximum: usize },
    #[error("sprite sheet is {height} pixels tall; the G1 object-tile grid is at most {maximum}")]
    SheetTooTall { height: usize, maximum: usize },
    #[error("expected {expected} pixels for a {width}x{height} sprite sheet, got {actual}")]
    PixelCount {
        width: usize,
        height: usize,
        expected: usize,
        actual: usize,
    },
    #[error(
        "palette index {index} at pixel ({x}, {y}), tile ({tile_x}, {tile_y}), is outside 0..=15"
    )]
    PaletteIndex {
        index: u8,
        x: usize,
        y: usize,
        tile_x: usize,
        tile_y: usize,
    },
}

/// Largest sheet width the object-tile grid can hold, in pixels.
pub const OBJ_SHEET_MAX_WIDTH: usize = OBJ_GRID_COLUMNS * OBJ_TILE_WIDTH;
/// Largest sheet height the object-tile grid can hold, in pixels.
pub const OBJ_SHEET_MAX_HEIGHT: usize = OBJ_GRID_ROWS * OBJ_TILE_HEIGHT;

/// Encodes a row-major indexed spritesheet as native G1 4 bpp object tiles.
///
/// The dimensions must be divisible by 8 and fit within
/// [`OBJ_SHEET_MAX_WIDTH`]x[`OBJ_SHEET_MAX_HEIGHT`], and `pixels` must
/// contain exactly `width * height` palette indices. The sheet tile at
/// `(tile_x, tile_y)` becomes object tile `tile_y * OBJ_GRID_COLUMNS +
/// tile_x`; each sheet row is padded with transparent tiles to a whole grid
/// row, so the output always holds whole grid rows. Within a tile, the left
/// pixel occupies the high nibble.
pub fn encode_obj_tiles(
    width: usize,
    height: usize,
    pixels: &[u8],
) -> Result<Vec<u8>, ObjTileEncodingError> {
    if !width.is_multiple_of(OBJ_TILE_WIDTH) || !height.is_multiple_of(OBJ_TILE_HEIGHT) {
        return Err(ObjTileEncodingError::InvalidDimensions { width, height });
    }
    if width > OBJ_SHEET_MAX_WIDTH {
        return Err(ObjTileEncodingError::SheetTooWide {
            width,
            maximum: OBJ_SHEET_MAX_WIDTH,
        });
    }
    if height > OBJ_SHEET_MAX_HEIGHT {
        return Err(ObjTileEncodingError::SheetTooTall {
            height,
            maximum: OBJ_SHEET_MAX_HEIGHT,
        });
    }
    let expected = width * height;
    if pixels.len() != expected {
        return Err(ObjTileEncodingError::PixelCount {
            width,
            height,
            expected,
            actual: pixels.len(),
        });
    }

    for (offset, &index) in pixels.iter().enumerate() {
        if index > 15 {
            let x = offset % width;
            let y = offset / width;
            return Err(ObjTileEncodingError::PaletteIndex {
                index,
                x,
                y,
                tile_x: x / OBJ_TILE_WIDTH,
                tile_y: y / OBJ_TILE_HEIGHT,
            });
        }
    }

    let tiles_x = width / OBJ_TILE_WIDTH;
    let tiles_y = height / OBJ_TILE_HEIGHT;
    let mut encoded = vec![0; tiles_y * OBJ_GRID_COLUMNS * OBJ_TILE_BYTES];
    for tile_y in 0..tiles_y {
        for tile_x in 0..tiles_x {
            let tile = tile_y * OBJ_GRID_COLUMNS + tile_x;
            let out = &mut encoded[tile * OBJ_TILE_BYTES..(tile + 1) * OBJ_TILE_BYTES];
            for row in 0..OBJ_TILE_HEIGHT {
                let y = tile_y * OBJ_TILE_HEIGHT + row;
                for pair in 0..OBJ_TILE_WIDTH / 2 {
                    let x = tile_x * OBJ_TILE_WIDTH + pair * 2;
                    let left = pixels[y * width + x];
                    let right = pixels[y * width + x + 1];
                    out[row * (OBJ_TILE_WIDTH / 2) + pair] = (left << 4) | right;
                }
            }
        }
    }
    Ok(encoded)
}

#[cfg(test)]
mod tests;
