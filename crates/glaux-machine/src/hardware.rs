//! The state of a running cartridge: its graphics memory and registers, Work
//! RAM and work framebuffer.

use crate::bg::BgVram;
use crate::framebuffer::Framebuffer;
use crate::memory_map::WORK_RAM_LEN;
use crate::oam::Oam;
use crate::palette::Palette;
use crate::registers::GraphicsRegisters;
use crate::sprite::ObjVram;
use crate::tile_window::TileWindow;
use crate::tilemap::BG_LAYERS;

/// The state a cartridge runs against.
///
/// It holds bytes and machine types only. Reading cartridge files and
/// decoding them is the host application's job, and the container is built
/// from the already validated result. A running cartridge owns one, and
/// whoever draws a frame borrows it instead of copying it.
///
/// The [bus](crate::bus) reads and writes these same bytes, so the typed
/// fields and the address space can never disagree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CartridgeHardware {
    /// The master palette that frames are presented with.
    pub palette: Palette,
    pub obj: ObjVram,
    pub bg: BgVram,
    pub work_ram: Box<[u8; WORK_RAM_LEN]>,
    /// The cartridge's own surface, separate from the machine's framebuffer.
    pub framebuffer: Framebuffer,
    /// The tilemap window of each BG layer.
    pub tilemaps: [TileWindow; BG_LAYERS],
    pub oam: Oam,
    pub registers: GraphicsRegisters,
}

impl CartridgeHardware {
    /// Creates the hardware around the given graphics state. Work RAM, the
    /// framebuffer, the tilemap windows and OAM start zeroed, and the
    /// registers start at their power-on values.
    pub fn new(palette: Palette, obj: ObjVram, bg: BgVram) -> Self {
        Self {
            palette,
            obj,
            bg,
            work_ram: Box::new([0; WORK_RAM_LEN]),
            framebuffer: Framebuffer::new(),
            tilemaps: std::array::from_fn(|_| TileWindow::new()),
            oam: Oam::new(),
            registers: GraphicsRegisters::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The hardware moves between threads while a script runtime borrows it,
    /// so it must stay `Send` without any manual impl.
    const _: fn() = || {
        fn assert_send<T: Send>() {}
        assert_send::<CartridgeHardware>();
    };
}
