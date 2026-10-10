//! Master palette and color types.

/// Number of entries in the master palette.
pub const PALETTE_LEN: usize = 256;

/// Size of a master palette in its native RGB888 byte form.
pub const PALETTE_RGB888_BYTES: usize = PALETTE_LEN * 3;

/// A failure to construct a palette from RGB888 bytes.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("palette data must be exactly {PALETTE_RGB888_BYTES} bytes, got {length}")]
pub struct PaletteBytesError {
    pub length: usize,
}

/// An 8-bit-per-channel RGB color.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Rgb {
    /// Builds a color from `0xRRGGBB`. Bits above the low 24 are ignored.
    pub const fn from_hex(hex: u32) -> Self {
        Self {
            r: (hex >> 16) as u8,
            g: (hex >> 8) as u8,
            b: hex as u8,
        }
    }
}

/// The Zughy 32 palette by Zughy (<https://lospec.com/palette-list/zughy-32>),
/// in its published order.
pub const ZUGHY_32: [Rgb; 32] = [
    Rgb::from_hex(0x472d3c),
    Rgb::from_hex(0x5e3643),
    Rgb::from_hex(0x7a444a),
    Rgb::from_hex(0xa05b53),
    Rgb::from_hex(0xbf7958),
    Rgb::from_hex(0xeea160),
    Rgb::from_hex(0xf4cca1),
    Rgb::from_hex(0xb6d53c),
    Rgb::from_hex(0x71aa34),
    Rgb::from_hex(0x397b44),
    Rgb::from_hex(0x3c5956),
    Rgb::from_hex(0x302c2e),
    Rgb::from_hex(0x5a5353),
    Rgb::from_hex(0x7d7071),
    Rgb::from_hex(0xa0938e),
    Rgb::from_hex(0xcfc6b8),
    Rgb::from_hex(0xdff6f5),
    Rgb::from_hex(0x8aebf1),
    Rgb::from_hex(0x28ccdf),
    Rgb::from_hex(0x3978a8),
    Rgb::from_hex(0x394778),
    Rgb::from_hex(0x39314b),
    Rgb::from_hex(0x564064),
    Rgb::from_hex(0x8e478c),
    Rgb::from_hex(0xcd6093),
    Rgb::from_hex(0xffaeb6),
    Rgb::from_hex(0xf4b41b),
    Rgb::from_hex(0xf47e1b),
    Rgb::from_hex(0xe6482e),
    Rgb::from_hex(0xa93b3b),
    Rgb::from_hex(0x827094),
    Rgb::from_hex(0x4f546b),
];

/// The colors after [`ZUGHY_32`] in the default master palette: mostly
/// eight-step ramps, dark to light, whose shadows and highlights shift hue
/// in the manner of Zughy 32's own ramps.
const ZUGHY_32_EXTENSION: [Rgb; PALETTE_LEN - ZUGHY_32.len()] = [
    // 32–47: warm greys, cool greys.
    Rgb::from_hex(0x181215),
    Rgb::from_hex(0x261f22),
    Rgb::from_hex(0x473f40),
    Rgb::from_hex(0x6b6261),
    Rgb::from_hex(0x8c827f),
    Rgb::from_hex(0xb4aaa5),
    Rgb::from_hex(0xe7ded6),
    Rgb::from_hex(0xfdf6eb),
    Rgb::from_hex(0x151520),
    Rgb::from_hex(0x242532),
    Rgb::from_hex(0x383b4a),
    Rgb::from_hex(0x636979),
    Rgb::from_hex(0x7e8796),
    Rgb::from_hex(0x9aa6b4),
    Rgb::from_hex(0xb5c3ce),
    Rgb::from_hex(0xd1dfe6),
    // 48–63: light skin, deep skin.
    Rgb::from_hex(0x8b5052),
    Rgb::from_hex(0xa56762),
    Rgb::from_hex(0xbb7d71),
    Rgb::from_hex(0xcc917f),
    Rgb::from_hex(0xdda690),
    Rgb::from_hex(0xecbba4),
    Rgb::from_hex(0xf7cdb7),
    Rgb::from_hex(0xffdfcc),
    Rgb::from_hex(0x301219),
    Rgb::from_hex(0x461c22),
    Rgb::from_hex(0x5b282a),
    Rgb::from_hex(0x723833),
    Rgb::from_hex(0x874a3d),
    Rgb::from_hex(0x9c5e48),
    Rgb::from_hex(0xae7357),
    Rgb::from_hex(0xc08a6a),
    // 64–79: crimson, rust.
    Rgb::from_hex(0x371325),
    Rgb::from_hex(0x562032),
    Rgb::from_hex(0x772e3d),
    Rgb::from_hex(0x9a3e45),
    Rgb::from_hex(0xbd4f4b),
    Rgb::from_hex(0xe26755),
    Rgb::from_hex(0xfa9075),
    Rgb::from_hex(0xffc0ac),
    Rgb::from_hex(0x612e2d),
    Rgb::from_hex(0x863b34),
    Rgb::from_hex(0xa84c3a),
    Rgb::from_hex(0xc66241),
    Rgb::from_hex(0xe17c4b),
    Rgb::from_hex(0xf79a5d),
    Rgb::from_hex(0xffbe8a),
    Rgb::from_hex(0xffdebc),
    // 80–95: amber, gold.
    Rgb::from_hex(0x592c1e),
    Rgb::from_hex(0x7e3919),
    Rgb::from_hex(0x9e4b12),
    Rgb::from_hex(0xbd6100),
    Rgb::from_hex(0xd67b10),
    Rgb::from_hex(0xee9d37),
    Rgb::from_hex(0xffbc5f),
    Rgb::from_hex(0xffd896),
    Rgb::from_hex(0x56341e),
    Rgb::from_hex(0x744205),
    Rgb::from_hex(0x935f00),
    Rgb::from_hex(0xb17f00),
    Rgb::from_hex(0xca9e00),
    Rgb::from_hex(0xe3c23f),
    Rgb::from_hex(0xf5df6b),
    Rgb::from_hex(0xfff5a5),
    // 96–111: lime, grass.
    Rgb::from_hex(0x315038),
    Rgb::from_hex(0x3c6b34),
    Rgb::from_hex(0x558429),
    Rgb::from_hex(0x769b19),
    Rgb::from_hex(0x9ab216),
    Rgb::from_hex(0xbec832),
    Rgb::from_hex(0xe0df5b),
    Rgb::from_hex(0xfbf488),
    Rgb::from_hex(0x1d4c41),
    Rgb::from_hex(0x09684e),
    Rgb::from_hex(0x0b8155),
    Rgb::from_hex(0x2f9a5b),
    Rgb::from_hex(0x56b163),
    Rgb::from_hex(0x7ec76f),
    Rgb::from_hex(0xa7dd84),
    Rgb::from_hex(0xccf0a0),
    // 112–127: forest, moss.
    Rgb::from_hex(0x00201b),
    Rgb::from_hex(0x002e23),
    Rgb::from_hex(0x053d2a),
    Rgb::from_hex(0x184c32),
    Rgb::from_hex(0x2c5d3c),
    Rgb::from_hex(0x437048),
    Rgb::from_hex(0x5e875c),
    Rgb::from_hex(0x7da175),
    Rgb::from_hex(0x362913),
    Rgb::from_hex(0x4d3d1a),
    Rgb::from_hex(0x635425),
    Rgb::from_hex(0x786c34),
    Rgb::from_hex(0x8d8549),
    Rgb::from_hex(0xa29f61),
    Rgb::from_hex(0xb9bb7e),
    Rgb::from_hex(0xd2d79e),
    // 128–143: teal, mint.
    Rgb::from_hex(0x0d2a35),
    Rgb::from_hex(0x00424e),
    Rgb::from_hex(0x005b63),
    Rgb::from_hex(0x007576),
    Rgb::from_hex(0x0b938c),
    Rgb::from_hex(0x41b0a0),
    Rgb::from_hex(0x70cfb9),
    Rgb::from_hex(0x9feed4),
    Rgb::from_hex(0x225050),
    Rgb::from_hex(0x176c64),
    Rgb::from_hex(0x198775),
    Rgb::from_hex(0x34a184),
    Rgb::from_hex(0x58ba93),
    Rgb::from_hex(0x7fd3a5),
    Rgb::from_hex(0xa8eabc),
    Rgb::from_hex(0xcefcd6),
    // 144–159: sky, azure.
    Rgb::from_hex(0x173852),
    Rgb::from_hex(0x125277),
    Rgb::from_hex(0x0b6f99),
    Rgb::from_hex(0x198cb7),
    Rgb::from_hex(0x39a9d1),
    Rgb::from_hex(0x61c6e7),
    Rgb::from_hex(0x8ddff7),
    Rgb::from_hex(0xbff2ff),
    Rgb::from_hex(0x28395f),
    Rgb::from_hex(0x314e86),
    Rgb::from_hex(0x3965a9),
    Rgb::from_hex(0x437fc8),
    Rgb::from_hex(0x509ae1),
    Rgb::from_hex(0x65b6f3),
    Rgb::from_hex(0x86d2ff),
    Rgb::from_hex(0xbbe9ff),
    // 160–175: cobalt, indigo.
    Rgb::from_hex(0x151733),
    Rgb::from_hex(0x20274f),
    Rgb::from_hex(0x2c3c70),
    Rgb::from_hex(0x365293),
    Rgb::from_hex(0x3d69b7),
    Rgb::from_hex(0x4786de),
    Rgb::from_hex(0x6dabf5),
    Rgb::from_hex(0xa8d2ff),
    Rgb::from_hex(0x14122b),
    Rgb::from_hex(0x211e42),
    Rgb::from_hex(0x312f5a),
    Rgb::from_hex(0x424171),
    Rgb::from_hex(0x565889),
    Rgb::from_hex(0x6e73a3),
    Rgb::from_hex(0x8b92bd),
    Rgb::from_hex(0xaeb6d8),
    // 176–191: violet, lilac.
    Rgb::from_hex(0x2d2046),
    Rgb::from_hex(0x432f63),
    Rgb::from_hex(0x5a4081),
    Rgb::from_hex(0x73509f),
    Rgb::from_hex(0x8f60be),
    Rgb::from_hex(0xad7ad4),
    Rgb::from_hex(0xcc9ae7),
    Rgb::from_hex(0xeabff8),
    Rgb::from_hex(0x5a5770),
    Rgb::from_hex(0x72688e),
    Rgb::from_hex(0x8b79a8),
    Rgb::from_hex(0xa68bbf),
    Rgb::from_hex(0xbfa0d3),
    Rgb::from_hex(0xd7b6e4),
    Rgb::from_hex(0xebcdf2),
    Rgb::from_hex(0xf8e3fb),
    // 192–207: magenta, rose.
    Rgb::from_hex(0x2b1531),
    Rgb::from_hex(0x472349),
    Rgb::from_hex(0x653160),
    Rgb::from_hex(0x8b4179),
    Rgb::from_hex(0xb25291),
    Rgb::from_hex(0xda6da9),
    Rgb::from_hex(0xf892bf),
    Rgb::from_hex(0xffc5d8),
    Rgb::from_hex(0x69455d),
    Rgb::from_hex(0x8c4f70),
    Rgb::from_hex(0xae5b7e),
    Rgb::from_hex(0xcd6b89),
    Rgb::from_hex(0xe88095),
    Rgb::from_hex(0xfe99a3),
    Rgb::from_hex(0xffc3c2),
    Rgb::from_hex(0xffe4e1),
    // 208–223: bark, sand.
    Rgb::from_hex(0x22110b),
    Rgb::from_hex(0x341e13),
    Rgb::from_hex(0x462b1c),
    Rgb::from_hex(0x583b26),
    Rgb::from_hex(0x6c4e35),
    Rgb::from_hex(0x806246),
    Rgb::from_hex(0x957b5d),
    Rgb::from_hex(0xab9476),
    Rgb::from_hex(0x775e45),
    Rgb::from_hex(0x91704a),
    Rgb::from_hex(0xa98554),
    Rgb::from_hex(0xbf9b63),
    Rgb::from_hex(0xd2b379),
    Rgb::from_hex(0xe5cb94),
    Rgb::from_hex(0xf1deb0),
    Rgb::from_hex(0xfbf0cf),
    // 224–239: stone, muted mid-tones.
    Rgb::from_hex(0x111c22),
    Rgb::from_hex(0x1d2f2d),
    Rgb::from_hex(0x334135),
    Rgb::from_hex(0x505543),
    Rgb::from_hex(0x6d6a56),
    Rgb::from_hex(0x8b826f),
    Rgb::from_hex(0xa79d8b),
    Rgb::from_hex(0xbfb7a6),
    Rgb::from_hex(0x8c666b),
    Rgb::from_hex(0x745971),
    Rgb::from_hex(0x80a5b3),
    Rgb::from_hex(0x59758d),
    Rgb::from_hex(0x5a8382),
    Rgb::from_hex(0x748f74),
    Rgb::from_hex(0x988b69),
    Rgb::from_hex(0x7d5a4e),
    // 240–255: muted darks; black, white and bright accents.
    Rgb::from_hex(0x503534),
    Rgb::from_hex(0x524d3b),
    Rgb::from_hex(0x454b35),
    Rgb::from_hex(0x1b3631),
    Rgb::from_hex(0x2b4a54),
    Rgb::from_hex(0x232e42),
    Rgb::from_hex(0x433a54),
    Rgb::from_hex(0x614b57),
    Rgb::from_hex(0x000000),
    Rgb::from_hex(0xffffff),
    Rgb::from_hex(0xddedff),
    Rgb::from_hex(0xf462a4),
    Rgb::from_hex(0x8ced77),
    Rgb::from_hex(0x66f1ec),
    Rgb::from_hex(0x4699fe),
    Rgb::from_hex(0xab73e4),
];

/// Entries of [`Palette::default`]: [`ZUGHY_32`], then
/// [`ZUGHY_32_EXTENSION`].
const DEFAULT_ENTRIES: [Rgb; PALETTE_LEN] = {
    let mut entries = [Rgb::from_hex(0); PALETTE_LEN];
    let mut index = 0;
    while index < PALETTE_LEN {
        entries[index] = if index < ZUGHY_32.len() {
            ZUGHY_32[index]
        } else {
            ZUGHY_32_EXTENSION[index - ZUGHY_32.len()]
        };
        index += 1;
    }
    entries
};

/// A 256-entry master palette. Every `u8` is a valid index.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Palette {
    entries: [Rgb; PALETTE_LEN],
}

/// The default master palette: [`ZUGHY_32`] in entries 0–31 and 224 more
/// colors in 32–255, every entry a distinct color. It holds the same colors
/// as the System ROM palette, which the machine is normally given instead.
impl Default for Palette {
    fn default() -> Self {
        Self::new(DEFAULT_ENTRIES)
    }
}

impl Palette {
    /// Creates a palette from explicit entries in index order.
    pub const fn new(entries: [Rgb; PALETTE_LEN]) -> Self {
        Self { entries }
    }

    /// Returns the color for a palette index.
    pub fn get(&self, index: u8) -> Rgb {
        self.entries[usize::from(index)]
    }

    /// Sets the color of a palette index and returns the color it replaced.
    pub fn set(&mut self, index: u8, color: Rgb) -> Rgb {
        std::mem::replace(&mut self.entries[usize::from(index)], color)
    }

    /// Returns all 256 entries in index order.
    pub fn entries(&self) -> &[Rgb; PALETTE_LEN] {
        &self.entries
    }

    /// Decodes 256 colors stored as consecutive `R, G, B` bytes in index
    /// order.
    pub fn from_rgb888(bytes: &[u8]) -> Result<Self, PaletteBytesError> {
        let (triples, rest) = bytes.as_chunks::<3>();
        if triples.len() != PALETTE_LEN || !rest.is_empty() {
            return Err(PaletteBytesError {
                length: bytes.len(),
            });
        }
        let mut entries = [Rgb::from_hex(0); PALETTE_LEN];
        for (entry, &[r, g, b]) in entries.iter_mut().zip(triples) {
            *entry = Rgb { r, g, b };
        }
        Ok(Self::new(entries))
    }

    /// Encodes the palette as consecutive `R, G, B` bytes in index order.
    pub fn to_rgb888(&self) -> [u8; PALETTE_RGB888_BYTES] {
        let mut bytes = [0; PALETTE_RGB888_BYTES];
        for (out, color) in bytes.as_chunks_mut::<3>().0.iter_mut().zip(&self.entries) {
            *out = [color.r, color.g, color.b];
        }
        bytes
    }
}

#[cfg(test)]
mod tests;
