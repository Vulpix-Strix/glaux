//! System ROM data injected into the machine.

use crate::font::BitmapFont;
use crate::palette::Palette;

/// Built-in data the machine is created wit like the system palette and the
/// system font.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SystemRom {
    palette: Palette,
    font: BitmapFont,
}

impl SystemRom {
    pub fn new(palette: Palette, font: BitmapFont) -> Self {
        Self { palette, font }
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

    #[test]
    fn keeps_injected_palette() {
        let palette = gradient_palette();
        let rom = SystemRom::new(palette.clone(), BitmapFont::new(FontMetrics::SYSTEM));

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

        let rom = SystemRom::new(Palette::provisional(), font.clone());

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
