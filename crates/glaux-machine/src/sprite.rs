//! Native G1 object-sprite tile encoding.

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

/// A failure to encode indexed pixels as native G1 object tiles.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ObjTileEncodingError {
    #[error("sprite sheet dimensions must be divisible by 8, got {width}x{height}")]
    InvalidDimensions { width: usize, height: usize },
    #[error("sprite sheet dimensions are too large: {width}x{height}")]
    DimensionsTooLarge { width: usize, height: usize },
    #[error("expected {expected} pixels for a {width}x{height} sprite sheet, got {actual}")]
    PixelCount {
        width: usize,
        height: usize,
        expected: usize,
        actual: usize,
    },
    #[error("sprite sheet contains {tiles} tiles; G1 supports at most {maximum}")]
    TooManyTiles { tiles: usize, maximum: usize },
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

/// Encodes a row-major indexed spritesheet as consecutive G1 4 bpp object tiles.
///
/// The dimensions must be divisible by 8 and `pixels` must contain exactly
/// `width * height` palette indices. Tiles are emitted left-to-right and then
/// top-to-bottom. Within a tile, the left pixel occupies the high nibble.
pub fn encode_obj_tiles(
    width: usize,
    height: usize,
    pixels: &[u8],
) -> Result<Vec<u8>, ObjTileEncodingError> {
    if !width.is_multiple_of(OBJ_TILE_WIDTH) || !height.is_multiple_of(OBJ_TILE_HEIGHT) {
        return Err(ObjTileEncodingError::InvalidDimensions { width, height });
    }
    let expected = width
        .checked_mul(height)
        .ok_or(ObjTileEncodingError::DimensionsTooLarge { width, height })?;
    if pixels.len() != expected {
        return Err(ObjTileEncodingError::PixelCount {
            width,
            height,
            expected,
            actual: pixels.len(),
        });
    }

    let tiles_x = width / OBJ_TILE_WIDTH;
    let tiles_y = height / OBJ_TILE_HEIGHT;
    let tiles = tiles_x
        .checked_mul(tiles_y)
        .ok_or(ObjTileEncodingError::DimensionsTooLarge { width, height })?;
    if tiles > OBJ_TILE_CAPACITY {
        return Err(ObjTileEncodingError::TooManyTiles {
            tiles,
            maximum: OBJ_TILE_CAPACITY,
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

    let mut encoded = Vec::with_capacity(tiles * OBJ_TILE_BYTES);
    for tile_y in 0..tiles_y {
        for tile_x in 0..tiles_x {
            for row in 0..OBJ_TILE_HEIGHT {
                let y = tile_y * OBJ_TILE_HEIGHT + row;
                for pair in 0..OBJ_TILE_WIDTH / 2 {
                    let x = tile_x * OBJ_TILE_WIDTH + pair * 2;
                    let left = pixels[y * width + x];
                    let right = pixels[y * width + x + 1];
                    encoded.push((left << 4) | right);
                }
            }
        }
    }
    Ok(encoded)
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn an_empty_bank_is_all_zero() {
        let bank = ObjTileBank::new();
        assert!(bank.as_bytes().iter().all(|&byte| byte == 0));
        assert_eq!(bank.pixel(0, 0, 0), Some(0));
        assert_eq!(bank.pixel(OBJ_TILE_CAPACITY - 1, 7, 7), Some(0));
    }

    #[test]
    fn loading_tiles_preserves_bytes_and_zeroes_unused_capacity() {
        let bytes = [0x01, 0x23, 0x45, 0x67]
            .into_iter()
            .cycle()
            .take(OBJ_TILE_BYTES * 2)
            .collect::<Vec<_>>();
        let bank = ObjTileBank::from_bytes(&bytes).unwrap();

        assert_eq!(&bank.as_bytes()[..bytes.len()], bytes);
        assert!(bank.as_bytes()[bytes.len()..].iter().all(|&byte| byte == 0));
    }

    #[test]
    fn decodes_left_high_and_right_low_nibbles() {
        let mut bytes = [0; OBJ_TILE_BYTES];
        bytes[0] = 0x3c;
        bytes[OBJ_TILE_BYTES - 1] = 0xe7;
        let bank = ObjTileBank::from_bytes(&bytes).unwrap();

        assert_eq!(bank.pixel(0, 0, 0), Some(3));
        assert_eq!(bank.pixel(0, 1, 0), Some(12));
        assert_eq!(bank.pixel(0, 6, 7), Some(14));
        assert_eq!(bank.pixel(0, 7, 7), Some(7));
    }

    #[test]
    fn reads_first_and_last_tiles() {
        let mut bytes = vec![0; OBJ_TILE_BANK_BYTES];
        bytes[0] = 0x10;
        bytes[OBJ_TILE_BANK_BYTES - 1] = 0x0f;
        let bank = ObjTileBank::from_bytes(&bytes).unwrap();

        assert_eq!(bank.pixel(0, 0, 0), Some(1));
        assert_eq!(bank.pixel(OBJ_TILE_CAPACITY - 1, 7, 7), Some(15));
    }

    #[test]
    fn invalid_tile_coordinates_return_none() {
        let bank = ObjTileBank::new();
        assert_eq!(bank.pixel(OBJ_TILE_CAPACITY, 0, 0), None);
        assert_eq!(bank.pixel(0, OBJ_TILE_WIDTH, 0), None);
        assert_eq!(bank.pixel(0, 0, OBJ_TILE_HEIGHT), None);
    }

    #[test]
    fn rejects_misaligned_and_oversized_data() {
        assert_eq!(
            ObjTileBank::from_bytes(&[0; OBJ_TILE_BYTES - 1]),
            Err(ObjTileBankError::Misaligned {
                length: OBJ_TILE_BYTES - 1
            })
        );
        assert_eq!(
            ObjTileBank::from_bytes(&vec![0; OBJ_TILE_BANK_BYTES + 1]),
            Err(ObjTileBankError::TooLarge {
                length: OBJ_TILE_BANK_BYTES + 1
            })
        );
    }

    #[test]
    fn packs_an_exact_tile_with_left_pixel_in_the_high_nibble() {
        let row = [0, 1, 2, 3, 4, 5, 14, 15];
        let pixels: Vec<u8> = row.into_iter().cycle().take(64).collect();

        assert_eq!(
            encode_obj_tiles(8, 8, &pixels).unwrap(),
            [0x01, 0x23, 0x45, 0xef].repeat(8)
        );
    }

    #[test]
    fn emits_tiles_in_spritesheet_row_major_order() {
        let mut pixels = vec![0; 16 * 16];
        for tile_y in 0..2 {
            for tile_x in 0..2 {
                let value = (tile_y * 2 + tile_x + 1) as u8;
                for y in 0..8 {
                    for x in 0..8 {
                        pixels[(tile_y * 8 + y) * 16 + tile_x * 8 + x] = value;
                    }
                }
            }
        }

        let encoded = encode_obj_tiles(16, 16, &pixels).unwrap();
        let (tiles, remainder) = encoded.as_chunks::<32>();
        assert!(remainder.is_empty());
        for (tile, value) in tiles.iter().copied().zip([0x11, 0x22, 0x33, 0x44]) {
            assert_eq!(tile, [value; 32]);
        }
    }

    #[test]
    fn rejects_invalid_dimensions_and_too_many_tiles() {
        assert_eq!(
            encode_obj_tiles(9, 8, &[0; 72]),
            Err(ObjTileEncodingError::InvalidDimensions {
                width: 9,
                height: 8
            })
        );
        assert_eq!(
            encode_obj_tiles(1025 * 8, 8, &vec![0; 1025 * 64]),
            Err(ObjTileEncodingError::TooManyTiles {
                tiles: 1025,
                maximum: 1024
            })
        );
    }

    #[test]
    fn rejects_wrong_pixel_count() {
        assert_eq!(
            encode_obj_tiles(8, 8, &[0; 63]),
            Err(ObjTileEncodingError::PixelCount {
                width: 8,
                height: 8,
                expected: 64,
                actual: 63
            })
        );
    }

    #[test]
    fn reports_the_invalid_palette_index_position() {
        let mut pixels = vec![0; 16 * 8];
        pixels[3 * 16 + 10] = 37;
        assert_eq!(
            encode_obj_tiles(16, 8, &pixels),
            Err(ObjTileEncodingError::PaletteIndex {
                index: 37,
                x: 10,
                y: 3,
                tile_x: 1,
                tile_y: 0
            })
        );
    }
}
