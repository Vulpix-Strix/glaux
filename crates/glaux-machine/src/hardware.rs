//! The state of a running cartridge: its graphics memory, Work RAM and work
//! framebuffer.

use crate::bg::BgVram;
use crate::framebuffer::Framebuffer;
use crate::memory_map::WORK_RAM_LEN;
use crate::palette::Palette;
use crate::sprite::ObjVram;

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
}

impl CartridgeHardware {
    /// Creates the hardware around the given graphics state, with Work RAM
    /// and the framebuffer zeroed.
    pub fn new(palette: Palette, obj: ObjVram, bg: BgVram) -> Self {
        Self {
            palette,
            obj,
            bg,
            work_ram: Box::new([0; WORK_RAM_LEN]),
            framebuffer: Framebuffer::new(),
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
