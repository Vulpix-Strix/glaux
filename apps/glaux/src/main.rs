mod app_dirs;
mod cartridge;
mod cartridge_fs;
mod clock;
mod logging;
mod platform;
mod rom;
mod system;

use std::env::consts::{ARCH, OS};
use std::path::Path;
use std::time::Instant;

use cartridge_fs::CartridgeStorage;
use clock::TickClock;
use glaux_host_raylib::{Presentation, RaylibHost, WindowPlacement};
use glaux_machine::machine::Machine;
use glaux_video::IndexedFrame;
use system::{System, SystemInfo};

const WINDOW_SCALE: i32 = 3;

/// How the host draws frames to the window.
const PRESENTATION: Presentation = Presentation::Crt;

fn main() -> anyhow::Result<()> {
    let data_dir = app_dirs::data_dir();
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

    let result = run(platform::window_placement(session), data_dir.as_deref());
    match &result {
        Ok(()) => tracing::info!("clean shutdown"),
        Err(err) => tracing::error!(error = %format!("{err:#}"), "Glaux stopped with an error"),
    }
    result
}

fn run(placement: WindowPlacement, data_dir: Option<&Path>) -> anyhow::Result<()> {
    let cartridges = CartridgeStorage::open(data_dir);

    let mut machine = Machine::new(rom::load()?);
    // The system draws into the machine's framebuffer with data from the
    // machine's ROM; a one-time copy of the ROM avoids borrowing both at once.
    let rom = machine.system_rom().clone();
    let mut system = System::new(
        SystemInfo {
            rom_version: rom.version().clone(),
        },
        cartridges,
    );

    let mut host = RaylibHost::new("Glaux", WINDOW_SCALE, placement, PRESENTATION)?;

    // Presentation follows the host refresh rate while the system advances
    // at the machine's fixed tick rate. Input polled on frames where no tick
    // is due waits for the next tick instead of being dropped.
    let mut clock = TickClock::new();
    let mut pending = Vec::new();
    let mut last = Instant::now();
    // Render before updating so the initial state is the first frame shown.
    while !host.should_close() {
        system.render(machine.framebuffer_mut(), &rom);
        host.present(&IndexedFrame::new(machine.framebuffer(), rom.palette()))?;
        pending.extend(host.poll_input(system.input_mode()));

        let now = Instant::now();
        let ticks = clock.advance(now - last);
        last = now;
        for _ in 0..ticks {
            system = system.update(&pending);
            pending.clear();
        }
    }

    Ok(())
}
