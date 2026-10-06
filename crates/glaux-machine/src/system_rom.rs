//! System ROM data injected into the machine.

use glaux_audio::sfx::Sfx;
use semver::Version;

use crate::font::BitmapFont;
use crate::palette::Palette;

/// A sound the System ROM plays.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SystemSfx {
    /// The power-on chime.
    Boot,
}

/// The sound effects shipped with the System ROM. The default has every
/// sound empty, so a ROM built without sounds plays nothing.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SystemSounds {
    boot: Sfx,
}

impl SystemSounds {
    pub fn new(boot: Sfx) -> Self {
        Self { boot }
    }

    pub fn get(&self, sfx: SystemSfx) -> &Sfx {
        match sfx {
            SystemSfx::Boot => &self.boot,
        }
    }
}

/// Built-in data the machine is created with: the ROM version, its
/// copyright notice, the system palette, the system font and the system
/// sounds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SystemRom {
    version: Version,
    copyright: String,
    palette: Palette,
    font: BitmapFont,
    sounds: SystemSounds,
}

impl SystemRom {
    pub fn new(version: Version, copyright: String, palette: Palette, font: BitmapFont) -> Self {
        Self {
            version,
            copyright,
            palette,
            font,
            sounds: SystemSounds::default(),
        }
    }

    /// This ROM with `sounds` in place of its sound effects.
    pub fn with_sounds(mut self, sounds: SystemSounds) -> Self {
        self.sounds = sounds;
        self
    }

    /// The ROM version, independent of the application version.
    pub fn version(&self) -> &Version {
        &self.version
    }

    /// The copyright notice shipped with this ROM release, e.g.
    /// `(C) 2026 CUERVOLU`.
    pub fn copyright(&self) -> &str {
        &self.copyright
    }

    /// The system palette.
    pub fn palette(&self) -> &Palette {
        &self.palette
    }

    /// The system font.
    pub fn font(&self) -> &BitmapFont {
        &self.font
    }

    /// The system sound effects.
    pub fn sounds(&self) -> &SystemSounds {
        &self.sounds
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

    fn notice() -> String {
        "(C) 2026 TEST".into()
    }

    #[test]
    fn keeps_injected_copyright() {
        let rom = SystemRom::new(
            Version::new(0, 1, 0),
            "(C) 1999 OWL".into(),
            Palette::provisional(),
            empty_font(),
        );
        assert_eq!(rom.copyright(), "(C) 1999 OWL");
    }

    #[test]
    fn keeps_injected_version() {
        let rom = SystemRom::new(
            Version::new(0, 1, 0),
            notice(),
            Palette::provisional(),
            empty_font(),
        );
        assert_eq!(rom.version(), &Version::new(0, 1, 0));
    }

    #[test]
    fn fake_roms_keep_their_own_versions() {
        let old = SystemRom::new(
            Version::new(0, 1, 0),
            notice(),
            Palette::provisional(),
            empty_font(),
        );
        let new = SystemRom::new(
            Version::new(3, 7, 0),
            notice(),
            Palette::provisional(),
            empty_font(),
        );
        assert_eq!(old.version(), &Version::new(0, 1, 0));
        assert_eq!(new.version(), &Version::new(3, 7, 0));
        assert_ne!(old, new);
    }

    #[test]
    fn keeps_injected_palette() {
        let palette = gradient_palette();
        let rom = SystemRom::new(
            Version::new(0, 1, 0),
            notice(),
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
            Version::new(0, 1, 0),
            notice(),
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

    #[test]
    fn sounds_are_empty_unless_injected() {
        use glaux_audio::sfx::{SfxAction, SfxStep};

        let rom = SystemRom::new(
            Version::new(0, 1, 0),
            notice(),
            Palette::provisional(),
            empty_font(),
        );
        assert_eq!(rom.sounds().get(SystemSfx::Boot), &Sfx::default());

        let chime = Sfx::new(vec![SfxStep {
            tick: 0,
            action: SfxAction::Gate {
                channel: 0,
                open: true,
            },
        }])
        .unwrap();
        let rom = rom.with_sounds(SystemSounds::new(chime.clone()));
        assert_eq!(rom.sounds().get(SystemSfx::Boot), &chime);
    }
}
