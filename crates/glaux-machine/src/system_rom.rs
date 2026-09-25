//! System ROM data injected into the machine.

use std::fmt;

use crate::font::BitmapFont;
use crate::palette::Palette;

/// Version of a System ROM, shown as `major.minor`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SystemRomVersion {
    pub major: u8,
    pub minor: u8,
}

impl SystemRomVersion {
    pub const fn new(major: u8, minor: u8) -> Self {
        Self { major, minor }
    }
}

impl fmt::Display for SystemRomVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}", self.major, self.minor)
    }
}

/// Built-in data the machine is created with: the ROM version, the system
/// palette and the system font.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SystemRom {
    version: SystemRomVersion,
    palette: Palette,
    font: BitmapFont,
}

impl SystemRom {
    pub fn new(version: SystemRomVersion, palette: Palette, font: BitmapFont) -> Self {
        Self {
            version,
            palette,
            font,
        }
    }

    /// The ROM version.
    pub fn version(&self) -> SystemRomVersion {
        self.version
    }

    /// The system palette.
    pub fn palette(&self) -> &Palette {
        &self.palette
    }

    /// The system font.
    pub fn font(&self) -> &BitmapFont {
        &self.font
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::font::FontMetrics;
    use crate::palette::{PALETTE_LEN, Rgb};
    use pretty_assertions::assert_eq;

    fn gradient_palette() -> Palette {
        let mut entries = [Rgb::from_hex(0); PALETTE_LEN];
        for (i, entry) in entries.iter_mut().enumerate() {
            *entry = Rgb {
                r: i as u8,
                g: 255 - i as u8,
                b: 0x40,
            };
        }
        Palette::new(entries)
    }

    fn empty_font() -> BitmapFont {
        BitmapFont::new(FontMetrics::SYSTEM)
    }

    #[test]
    fn keeps_injected_version() {
        let rom = SystemRom::new(
            SystemRomVersion::new(0, 1),
            Palette::provisional(),
            empty_font(),
        );
        assert_eq!(rom.version(), SystemRomVersion { major: 0, minor: 1 });
    }

    #[test]
    fn fake_roms_keep_their_own_versions() {
        let old = SystemRom::new(
            SystemRomVersion::new(0, 1),
            Palette::provisional(),
            empty_font(),
        );
        let new = SystemRom::new(
            SystemRomVersion::new(3, 7),
            Palette::provisional(),
            empty_font(),
        );
        assert_eq!(old.version(), SystemRomVersion::new(0, 1));
        assert_eq!(new.version(), SystemRomVersion::new(3, 7));
        assert_ne!(old, new);
    }

    #[test]
    fn version_displays_as_major_dot_minor() {
        assert_eq!(SystemRomVersion::new(0, 1).to_string(), "0.1");
        assert_eq!(SystemRomVersion::new(12, 3).to_string(), "12.3");
    }

    #[test]
    fn keeps_injected_palette() {
        let palette = gradient_palette();
        let rom = SystemRom::new(
            SystemRomVersion::new(0, 1),
            palette.clone(),
            BitmapFont::new(FontMetrics::SYSTEM),
        );

        assert_eq!(rom.palette(), &palette);
        assert_eq!(
            rom.palette().get(10),
            Rgb {
                r: 10,
                g: 245,
                b: 0x40
            }
        );
    }

    #[test]
    fn keeps_injected_font() {
        let mut font = BitmapFont::new(FontMetrics::new(3, 3, 4, 4).unwrap());
        font.insert('o', &[0b111, 0b101, 0b111]).unwrap();
        font.insert('.', &[0, 0, 0b010]).unwrap();

        let rom = SystemRom::new(
            SystemRomVersion::new(0, 1),
            Palette::provisional(),
            font.clone(),
        );

        assert_eq!(rom.font(), &font);
        assert_eq!(rom.font().metrics(), FontMetrics::new(3, 3, 4, 4).unwrap());
        assert_eq!(
            rom.font().glyph('o').unwrap().rows(),
            &[0b111, 0b101, 0b111]
        );
        assert_eq!(rom.font().glyph('.').unwrap().rows(), &[0, 0, 0b010]);
        assert_eq!(rom.font().glyph('x'), None);
    }
}
