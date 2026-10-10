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

/// Color used for palette entries the provisional palette does not define.
pub const DEBUG_MAGENTA: Rgb = Rgb::from_hex(0xFF00FF);

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

/// A 256-entry master palette. Every `u8` is a valid index.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Palette {
    entries: [Rgb; PALETTE_LEN],
}

impl Palette {
    /// Creates a palette from explicit entries in index order.
    pub const fn new(entries: [Rgb; PALETTE_LEN]) -> Self {
        Self { entries }
    }

    /// Zughy 32 in entries `0..32`; the unused entries `32..256` are
    /// [`DEBUG_MAGENTA`] so that stray indices are easy to spot.
    pub fn provisional() -> Self {
        let mut entries = [DEBUG_MAGENTA; PALETTE_LEN];
        entries[..ZUGHY_32.len()].copy_from_slice(&ZUGHY_32);
        Self::new(entries)
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
