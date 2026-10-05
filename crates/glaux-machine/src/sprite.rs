//! Native G1 object-sprite tile encoding.

/// Width of one object tile in pixels.
pub const OBJ_TILE_WIDTH: usize = 8;
/// Height of one object tile in pixels.
pub const OBJ_TILE_HEIGHT: usize = 8;
/// Encoded size of one 4 bpp object tile.
pub const OBJ_TILE_BYTES: usize = 32;
/// Maximum number of object tiles addressable by G1.
pub const OBJ_TILE_CAPACITY: usize = 1024;

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
