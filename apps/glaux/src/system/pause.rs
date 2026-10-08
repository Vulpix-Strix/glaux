//! The system's pause screen: a paused cartridge's last frame, darkened,
//! under a pause indicator drawn by Glaux itself.
//!
//! The overlay is composed into the same indexed framebuffer as the frozen
//! cartridge frame, which is presented with the cartridge's own master
//! palette. The overlay's colors are system palette colors, so each is drawn
//! with the index of the nearest color in the cartridge palette; a cartridge
//! palette without anything close to them shows the indicator approximately.

use glaux_machine::framebuffer::{Framebuffer, WIDTH};
use glaux_machine::palette::Palette;
use glaux_video::draw::{fill_rect, remap};
use glaux_video::shade::{nearest_index, shade_table};

use super::{BACKGROUND, DIM, PALE, TEXT};

/// How much of each color channel the darkened cartridge frame keeps.
const KEEP_PERCENT: u8 = 65;

/// Distance of the indicator from the screen edges.
const MARGIN: i32 = 8;
/// Side of the square plate behind the pause bars.
const PLATE: i32 = 20;
const BAR_WIDTH: i32 = 3;
const BAR_HEIGHT: i32 = 10;
/// Left edges of the two bars and their top, relative to the plate.
const BAR_XS: [i32; 2] = [5, 12];
const BAR_Y: i32 = 5;

/// Overlay ticks the bars spend in each of their two colors.
pub const PULSE_TICKS: u32 = 40;

/// Top-left corner of the indicator plate.
pub const PLATE_ORIGIN: (i32, i32) = (WIDTH as i32 - MARGIN - PLATE, MARGIN);

/// The pause screen's own animation clock. It counts system ticks spent on
/// the pause screen and is unrelated to the paused cartridge's time.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PauseOverlay {
    ticks: u32,
}

impl PauseOverlay {
    pub fn new() -> Self {
        Self::default()
    }

    /// Advances the indicator animation by one system tick.
    pub fn tick(&mut self) {
        self.ticks = self.ticks.wrapping_add(1);
    }

    /// Darkens the cartridge frame already in `framebuffer`, read through
    /// `palette`, then draws the pause indicator in the upper-right corner
    /// in the colors of `system` resolved to `palette`.
    ///
    /// `palette` must be the palette `framebuffer` is presented with.
    pub fn render(&self, framebuffer: &mut Framebuffer, palette: &Palette, system: &Palette) {
        remap(framebuffer, &shade_table(palette, KEEP_PERCENT));
        let color = |index| nearest_index(palette, system.get(index));

        let (x, y) = PLATE_ORIGIN;
        fill_rect(framebuffer, x, y, PLATE, PLATE, color(DIM));
        fill_rect(
            framebuffer,
            x + 1,
            y + 1,
            PLATE - 2,
            PLATE - 2,
            color(BACKGROUND),
        );
        let bars = color(if (self.ticks / PULSE_TICKS).is_multiple_of(2) {
            TEXT
        } else {
            PALE
        });
        for bar_x in BAR_XS {
            fill_rect(
                framebuffer,
                x + bar_x,
                y + BAR_Y,
                BAR_WIDTH,
                BAR_HEIGHT,
                bars,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
        assert_eq!(rendered(&overlay, 1), first);

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
        assert_eq!(rendered(&overlay, 1), first);
    }
}
