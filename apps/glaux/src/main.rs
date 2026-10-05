mod logging;
mod platform;
mod rom;
mod system;

use std::env::consts::{ARCH, OS};

use glaux_host_raylib::{RaylibHost, WindowPlacement};
use glaux_machine::machine::Machine;
use glaux_video::IndexedFrame;
use system::System;

const WINDOW_SCALE: i32 = 3;

fn main() -> anyhow::Result<()> {
    let data_dir = logging::data_dir();
    // Held until `main` returns so the file log is flushed on exit.
    let _logging = logging::init(data_dir.as_deref());

    tracing::info!(
        version = env!("CARGO_PKG_VERSION"),
        os = OS,
        arch = ARCH,
        data_dir = ?data_dir,
        "Glaux starting"
    );
    let session = platform::detect();
    if cfg!(target_os = "linux") {
        tracing::info!(session = %session, "display session detected");
    }

    let result = run(platform::window_placement(session));
    match &result {
        Ok(()) => tracing::info!("clean shutdown"),
        Err(err) => tracing::error!(error = %format!("{err:#}"), "Glaux stopped with an error"),
    }
    result
}

fn run(placement: WindowPlacement) -> anyhow::Result<()> {
    let mut machine = Machine::new(rom::load()?);
    // The system draws into the machine's framebuffer with data from the
    // machine's ROM; a one-time copy of the ROM avoids borrowing both at once.
    let rom = machine.system_rom().clone();
    let mut system = System::new(rom.version().clone());

    let mut host = RaylibHost::new("Glaux", WINDOW_SCALE, placement)?;

    // Render before updating so the initial state is the first frame shown.
    while !host.should_close() {
        system.render(machine.framebuffer_mut(), &rom);
        host.present(&IndexedFrame::new(machine.framebuffer(), rom.palette()))?;
        let input = host.poll_input();
        system.update(&input);
    }

    Ok(())
}
