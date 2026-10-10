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
fn set_pixel_writes_one_nibble_and_returns_the_old_index() {
    let mut bytes = [0; OBJ_TILE_BYTES];
    bytes[0] = 0x3c;
    let mut bank = ObjTileBank::from_bytes(&bytes).unwrap();

    assert_eq!(bank.set_pixel(0, 0, 0, 0xa), Some(3));
    assert_eq!(bank.as_bytes()[0], 0xac, "the right pixel is kept");
    assert_eq!(bank.set_pixel(0, 1, 0, 0x5), Some(12));
    assert_eq!(bank.as_bytes()[0], 0xa5, "the left pixel is kept");

    assert_eq!(bank.set_pixel(OBJ_TILE_CAPACITY - 1, 7, 7, 15), Some(0));
    assert_eq!(bank.pixel(OBJ_TILE_CAPACITY - 1, 7, 7), Some(15));
    assert_eq!(bank.as_bytes()[OBJ_TILE_BANK_BYTES - 1], 0x0f);
}

#[test]
fn set_pixel_rejects_invalid_input_without_writing() {
    let mut bank = ObjTileBank::new();
    assert_eq!(bank.set_pixel(OBJ_TILE_CAPACITY, 0, 0, 1), None);
    assert_eq!(bank.set_pixel(0, OBJ_TILE_WIDTH, 0, 1), None);
    assert_eq!(bank.set_pixel(0, 0, OBJ_TILE_HEIGHT, 1), None);
    assert_eq!(bank.set_pixel(0, 0, 0, 16), None);
    assert_eq!(bank, ObjTileBank::new());
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

/// A sheet of solid tiles; tile `(tx, ty)` uses index `values[ty][tx]`.
fn solid_sheet(values: &[&[u8]]) -> (usize, usize, Vec<u8>) {
    let (width, height) = (values[0].len() * 8, values.len() * 8);
    let mut pixels = vec![0; width * height];
    for (offset, pixel) in pixels.iter_mut().enumerate() {
        *pixel = values[offset / width / 8][offset % width / 8];
    }
    (width, height, pixels)
}

#[test]
fn packs_an_exact_tile_with_left_pixel_in_the_high_nibble() {
    let row = [0, 1, 2, 3, 4, 5, 14, 15];
    let pixels: Vec<u8> = row.into_iter().cycle().take(64).collect();

    let encoded = encode_obj_tiles(8, 8, &pixels).unwrap();
    assert_eq!(
        encoded[..OBJ_TILE_BYTES],
        [0x01, 0x23, 0x45, 0xef].repeat(8)
    );
}

#[test]
fn places_sheet_tiles_on_the_grid_and_pads_each_row() {
    let (width, height, pixels) = solid_sheet(&[&[1, 2], &[3, 4]]);

    let encoded = encode_obj_tiles(width, height, &pixels).unwrap();
    assert_eq!(encoded.len(), 2 * OBJ_GRID_COLUMNS * OBJ_TILE_BYTES);
    let (tiles, remainder) = encoded.as_chunks::<OBJ_TILE_BYTES>();
    assert!(remainder.is_empty());
    for (index, tile) in tiles.iter().enumerate() {
        let expected = match index {
            0 => 0x11,
            1 => 0x22,
            16 => 0x33,
            17 => 0x44,
            _ => 0,
        };
        assert_eq!(*tile, [expected; OBJ_TILE_BYTES], "tile {index}");
    }
}

#[test]
fn a_full_width_sheet_keeps_packed_row_major_order() {
    let row: Vec<u8> = (0..16).map(|n| n as u8).collect();
    let (width, height, pixels) = solid_sheet(&[&row, &row]);

    let encoded = encode_obj_tiles(width, height, &pixels).unwrap();
    let (tiles, _) = encoded.as_chunks::<OBJ_TILE_BYTES>();
    assert_eq!(tiles.len(), 32);
    for (index, tile) in tiles.iter().enumerate() {
        let value = (index % 16) as u8;
        assert_eq!(*tile, [(value << 4) | value; OBJ_TILE_BYTES]);
    }
}

#[test]
fn rejects_invalid_dimensions_and_sheets_larger_than_the_grid() {
    assert_eq!(
        encode_obj_tiles(9, 8, &[0; 72]),
        Err(ObjTileEncodingError::InvalidDimensions {
            width: 9,
            height: 8
        })
    );
    assert_eq!(
        encode_obj_tiles(17 * 8, 8, &vec![0; 17 * 64]),
        Err(ObjTileEncodingError::SheetTooWide {
            width: 136,
            maximum: 128
        })
    );
    assert_eq!(
        encode_obj_tiles(8, 65 * 8, &vec![0; 65 * 64]),
        Err(ObjTileEncodingError::SheetTooTall {
            height: 520,
            maximum: 512
        })
    );
    let full = encode_obj_tiles(128, 512, &vec![0; 128 * 512]).unwrap();
    assert_eq!(full.len(), OBJ_TILE_BANK_BYTES);
}

#[test]
fn grid_tiles_never_wrap_across_rows_or_leave_the_bank() {
    assert_eq!(obj_grid_tile(24, 0, 0), Some(24));
    assert_eq!(obj_grid_tile(24, 1, 1), Some(41));
    assert_eq!(obj_grid_tile(15, 0, 0), Some(15));
    assert_eq!(obj_grid_tile(15, 1, 0), None);
    assert_eq!(obj_grid_tile(1008, 15, 0), Some(1023));
    assert_eq!(obj_grid_tile(1023, 0, 1), None);
    assert_eq!(obj_grid_tile(OBJ_TILE_CAPACITY, 0, 0), None);
    assert_eq!(obj_grid_tile(0, usize::MAX, usize::MAX), None);
}

#[test]
fn short_attribute_data_defaults_remaining_tiles_to_bank_zero() {
    let attributes = ObjTileAttributes::from_bytes(&[3, 0, 7]).unwrap();
    assert_eq!(attributes.bank(0).get(), 3);
    assert_eq!(attributes.bank(2).get(), 7);
    assert_eq!(attributes.bank(3), ObjBank::ZERO);
    assert_eq!(attributes.bank(OBJ_TILE_CAPACITY), ObjBank::ZERO);
}

#[test]
fn set_bank_writes_one_tile_in_native_form() {
    let mut attributes = ObjTileAttributes::from_bytes(&[3, 5]).unwrap();
    let seven = ObjBank::new(7).unwrap();
    assert_eq!(attributes.set_bank(1, seven), ObjBank::new(5));
    assert_eq!(
        attributes.set_bank(OBJ_TILE_CAPACITY - 1, seven),
        Some(ObjBank::ZERO)
    );
    assert_eq!(attributes.as_bytes()[..3], [3, 7, 0]);
    assert_eq!(attributes.as_bytes()[OBJ_TILE_CAPACITY - 1], 7);

    let before = attributes.clone();
    assert_eq!(attributes.set_bank(OBJ_TILE_CAPACITY, seven), None);
    assert_eq!(attributes, before);
    assert_eq!(
        ObjTileAttributes::from_bytes(attributes.as_bytes()),
        Ok(attributes)
    );
}

#[test]
fn attributes_reject_reserved_bits_and_oversized_data() {
    assert_eq!(
        ObjTileAttributes::from_bytes(&[1, 0x09]),
        Err(ObjTileAttributesError::ReservedBits {
            tile: 1,
            value: 0x09
        })
    );
    assert_eq!(
        ObjTileAttributes::from_bytes(&vec![0; OBJ_TILE_ATTRIBUTES_BYTES + 1]),
        Err(ObjTileAttributesError::TooLarge {
            length: OBJ_TILE_ATTRIBUTES_BYTES + 1
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
