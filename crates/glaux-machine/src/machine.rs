//! The Glaux machine.

use crate::framebuffer::Framebuffer;
use crate::system_rom::SystemRom;

/// its framebuffer and the System ROM it was built with.
#[derive(Debug, Clone)]
pub struct Machine {
    framebuffer: Framebuffer,
    system_rom: SystemRom,
}

impl Machine {
    /// Creates a machine around `system_rom`, with the framebuffer cleared to
    /// index 0.
    pub fn new(system_rom: SystemRom) -> Self {
        Self {
            framebuffer: Framebuffer::new(),
            system_rom,
        }
    }

    pub fn framebuffer(&self) -> &Framebuffer {
        &self.framebuffer
    }

    pub fn framebuffer_mut(&mut self) -> &mut Framebuffer {
        &mut self.framebuffer
    }

    pub fn system_rom(&self) -> &SystemRom {
        &self.system_rom
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::font::{BitmapFont, FontMetrics};
    use crate::palette::{PALETTE_LEN, Palette, Rgb};
    use pretty_assertions::assert_eq;
    use semver::Version;

    fn fake_rom(color: u32, glyph: char) -> SystemRom {
        let mut font = BitmapFont::new(FontMetrics::SYSTEM);
        font.insert(glyph, &[0b10101; 9]).unwrap();
        SystemRom::new(
            Version::new(0, 1, 0),
            Palette::new([Rgb::from_hex(color); PALETTE_LEN]),
            font,
        )
    }

    #[test]
    fn exposes_the_injected_rom() {
        let rom = fake_rom(0x123456, 'g');
        let machine = Machine::new(rom.clone());
        assert_eq!(machine.system_rom(), &rom);
    }

    #[test]
    fn framebuffer_starts_cleared() {
        let machine = Machine::new(fake_rom(0x123456, 'g'));
        assert_eq!(machine.framebuffer(), &Framebuffer::new());
    }

    #[test]
    fn machines_do_not_share_state() {
        let mut first = Machine::new(fake_rom(0x111111, 'a'));
        let second = Machine::new(fake_rom(0x222222, 'b'));

        first.framebuffer_mut().set_pixel(3, 4, 9);

        assert_eq!(first.framebuffer().pixel(3, 4), Some(9));
        assert_eq!(second.framebuffer().pixel(3, 4), Some(0));

        assert_eq!(first.system_rom().palette().get(0), Rgb::from_hex(0x111111));
        assert_eq!(
            second.system_rom().palette().get(0),
            Rgb::from_hex(0x222222)
        );

        assert!(first.system_rom().font().glyph('a').is_some());
        assert!(first.system_rom().font().glyph('b').is_none());
        assert!(second.system_rom().font().glyph('b').is_some());
        assert!(second.system_rom().font().glyph('a').is_none());
    }
}
