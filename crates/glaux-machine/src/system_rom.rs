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

/// A pointer image, or the frames of an animated one, and the pixel of it
/// that marks the pointed-at position.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Cursor {
    /// One image for a still cursor, or each frame of an animated one in
    /// order, all the same size. Empty draws nothing.
    pub frames: Vec<InkImage>,
    /// Offset of the pointed-at pixel from the image's top-left corner.
    pub hotspot: (u8, u8),
}

impl Cursor {
    /// Frame `n` of the animation, counting on from the first after the
    /// last. `None` for a cursor without frames.
    pub fn frame(&self, n: usize) -> Option<&InkImage> {
        self.frames.get(n.checked_rem(self.frames.len())?)
    }
}

/// A mouse pointer the System ROM's UI shows, by what the pointer is over
/// or doing.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum SystemCursor {
    /// The arrow, over anything without a pointer of its own.
    #[default]
    Arrow,
    /// An I-beam, over text the pointer places a caret in.
    Text,
    /// A pointing hand, over a control a click acts on.
    Hand,
    /// A closed hand, while something is being dragged.
    Grab,
    /// An hourglass, animated, while the system waits and takes no input.
    Busy,
}

impl SystemCursor {
    /// Number of cursors, the length of [`SystemUi::new`]'s `cursors`.
    pub const COUNT: usize = 5;
}

/// An icon the System ROM's UI shows: one per workspace view, then the
/// sprite editor's tools, then the Code view's settings button.
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
    Fill,
    /// A cogwheel. Its art fills only the central 12×12 pixels.
    Settings,
}

impl SystemIcon {
    /// Number of icons, the length of [`SystemUi::new`]'s `icons`.
    pub const COUNT: usize = 9;
}

/// The images the system UI draws with. The default has every image
/// empty, so a ROM built without them draws nothing for them.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SystemUi {
    cursors: [Cursor; SystemCursor::COUNT],
    icons: [InkImage; SystemIcon::COUNT],
}

impl SystemUi {
    /// `cursors` are in [`SystemCursor`] declaration order, and `icons` in
    /// [`SystemIcon`] declaration order.
    pub fn new(
        cursors: [Cursor; SystemCursor::COUNT],
        icons: [InkImage; SystemIcon::COUNT],
    ) -> Self {
        Self { cursors, icons }
    }

    pub fn cursor(&self, cursor: SystemCursor) -> &Cursor {
        &self.cursors[cursor as usize]
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
mod tests;
