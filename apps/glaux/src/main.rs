mod test_pattern;

use glaux_host_raylib::RaylibHost;
use glaux_machine::framebuffer::Framebuffer;
use glaux_machine::palette::Palette;
use glaux_video::IndexedFrame;

const WINDOW_SCALE: i32 = 3;

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let palette = Palette::provisional();
    let mut framebuffer = Framebuffer::new();
    test_pattern::draw(&mut framebuffer);

    let mut host = RaylibHost::new("Glaux", WINDOW_SCALE)?;
    tracing::info!("window open");

    while !host.should_close() {
        host.present(&IndexedFrame::new(&framebuffer, &palette))?;
    }

    Ok(())
}
