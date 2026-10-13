use super::*;
use crate::font::FontMetrics;
use pretty_assertions::assert_eq;

fn empty_font() -> BitmapFont {
    BitmapFont::new(FontMetrics::SYSTEM)
}

fn notice() -> String {
    "(C) 2026 TEST".into()
}

#[test]
fn ui_images_are_empty_unless_injected_and_found_by_icon() {
    let rom = SystemRom::new(
        Version::new(0, 1, 0),
        notice(),
        Palette::default(),
        empty_font(),
    );
    assert_eq!(rom.ui(), &SystemUi::default());

    let solid = |slot| InkImage::new(1, 1, vec![slot]).unwrap();
    let cursor = |slot| Cursor {
        frames: vec![solid(slot)],
        hotspot: (0, 0),
    };
    let cursors = [cursor(1), cursor(2), cursor(3), cursor(0), cursor(1)];
    let icons = [
        solid(0),
        solid(1),
        solid(2),
        solid(3),
        solid(1),
        solid(2),
        solid(3),
        solid(0),
        solid(2),
    ];
    let rom = rom.with_ui(SystemUi::new(cursors, icons));
    assert_eq!(rom.ui().cursor(SystemCursor::Text), &cursor(2));
    assert_eq!(rom.ui().cursor(SystemCursor::Busy), &cursor(1));
    assert_eq!(rom.ui().icon(SystemIcon::Map), &solid(2));
    assert_eq!(rom.ui().icon(SystemIcon::Music), &solid(1));
    assert_eq!(rom.ui().icon(SystemIcon::Eyedropper), &solid(3));
    assert_eq!(rom.ui().icon(SystemIcon::Fill), &solid(0));
    assert_eq!(rom.ui().icon(SystemIcon::Settings), &solid(2));
}

#[test]
fn a_rom_built_without_sounds_plays_nothing() {
    let rom = SystemRom::new(
        Version::new(0, 1, 0),
        notice(),
        Palette::default(),
        empty_font(),
    );
    assert!(rom.sounds().get(SystemSfx::Boot).steps().is_empty());
}

#[test]
fn animated_cursors_loop_through_their_frames() {
    let frame = |slot| InkImage::new(1, 1, vec![slot]).unwrap();
    let cursor = Cursor {
        frames: vec![frame(1), frame(2), frame(3)],
        hotspot: (0, 0),
    };
    let shown: Vec<_> = (0..7)
        .map(|n| cursor.frame(n).unwrap().pixel(0, 0))
        .collect();
    assert_eq!(shown, [1, 2, 3, 1, 2, 3, 1]);
    assert_eq!(Cursor::default().frame(0), None);
}
