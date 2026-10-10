use super::*;
use crate::system::test_frames::assert_frame_eq;
use glaux_video::draw::clear;
use pretty_assertions::assert_eq;

fn rendered(overlay: &PauseOverlay, color: u8) -> Framebuffer {
    let mut fb = Framebuffer::new();
    clear(&mut fb, color);
    overlay.render(&mut fb, &Palette::provisional(), &Palette::provisional());
    fb
}

fn in_plate(x: i32, y: i32) -> bool {
    let (px, py) = PLATE_ORIGIN;
    (px..px + PLATE).contains(&x) && (py..py + PLATE).contains(&y)
}

#[test]
fn plate_sits_in_the_upper_right_corner_with_a_margin() {
    assert_eq!(PLATE_ORIGIN, (292, 8));
    let fb = rendered(&PauseOverlay::new(), 1);
    let (x, y) = PLATE_ORIGIN;
    assert_eq!(fb.pixel(x, y), Some(DIM));
    assert_eq!(fb.pixel(x + PLATE - 1, y + PLATE - 1), Some(DIM));
    assert_eq!(fb.pixel(x + 1, y + 1), Some(BACKGROUND));
    assert_eq!(fb.pixel(x + 5, y + 5), Some(TEXT));
    assert_eq!(fb.pixel(x + 14, y + 14), Some(TEXT));
    assert_eq!(
        fb.pixel(x + 8, y + 10),
        Some(BACKGROUND),
        "gap between bars"
    );
}

#[test]
fn the_frame_outside_the_plate_is_darkened() {
    let palette = Palette::provisional();
    let shaded = shade_table(&palette, KEEP_PERCENT);
    let fb = rendered(&PauseOverlay::new(), 15);
    assert_ne!(shaded[15], 15);
    for (x, y) in [(0, 0), (319, 239), (291, 8), (300, 28)] {
        assert_eq!(fb.pixel(x, y), Some(shaded[15]), "({x}, {y})");
    }
}

#[test]
fn bars_pulse_with_overlay_time_only_inside_the_plate() {
    let mut overlay = PauseOverlay::new();
    let first = rendered(&overlay, 1);
    for _ in 0..PULSE_TICKS - 1 {
        overlay.tick();
    }
    assert_frame_eq!(rendered(&overlay, 1), first);

    overlay.tick();
    let second = rendered(&overlay, 1);
    let (x, y) = PLATE_ORIGIN;
    assert_eq!(second.pixel(x + 5, y + 5), Some(PALE));
    for py in 0..240 {
        for px in 0..320 {
            if !in_plate(px, py) {
                assert_eq!(second.pixel(px, py), first.pixel(px, py));
            }
        }
    }

    for _ in 0..PULSE_TICKS {
        overlay.tick();
    }
    assert_frame_eq!(rendered(&overlay, 1), first);
}
