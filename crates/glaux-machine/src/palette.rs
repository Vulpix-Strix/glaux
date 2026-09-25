//! Master palette and color types.

/// Number of entries in the master palette.
pub const PALETTE_LEN: usize = 256;

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
    /// Zughy 32 in entries `0..32`; the unused entries `32..256` are
    /// [`DEBUG_MAGENTA`] so that stray indices are easy to spot.
    pub fn provisional() -> Self {
        let mut entries = [DEBUG_MAGENTA; PALETTE_LEN];
        entries[..ZUGHY_32.len()].copy_from_slice(&ZUGHY_32);
        Self { entries }
    }

    /// Returns the color for a palette index.
    pub fn get(&self, index: u8) -> Rgb {
        self.entries[usize::from(index)]
    }

    /// Returns all 256 entries in index order.
    pub fn entries(&self) -> &[Rgb; PALETTE_LEN] {
        &self.entries
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn has_256_entries() {
        assert_eq!(Palette::provisional().entries().len(), 256);
    }

    #[test]
    fn first_32_entries_are_zughy_32() {
        let palette = Palette::provisional();
        assert_eq!(palette.entries()[..32], ZUGHY_32);
        assert_eq!(
            palette.get(0),
            Rgb {
                r: 0x47,
                g: 0x2d,
                b: 0x3c
            }
        );
        assert_eq!(
            palette.get(31),
            Rgb {
                r: 0x4f,
                g: 0x54,
                b: 0x6b
            }
        );
    }

    #[test]
    fn unused_entries_are_debug_magenta() {
        let palette = Palette::provisional();
        assert_eq!(
            DEBUG_MAGENTA,
            Rgb {
                r: 255,
                g: 0,
                b: 255
            }
        );
        for index in 32..=255u8 {
            assert_eq!(palette.get(index), DEBUG_MAGENTA, "entry {index}");
        }
    }
}
