mod rom;
mod system;

use glaux_host_raylib::RaylibHost;
use glaux_machine::machine::Machine;
use glaux_video::IndexedFrame;
use system::System;

const WINDOW_SCALE: i32 = 3;

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let mut machine = Machine::new(rom::load()?);
    // The system draws into the machine's framebuffer with data from the
    // machine's ROM; a one-time copy of the ROM avoids borrowing both at once.
    let rom = machine.system_rom().clone();
    let mut system = System::new();

    let mut host = RaylibHost::new("Glaux", WINDOW_SCALE)?;
    tracing::info!("window open");

    // Render before updating so the initial state is the first frame shown.
    while !host.should_close() {
        system.render(machine.framebuffer_mut(), &rom);
        host.present(&IndexedFrame::new(machine.framebuffer(), rom.palette()))?;
        system.update();
    }

    Ok(())
}
