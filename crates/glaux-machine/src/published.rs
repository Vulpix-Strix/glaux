//! The frame a running cartridge shows the user.

use crate::framebuffer::Framebuffer;
use crate::hardware::CartridgeHardware;
use crate::palette::Palette;

/// A snapshot of a cartridge's work framebuffer and master palette.
///
/// It owns both, so nothing the cartridge does to its hardware afterwards,
/// including a palette change, reaches a frame that was already published.
/// The cartridge cannot address it. Whoever presents the cartridge reads it
/// and never writes to it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PublishedFrame {
    framebuffer: Framebuffer,
    palette: Palette,
}

impl PublishedFrame {
    /// A snapshot of `hardware` as it is now.
    pub fn new(hardware: &CartridgeHardware) -> Self {
        Self {
            framebuffer: hardware.framebuffer.clone(),
            palette: hardware.palette.clone(),
        }
    }

    /// Replaces the snapshot with `hardware` as it is now, reusing the
    /// buffers it already has.
    pub fn publish(&mut self, hardware: &CartridgeHardware) {
        self.framebuffer.copy_from(&hardware.framebuffer);
        self.palette.clone_from(&hardware.palette);
    }

    pub fn framebuffer(&self) -> &Framebuffer {
        &self.framebuffer
    }

    pub fn palette(&self) -> &Palette {
        &self.palette
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;
    use crate::bg::BgVram;
    use crate::palette::Rgb;
    use crate::sprite::ObjVram;

    fn hardware() -> CartridgeHardware {
        CartridgeHardware::new(Palette::default(), ObjVram::default(), BgVram::default())
    }

    #[test]
    fn a_snapshot_does_not_follow_later_changes_to_the_hardware() {
        let mut hardware = hardware();
        hardware.framebuffer.set_pixel(3, 4, 9);
        let old = hardware.palette.get(9);
        let mut frame = PublishedFrame::new(&hardware);

        hardware.framebuffer.set_pixel(3, 4, 1);
        hardware.palette.set(9, Rgb { r: 1, g: 2, b: 3 });

        assert_eq!(frame.framebuffer().pixel(3, 4), Some(9));
        assert_eq!(frame.palette().get(9), old);

        frame.publish(&hardware);
        assert_eq!(frame.framebuffer().pixel(3, 4), Some(1));
        assert_eq!(frame.palette().get(9), Rgb { r: 1, g: 2, b: 3 });
        assert_eq!(frame, PublishedFrame::new(&hardware));
    }
}
