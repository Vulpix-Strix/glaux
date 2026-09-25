mod rom;
mod test_pattern;

use glaux_host_raylib::RaylibHost;
use glaux_machine::machine::Machine;
use glaux_video::IndexedFrame;

const WINDOW_SCALE: i32 = 3;

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let mut machine = Machine::new(rom::load()?);
    test_pattern::draw(machine.framebuffer_mut());

    let mut host = RaylibHost::new("Glaux", WINDOW_SCALE)?;
    tracing::info!("window open");

    while !host.should_close() {
        let frame = IndexedFrame::new(machine.framebuffer(), machine.system_rom().palette());
        host.present(&frame)?;
    }

    Ok(())
}
