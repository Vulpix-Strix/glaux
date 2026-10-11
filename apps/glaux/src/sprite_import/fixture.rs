//! PNG files for tests of the importer and its callers.

use std::fs::File;
use std::io::BufWriter;
use std::path::Path;

use png::{BitDepth, ColorType};

/// Writes `rgba`, `width`x`height` RGBA8 pixels, to a PNG at `path`.
pub(crate) fn rgba_png(path: &Path, width: u32, height: u32, rgba: &[u8]) {
    let file = BufWriter::new(File::create(path).unwrap());
    let mut encoder = png::Encoder::new(file, width, height);
    encoder.set_color(ColorType::Rgba);
    encoder.set_depth(BitDepth::Eight);
    encoder
        .write_header()
        .unwrap()
        .write_image_data(rgba)
        .unwrap();
}
