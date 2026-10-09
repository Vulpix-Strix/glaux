//! System ROM data injected into the machine.

use glaux_audio::sfx::Sfx;
use semver::Version;

use crate::font::BitmapFont;
use crate::ink_image::InkImage;
use crate::obj_palette::ObjPalettes;
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

/// A pointer image and the pixel of it that marks the pointed-at position.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Cursor {
    pub image: InkImage,
    /// Offset of the pointed-at pixel from the image's top-left corner.
    pub hotspot: (u8, u8),
}

/// An icon the System ROM's UI shows: one per workspace view, then the
/// sprite editor's tools.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SystemIcon {
    Code,
    /// A pencil, also used for the sprite editor's pencil tool.
    Sprites,
    Map,
    Sfx,
    Music,
    Eraser,
    Eyedropper,
}

impl SystemIcon {
    /// Number of icons, the length of [`SystemUi::new`]'s `icons`.
    pub const COUNT: usize = 7;
}

/// The images the system UI draws with. The default has every image
/// empty, so a ROM built without them draws nothing for them.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SystemUi {
    pointer: Cursor,
    icons: [InkImage; SystemIcon::COUNT],
}

impl SystemUi {
    /// `icons` are in [`SystemIcon`] declaration order.
    pub fn new(pointer: Cursor, icons: [InkImage; SystemIcon::COUNT]) -> Self {
        Self { pointer, icons }
    }

    /// The default mouse pointer.
    pub fn pointer(&self) -> &Cursor {
        &self.pointer
    }

    pub fn icon(&self, icon: SystemIcon) -> &InkImage {
        &self.icons[icon as usize]
    }
}

/// Built-in data the machine is created with: the ROM version, its
/// copyright notice, the system palette, the system font, the system
/// sounds, the system UI images and the object palette banks cartridges
/// start with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SystemRom {
    version: Version,
    copyright: String,
    palette: Palette,
    font: BitmapFont,
    sounds: SystemSounds,
    ui: SystemUi,
    obj_palettes: ObjPalettes,
}

impl SystemRom {
    pub fn new(version: Version, copyright: String, palette: Palette, font: BitmapFont) -> Self {
        Self {
            version,
            copyright,
            palette,
            font,
            sounds: SystemSounds::default(),
            ui: SystemUi::default(),
            obj_palettes: ObjPalettes::linear(),
        }
    }

    /// This ROM with `sounds` in place of its sound effects.
    pub fn with_sounds(mut self, sounds: SystemSounds) -> Self {
        self.sounds = sounds;
        self
    }

    /// This ROM with `ui` in place of its UI images.
    pub fn with_ui(mut self, ui: SystemUi) -> Self {
        self.ui = ui;
        self
    }

    /// This ROM with `obj_palettes` as the object palette banks cartridges
    /// start with.
    pub fn with_obj_palettes(mut self, obj_palettes: ObjPalettes) -> Self {
        self.obj_palettes = obj_palettes;
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

    /// The images the system UI draws with.
    pub fn ui(&self) -> &SystemUi {
        &self.ui
    }

    /// The object palette banks a cartridge starts with unless it supplies
    /// its own.
    pub fn obj_palettes(&self) -> &ObjPalettes {
        &self.obj_palettes
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
    fn ui_images_are_empty_unless_injected_and_found_by_icon() {
        let rom = SystemRom::new(
            Version::new(0, 1, 0),
            notice(),
            Palette::provisional(),
            empty_font(),
        );
        assert_eq!(rom.ui(), &SystemUi::default());

        let solid = |slot| InkImage::new(1, 1, vec![slot]).unwrap();
        let pointer = Cursor {
            image: InkImage::new(2, 1, vec![1, 2]).unwrap(),
            hotspot: (1, 0),
        };
        let icons = [
            solid(0),
            solid(1),
            solid(2),
            solid(3),
            solid(1),
            solid(2),
            solid(3),
        ];
        let rom = rom.with_ui(SystemUi::new(pointer.clone(), icons));
        assert_eq!(rom.ui().pointer(), &pointer);
        assert_eq!(rom.ui().icon(SystemIcon::Map), &solid(2));
        assert_eq!(rom.ui().icon(SystemIcon::Music), &solid(1));
        assert_eq!(rom.ui().icon(SystemIcon::Eyedropper), &solid(3));
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
