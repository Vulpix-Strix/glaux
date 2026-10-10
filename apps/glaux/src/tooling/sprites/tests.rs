use std::fs;
use std::io::BufWriter;

use super::*;
use crate::scratch_dir::ScratchDir;
use pretty_assertions::assert_eq;

struct Fixture {
    root: ScratchDir,
}

impl Fixture {
    fn new(name: &str) -> Self {
        Self {
            root: ScratchDir::new(&format!("sprite-convert-{name}")),
        }
    }
}

fn indexed_png(path: &Path, width: u32, height: u32, pixels: &[u8], palette_len: usize) {
    let file = BufWriter::new(File::create(path).unwrap());
    let mut encoder = png::Encoder::new(file, width, height);
    encoder.set_color(ColorType::Indexed);
    encoder.set_depth(BitDepth::Eight);
    encoder.set_palette(vec![0; palette_len * 3]);
    encoder
        .write_header()
        .unwrap()
        .write_image_data(pixels)
        .unwrap();
}

#[test]
fn valid_indexed_png_produces_exact_tile_bytes_at_the_default_path() {
    let fixture = Fixture::new("valid");
    let input = fixture.root.join("sheet.png");
    let row = [0, 1, 2, 3, 4, 5, 14, 15];
    let pixels: Vec<u8> = row.into_iter().cycle().take(64).collect();
    indexed_png(&input, 8, 8, &pixels, 16);

    let summary = convert(&input, None).unwrap();
    let output = fixture.root.join("sprites.bin");
    assert_eq!(summary.output, output);
    assert_eq!(summary.source_tiles, 1);
    assert_eq!(summary.obj_slots, 16);
    assert_eq!(summary.byte_count, 16 * 32);
    let bytes = fs::read(output).unwrap();
    assert_eq!(bytes[..32], [0x01, 0x23, 0x45, 0xef].repeat(8));
    assert!(bytes[32..].iter().all(|&byte| byte == 0));
}

#[test]
fn checked_in_example_native_data_matches_its_source_png() {
    let example = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/sprites");
    let decoder = png::Decoder::new(BufReader::new(
        File::open(example.join("sprites.png")).unwrap(),
    ));
    let mut reader = decoder.read_info().unwrap();
    let info = reader.info();
    assert_eq!(info.color_type, ColorType::Indexed);
    let (width, height) = (info.width as usize, info.height as usize);
    let mut decoded = vec![0; reader.output_buffer_size().unwrap()];
    let frame = reader.next_frame(&mut decoded).unwrap();
    let pixels = unpack_indices(
        &decoded[..frame.buffer_size()],
        width,
        height,
        frame.line_size,
        frame.bit_depth,
    )
    .unwrap();

    let encoded = encode_obj_tiles(width, height, &pixels).unwrap();
    assert_eq!(encoded, fs::read(example.join("sprites.bin")).unwrap());
}

#[test]
fn explicit_output_path_is_honored() {
    let fixture = Fixture::new("explicit");
    let input = fixture.root.join("sheet.png");
    let output = fixture.root.join("custom.bin");
    indexed_png(&input, 8, 8, &[0; 64], 1);

    let summary = convert(&input, Some(&output)).unwrap();
    assert_eq!(summary.output, output);
    assert_eq!(fs::read(output).unwrap(), [0; 16 * 32]);
    assert!(!fixture.root.join("sprites.bin").exists());
}

#[test]
fn dimensions_must_be_divisible_by_eight() {
    let fixture = Fixture::new("dimensions");
    let input = fixture.root.join("sheet.png");
    indexed_png(&input, 9, 8, &[0; 72], 1);

    assert!(
        convert(&input, None)
            .unwrap_err()
            .to_string()
            .contains("dimensions must be divisible by 8")
    );
}

#[test]
fn sheets_larger_than_the_tile_grid_are_rejected() {
    let fixture = Fixture::new("capacity");
    let input = fixture.root.join("sheet.png");
    for (width, height) in [(136, 8), (8, 520)] {
        indexed_png(
            &input,
            width,
            height,
            &vec![0; (width * height) as usize],
            1,
        );
        let error = convert(&input, None).unwrap_err().to_string();
        assert!(error.contains(&format!("{width}x{height}")), "{error}");
        assert!(error.contains("at most 128x512"), "{error}");
    }
}

#[test]
fn high_palette_index_is_rejected_even_with_few_used_colors() {
    let fixture = Fixture::new("index");
    let input = fixture.root.join("sheet.png");
    let mut pixels = vec![0; 64];
    pixels[19] = 37;
    indexed_png(&input, 8, 8, &pixels, 38);

    let error = convert(&input, None).unwrap_err().to_string();
    assert!(error.contains("palette index 37"), "{error}");
    assert!(error.contains("pixel (3, 2)"), "{error}");
}

#[test]
fn true_color_png_is_rejected() {
    let fixture = Fixture::new("rgb");
    let input = fixture.root.join("sheet.png");
    let file = BufWriter::new(File::create(&input).unwrap());
    let mut encoder = png::Encoder::new(file, 8, 8);
    encoder.set_color(ColorType::Rgb);
    encoder.set_depth(BitDepth::Eight);
    encoder
        .write_header()
        .unwrap()
        .write_image_data(&[0; 8 * 8 * 3])
        .unwrap();

    assert!(
        convert(&input, None)
            .unwrap_err()
            .to_string()
            .contains("unsupported PNG color type Rgb")
    );
}
