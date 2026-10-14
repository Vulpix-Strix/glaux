//! The Glaux machine.

use crate::framebuffer::Framebuffer;
use crate::psg::Psg;
use crate::system_rom::SystemRom;

/// Machine family name.
pub const NAME: &str = "GLAUX";

/// Hardware generation within the family.
pub const GENERATION: &str = "G1";

/// Logical ticks per second. The system and cartridges advance exactly once
/// per tick, regardless of how often the host presents frames.
pub const TICK_RATE: u32 = 60;

/// its framebuffer and the System ROM it was built with.
#[derive(Debug, Clone)]
pub struct Machine {
    framebuffer: Framebuffer,
    psg: Psg,
    system_rom: SystemRom,
}

impl Machine {
    /// Creates a machine around `system_rom`, with the framebuffer cleared to
    /// index 0 and the PSG silent.
    pub fn new(system_rom: SystemRom) -> Self {
        Self {
            framebuffer: Framebuffer::new(),
            psg: Psg::new(),
            system_rom,
        }
    }

    pub fn framebuffer(&self) -> &Framebuffer {
        &self.framebuffer
    }

    pub fn framebuffer_mut(&mut self) -> &mut Framebuffer {
        &mut self.framebuffer
    }

    pub fn psg(&self) -> &Psg {
        &self.psg
    }

    pub fn psg_mut(&mut self) -> &mut Psg {
        &mut self.psg
    }

    pub fn system_rom(&self) -> &SystemRom {
        &self.system_rom
    }
}
