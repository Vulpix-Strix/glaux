//! The graphics state of a running cartridge.

use crate::bg::BgVram;
use crate::palette::Palette;
use crate::sprite::ObjVram;

/// The graphics state a cartridge runs against.
///
/// It holds bytes and machine types only. Reading cartridge files and
/// decoding them is the host application's job, and the container is built
/// from the already validated result. A running cartridge owns one, and
/// whoever draws a frame borrows it instead of copying it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CartridgeHardware {
    /// The master palette that frames are presented with.
    pub palette: Palette,
    pub obj: ObjVram,
    pub bg: BgVram,
}

impl CartridgeHardware {
    pub fn new(palette: Palette, obj: ObjVram, bg: BgVram) -> Self {
        Self { palette, obj, bg }
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
