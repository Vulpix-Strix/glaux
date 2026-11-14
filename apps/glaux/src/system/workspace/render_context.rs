//! What the workspace views draw with and what the system lends them for
//! one frame.

use glaux_input::Point;
use glaux_machine::font::BitmapFont;
use glaux_machine::framebuffer::Framebuffer;
use glaux_machine::system_rom::SystemUi;

use super::StatusInfo;
use super::sprites::SpriteSource;
use crate::cartridge::LoadedCartridge;
use crate::system::editing::session::CodeSession;
use crate::system::map_document::MapDocument;

/// The frame being drawn and the System ROM resources to draw it with.
pub struct RenderContext<'a> {
    pub framebuffer: &'a mut Framebuffer,
    pub font: &'a BitmapFont,
    pub ui: &'a SystemUi,
}

impl<'a> RenderContext<'a> {
    pub fn new(framebuffer: &'a mut Framebuffer, font: &'a BitmapFont, ui: &'a SystemUi) -> Self {
        Self {
            framebuffer,
            font,
            ui,
        }
    }
}

/// What an editor takes of the frame besides its own data: where the pointer
/// is, which it highlights what is under, and the status bar's extras.
#[derive(Debug, Clone, Copy, Default)]
pub struct ViewInput<'a> {
    pub pointer: Option<Point>,
    pub info: StatusInfo<'a>,
}

/// What the system lends the workspace to draw one frame.
#[derive(Clone, Copy)]
pub struct ViewSources<'a> {
    pub cartridge: &'a LoadedCartridge,
    /// The sprite editor's cartridge state, or why there is none.
    pub sprites: Result<SpriteSource<'a>, &'a str>,
    pub code: Option<&'a CodeSession>,
    pub maps: Option<&'a MapDocument>,
}
