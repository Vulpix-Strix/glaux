//! Indexed PNG to native G1 object-tile importing.

use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};

use anyhow::{Context as _, bail};
use glaux_machine::sprite::{OBJ_TILE_CAPACITY, OBJ_TILE_HEIGHT, OBJ_TILE_WIDTH, encode_obj_tiles};
use png::{BitDepth, ColorType};

/// Facts reported after a successful sprite import.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ImportSummary {
    pub(super) output: PathBuf,
    pub(super) tile_count: usize,
    pub(super) byte_count: usize,
}

/// Imports an indexed PNG spritesheet into raw G1 4 bpp object tiles.
pub(super) fn import(input: &Path, output: Option<&Path>) -> anyhow::Result<ImportSummary> {
    let file = File::open(input)
        .with_context(|| format!("failed to open input PNG {}", input.display()))?;
    let decoder = png::Decoder::new(BufReader::new(file));
    let mut reader = decoder
        .read_info()
        .with_context(|| format!("failed to read PNG {}", input.display()))?;

    let header = reader.info();
    let source_width = header.width;
    let source_height = header.height;
    if header.color_type != ColorType::Indexed {
        bail!(
            "unsupported PNG color type {:?}; expected indexed",
            header.color_type
        );
    }
    if header.animation_control.is_some() {
        bail!("animated PNG is not supported; expected a static indexed PNG");
    }

    let width = usize::try_from(source_width).context("PNG width does not fit this platform")?;
    let height = usize::try_from(source_height).context("PNG height does not fit this platform")?;
    validate_dimensions(width, height)?;

    let size = reader
        .output_buffer_size()
        .context("decoded PNG is too large for this platform")?;
    let mut decoded = vec![0; size];
    let info = reader
        .next_frame(&mut decoded)
        .with_context(|| format!("failed to decode PNG {}", input.display()))?;
    if info.width != source_width || info.height != source_height {
        bail!("animated PNG subframes are not supported");
    }
    let pixels = unpack_indices(
        &decoded[..info.buffer_size()],
        width,
        height,
        info.line_size,
        info.bit_depth,
    )?;
    let encoded = encode_obj_tiles(width, height, &pixels)?;

    let output = output.map_or_else(|| default_output_path(input), Path::to_path_buf);
    std::fs::write(&output, &encoded)
        .with_context(|| format!("failed to write {}", output.display()))?;
    Ok(ImportSummary {
        output,
        tile_count: (width / OBJ_TILE_WIDTH) * (height / OBJ_TILE_HEIGHT),
        byte_count: encoded.len(),
    })
}

fn validate_dimensions(width: usize, height: usize) -> anyhow::Result<()> {
    if !width.is_multiple_of(OBJ_TILE_WIDTH) || !height.is_multiple_of(OBJ_TILE_HEIGHT) {
        bail!("sprite sheet dimensions must be divisible by 8, got {width}x{height}");
    }
    let tiles = (width / OBJ_TILE_WIDTH)
        .checked_mul(height / OBJ_TILE_HEIGHT)
        .context("sprite sheet dimensions are too large")?;
    if tiles > OBJ_TILE_CAPACITY {
        bail!("sprite sheet contains {tiles} tiles; G1 supports at most {OBJ_TILE_CAPACITY}");
    }
    Ok(())
}

/// Expands PNG's packed indexed samples without resolving them through PLTE.
fn unpack_indices(
    bytes: &[u8],
    width: usize,
    height: usize,
    line_size: usize,
    depth: BitDepth,
) -> anyhow::Result<Vec<u8>> {
    let bits = match depth {
        BitDepth::One => 1,
        BitDepth::Two => 2,
        BitDepth::Four => 4,
        BitDepth::Eight => 8,
        BitDepth::Sixteen => bail!("unsupported indexed PNG bit depth: 16"),
    };
    let mut pixels = Vec::with_capacity(width * height);
    for y in 0..height {
        let row = &bytes[y * line_size..(y + 1) * line_size];
        for x in 0..width {
            let bit = x * bits;
            let shift = 8 - bits - bit % 8;
            let mask = if bits == 8 {
                u8::MAX
            } else {
                (1_u8 << bits) - 1
            };
            pixels.push((row[bit / 8] >> shift) & mask);
        }
    }
    Ok(pixels)
}

fn default_output_path(input: &Path) -> PathBuf {
    input
        .parent()
        .unwrap_or_else(|| Path::new(""))
        .join("sprites.bin")
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::io::BufWriter;

    use super::*;
    use pretty_assertions::assert_eq;

    struct Fixture {
        root: PathBuf,
    }

    impl Fixture {
        fn new(name: &str) -> Self {
            let root = std::env::temp_dir().join(format!(
                "glaux-test-{}-sprite-import-{name}",
                std::process::id()
            ));
            let _ = fs::remove_dir_all(&root);
            fs::create_dir_all(&root).unwrap();
            Self { root }
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
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

        let summary = import(&input, None).unwrap();
        let output = fixture.root.join("sprites.bin");
        assert_eq!(summary.output, output);
        assert_eq!(summary.tile_count, 1);
        assert_eq!(summary.byte_count, 32);
        assert_eq!(
            fs::read(output).unwrap(),
            [0x01, 0x23, 0x45, 0xef].repeat(8)
        );
    }

    #[test]
    fn explicit_output_path_is_honored() {
        let fixture = Fixture::new("explicit");
        let input = fixture.root.join("sheet.png");
        let output = fixture.root.join("custom.bin");
        indexed_png(&input, 8, 8, &[0; 64], 1);

        let summary = import(&input, Some(&output)).unwrap();
        assert_eq!(summary.output, output);
        assert_eq!(fs::read(output).unwrap(), [0; 32]);
        assert!(!fixture.root.join("sprites.bin").exists());
    }

    #[test]
    fn dimensions_must_be_divisible_by_eight() {
        let fixture = Fixture::new("dimensions");
        let input = fixture.root.join("sheet.png");
        indexed_png(&input, 9, 8, &[0; 72], 1);

        assert!(
            import(&input, None)
                .unwrap_err()
                .to_string()
                .contains("dimensions must be divisible by 8")
        );
    }

    #[test]
    fn more_than_1024_tiles_is_rejected() {
        let fixture = Fixture::new("capacity");
        let input = fixture.root.join("sheet.png");
        indexed_png(&input, 1025 * 8, 8, &vec![0; 1025 * 64], 1);

        assert!(
            import(&input, None)
                .unwrap_err()
                .to_string()
                .contains("1025 tiles")
        );
    }

    #[test]
    fn high_palette_index_is_rejected_even_with_few_used_colors() {
        let fixture = Fixture::new("index");
        let input = fixture.root.join("sheet.png");
        let mut pixels = vec![0; 64];
        pixels[19] = 37;
        indexed_png(&input, 8, 8, &pixels, 38);

        let error = import(&input, None).unwrap_err().to_string();
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
            import(&input, None)
                .unwrap_err()
                .to_string()
                .contains("unsupported PNG color type Rgb")
        );
    }
}
