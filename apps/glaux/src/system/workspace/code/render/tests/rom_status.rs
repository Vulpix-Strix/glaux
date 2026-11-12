use super::*;
use crate::cartridge::rom::RomUsage;
use pretty_assertions::assert_eq;

fn status_pixels(
    editor: &CodeEditor,
    session: &CodeSession,
    notice: Option<(&str, u8)>,
    rom: Option<&RomUsage>,
) -> Vec<Option<u8>> {
    let rom_font = rom_font();
    let mut framebuffer = Framebuffer::new();
    clear(&mut framebuffer, BACKGROUND);
    editor.render(
        &mut framebuffer,
        rom_font.font(),
        rom_font.ui(),
        Some(session),
        None,
        StatusInfo { notice, rom },
    );
    let bar = editor.layout().status;
    (bar.y..bar.bottom())
        .flat_map(|y| (bar.x..bar.right()).map(move |x| (x, y)))
        .map(|(x, y)| framebuffer.pixel(x, y))
        .collect()
}

fn rom_font() -> SystemRom {
    rom()
}

fn showing_a(fixture: &Fixture, editor: &CodeEditor) -> CodeSession {
    showing(fixture, editor, "x\n")
}

#[test]
fn cart_rom_shows_at_the_right_edge_of_the_status_bar() {
    let fixture = Fixture::new("code-rom-bar");
    let editor = editor();
    let session = showing_a(&fixture, &editor);
    let usage = RomUsage {
        used: 100 * 1024,
        ..RomUsage::default()
    };

    let without = status_pixels(&editor, &session, None, None);
    let with = status_pixels(&editor, &session, None, Some(&usage));

    assert_ne!(with, without);
    let bar = editor.layout().status;
    let width = usize::try_from(bar.width).unwrap();
    // Only the right half of the bar changed, where the hint is not.
    for (i, (a, b)) in with.iter().zip(&without).enumerate() {
        if a != b {
            assert!(i % width > width / 2, "pixel {i} changed on the left");
        }
    }
}

#[test]
fn a_notice_that_fills_the_bar_keeps_every_column() {
    let fixture = Fixture::new("code-rom-notice");
    let editor = editor();
    let session = showing_a(&fixture, &editor);
    let usage = RomUsage {
        used: 100 * 1024,
        ..RomUsage::default()
    };
    let long = "n".repeat(60);
    let notice = Some((long.as_str(), PALE));

    assert_eq!(
        status_pixels(&editor, &session, notice, Some(&usage)),
        status_pixels(&editor, &session, notice, None)
    );
}
