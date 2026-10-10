//! Indexed PNG to native G1 object-tile conversion.

use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};

use anyhow::{Context as _, bail};
use glaux_machine::sprite::{
    OBJ_SHEET_MAX_HEIGHT, OBJ_SHEET_MAX_WIDTH, OBJ_TILE_BYTES, OBJ_TILE_HEIGHT, OBJ_TILE_WIDTH,
    encode_obj_tiles,
};
use png::{BitDepth, ColorType};

/// Facts reported after a successful sprite conversion.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ConversionSummary {
    pub(super) output: PathBuf,
    /// Tiles in the source sheet.
    pub(super) source_tiles: usize,
    /// Object-tile slots written, including the transparent tiles that pad
    /// each sheet row to a whole grid row.
    pub(super) obj_slots: usize,
    pub(super) byte_count: usize,
}

/// Converts an indexed PNG spritesheet into raw G1 4 bpp object tiles.
pub(super) fn convert(input: &Path, output: Option<&Path>) -> anyhow::Result<ConversionSummary> {
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
    Ok(ConversionSummary {
        output,
        source_tiles: (width / OBJ_TILE_WIDTH) * (height / OBJ_TILE_HEIGHT),
        obj_slots: encoded.len() / OBJ_TILE_BYTES,
        byte_count: encoded.len(),
    })
}

fn validate_dimensions(width: usize, height: usize) -> anyhow::Result<()> {
    if !width.is_multiple_of(OBJ_TILE_WIDTH) || !height.is_multiple_of(OBJ_TILE_HEIGHT) {
        bail!("sprite sheet dimensions must be divisible by 8, got {width}x{height}");
    }
    if width > OBJ_SHEET_MAX_WIDTH || height > OBJ_SHEET_MAX_HEIGHT {
        bail!(
            "sprite sheet is {width}x{height}; the G1 object-tile grid holds at most \
             {OBJ_SHEET_MAX_WIDTH}x{OBJ_SHEET_MAX_HEIGHT}"
        );
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
mod tests;
