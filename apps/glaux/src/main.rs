mod demo;
mod rom;

use glaux_host_raylib::RaylibHost;
use glaux_machine::machine::Machine;
use glaux_video::IndexedFrame;

const WINDOW_SCALE: i32 = 3;

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let mut machine = Machine::new(rom::load()?);
    let font = machine.system_rom().font().clone();
    demo::draw(machine.framebuffer_mut(), &font);

    let mut host = RaylibHost::new("Glaux", WINDOW_SCALE)?;
    tracing::info!("window open");

    while !host.should_close() {
        let frame = IndexedFrame::new(machine.framebuffer(), machine.system_rom().palette());
        host.present(&frame)?;
    }

    Ok(())
}
