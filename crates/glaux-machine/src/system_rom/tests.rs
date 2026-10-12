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
    let pointer = Cursor {
        image: InkImage::new(2, 1, vec![1, 2]).unwrap(),
        hotspot: (1, 0),
    };
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
    let rom = rom.with_ui(SystemUi::new(pointer.clone(), icons));
    assert_eq!(rom.ui().pointer(), &pointer);
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
