use std::io::Cursor;

use super::*;
use pretty_assertions::assert_eq;

fn png_bytes(width: u32, height: u32, rgba: &[u8]) -> Vec<u8> {
    let mut bytes = Vec::new();
    let mut encoder = png::Encoder::new(&mut bytes, width, height);
    encoder.set_color(ColorType::Rgba);
    encoder.set_depth(BitDepth::Eight);
    encoder
        .write_header()
        .unwrap()
        .write_image_data(rgba)
        .unwrap();
    bytes
}

#[test]
fn an_image_wider_than_the_tile_grid_is_rejected_as_too_large() {
    let width = OBJ_SHEET_MAX_WIDTH as u32 + 8;
    let bytes = png_bytes(width, 8, &vec![0; width as usize * 8 * 4]);
    let err = decode_png(Cursor::new(bytes)).unwrap_err();
    assert_eq!(
        err.downcast_ref::<FitError>(),
        Some(&FitError::TooLarge {
            width: width as usize,
            height: 8
        })
    );
}

#[test]
fn the_top_left_color_is_only_a_key_when_that_pixel_is_opaque() {
    let image = |first: [u8; 4]| RgbaImage {
        width: 1,
        height: 1,
        rgba: first.to_vec(),
    };
    assert_eq!(
        image([255, 0, 255, 255]).top_left(),
        Some(Rgb::from_hex(0xff00ff))
    );
    assert_eq!(image([255, 0, 255, 0]).top_left(), None);
    assert_eq!(image([255, 0, 255, 128]).top_left(), None);
}

#[test]
fn decodes_rgba_pixels_unchanged() {
    let rgba: Vec<u8> = (0..8 * 8 * 4).map(|n| n as u8 | 1).collect();
    let image = decode_png(Cursor::new(png_bytes(8, 8, &rgba))).unwrap();
    assert_eq!((image.width, image.height), (8, 8));
    assert_eq!(image.rgba, rgba);
}
