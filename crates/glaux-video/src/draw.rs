//! Drawing primitives over the logical framebuffer.
//!
//! All primitives clip through [`Framebuffer::set_pixel`]: anything that
//! falls outside the framebuffer is silently dropped.

use glaux_machine::font::BitmapFont;
use glaux_machine::framebuffer::{Framebuffer, HEIGHT, WIDTH};

/// Fills the whole framebuffer with a palette index.
pub fn clear(framebuffer: &mut Framebuffer, color: u8) {
    framebuffer.clear(color);
}

/// Writes one pixel; positions outside the framebuffer are ignored.
pub fn pixel(framebuffer: &mut Framebuffer, x: i32, y: i32, color: u8) {
    framebuffer.set_pixel(x, y, color);
}

/// Fills the `width`×`height` rectangle whose top-left corner is `(x, y)`.
///
/// Non-positive sizes draw nothing; the parts outside the framebuffer are
/// clipped.
pub fn fill_rect(
    framebuffer: &mut Framebuffer,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    color: u8,
) {
    // Clamp to the i32 range in i64 so that `x + width` cannot overflow.
    let clamp = |v: i64| v.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32;
    let (x0, y0) = (x.max(0), y.max(0));
    let x1 = clamp(i64::from(x) + i64::from(width)).min(WIDTH as i32);
    let y1 = clamp(i64::from(y) + i64::from(height)).min(HEIGHT as i32);
    for py in y0..y1 {
        for px in x0..x1 {
            framebuffer.set_pixel(px, py, color);
        }
    }
}

/// Draws `text` with `font`, with `(x, y)` as the top-left corner of the
/// first character cell.
///
/// - Only set glyph bits are written; the rest of each cell keeps whatever
///   was already in the framebuffer.
/// - Each character advances the pen by the font's `cell_width`.
/// - `'\n'` returns the pen to `x` and moves it down by `cell_height`.
/// - Any other character without a glyph in `font` (including `'\r'`,
///   `'\t'` and non-ASCII) leaves its cell empty but still advances the pen.
pub fn draw_text(
    framebuffer: &mut Framebuffer,
    font: &BitmapFont,
    text: &str,
    x: i32,
    y: i32,
    color: u8,
) {
    let metrics = font.metrics();
    let (glyph_width, glyph_height) = (metrics.glyph_width(), metrics.glyph_height());

    // The pen is tracked in i64 so that advancing it near i32::MAX cannot
    // overflow; pixels that no longer fit in i32 are off-screen anyway.
    let (mut pen_x, mut pen_y) = (i64::from(x), i64::from(y));

    for ch in text.chars() {
        if ch == '\n' {
            pen_x = i64::from(x);
            pen_y += i64::from(metrics.cell_height());
            continue;
        }

        if let Some(glyph) = font.glyph(ch) {
            for gy in 0..glyph_height {
                for gx in 0..glyph_width {
                    if !glyph.is_set(usize::from(gx), usize::from(gy)) {
                        continue;
                    }
                    let px = i32::try_from(pen_x + i64::from(gx));
                    let py = i32::try_from(pen_y + i64::from(gy));
                    if let (Ok(px), Ok(py)) = (px, py) {
                        framebuffer.set_pixel(px, py, color);
                    }
                }
            }
        }

        pen_x += i64::from(metrics.cell_width());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glaux_machine::font::FontMetrics;
    use pretty_assertions::assert_eq;

    const INK: u8 = 7;

    /// 3×3 glyphs in 4×5 cells. `'?'` is deliberately missing.
    fn tiny_font() -> BitmapFont {
        let mut font = BitmapFont::new(FontMetrics::new(3, 3, 4, 5).unwrap());
        font.insert('x', &[0b101, 0b010, 0b101]).unwrap();
        font.insert('#', &[0b111, 0b111, 0b111]).unwrap();
        font
    }

    /// A blank framebuffer with `pixels` set to `INK`.
    fn with_ink(pixels: impl IntoIterator<Item = (i32, i32)>) -> Framebuffer {
        let mut fb = Framebuffer::new();
        for (x, y) in pixels {
            assert!(
                fb.set_pixel(x, y, INK),
                "expected pixel ({x}, {y}) is on screen"
            );
        }
        fb
    }

    /// The 9 pixels of a `'#'` whose cell starts at `(x, y)`.
    fn block(x: i32, y: i32) -> impl Iterator<Item = (i32, i32)> {
        (0..3).flat_map(move |dy| (0..3).map(move |dx| (x + dx, y + dy)))
    }

    fn render(text: &str, x: i32, y: i32) -> Framebuffer {
        let mut fb = Framebuffer::new();
        draw_text(&mut fb, &tiny_font(), text, x, y, INK);
        fb
    }

    #[test]
    fn clear_fills_every_pixel() {
        let mut fb = Framebuffer::new();
        pixel(&mut fb, 5, 5, 1);
        clear(&mut fb, 42);
        assert!(fb.as_bytes().iter().all(|&b| b == 42));
    }

    #[test]
    fn pixel_writes_exact_position() {
        let mut fb = Framebuffer::new();
        pixel(&mut fb, 12, 34, INK);
        assert_eq!(fb, with_ink([(12, 34)]));
    }

    #[test]
    fn pixel_out_of_bounds_is_ignored() {
        let mut fb = Framebuffer::new();
        let (w, h) = (WIDTH as i32, HEIGHT as i32);
        for (x, y) in [
            (-1, 0),
            (0, -1),
            (w, 0),
            (0, h),
            (i32::MIN, i32::MIN),
            (i32::MAX, i32::MAX),
        ] {
            pixel(&mut fb, x, y, INK);
        }
        assert_eq!(fb, Framebuffer::new());
    }

    #[test]
    fn fill_rect_covers_exact_region() {
        let mut fb = Framebuffer::new();
        fill_rect(&mut fb, 10, 20, 3, 2, INK);
        let expected = with_ink((20..22).flat_map(|y| (10..13).map(move |x| (x, y))));
        assert_eq!(fb, expected);
    }

    #[test]
    fn fill_rect_clips_at_edges() {
        let mut fb = Framebuffer::new();
        fill_rect(&mut fb, -1, -1, 2, 2, INK);
        fill_rect(&mut fb, 319, 239, 5, 5, INK);
        assert_eq!(fb, with_ink([(0, 0), (319, 239)]));
    }

    #[test]
    fn fill_rect_with_empty_size_draws_nothing() {
        let mut fb = Framebuffer::new();
        for (w, h) in [(0, 5), (5, 0), (-3, 5), (5, -3)] {
            fill_rect(&mut fb, 10, 10, w, h, INK);
        }
        assert_eq!(fb, Framebuffer::new());
    }

    #[test]
    fn fill_rect_extreme_values_do_not_panic() {
        let mut fb = Framebuffer::new();
        fill_rect(&mut fb, i32::MAX, i32::MAX, i32::MAX, i32::MAX, INK);
        fill_rect(&mut fb, i32::MIN, i32::MIN, i32::MAX, i32::MAX, INK);
        assert_eq!(fb, Framebuffer::new());

        fill_rect(&mut fb, -10, -10, i32::MAX, i32::MAX, INK);
        assert!(fb.as_bytes().iter().all(|&b| b == INK));
    }

    #[test]
    fn glyph_produces_exact_pixels() {
        let expected = with_ink([(10, 20), (12, 20), (11, 21), (10, 22), (12, 22)]);
        assert_eq!(render("x", 10, 20), expected);
    }

    #[test]
    fn characters_advance_by_cell_width() {
        let fb = render("##", 10, 20);
        assert_eq!(fb, with_ink(block(10, 20).chain(block(14, 20))));
        assert!((20..23).all(|y| fb.pixel(13, y) == Some(0)));
    }

    #[test]
    fn newline_moves_down_by_cell_height() {
        let fb = render("#\n#", 10, 20);
        assert_eq!(fb, with_ink(block(10, 20).chain(block(10, 25))));
        assert!((10..13).all(|x| fb.pixel(x, 23) == Some(0) && fb.pixel(x, 24) == Some(0)));
    }

    #[test]
    fn missing_glyph_leaves_cell_empty_but_advances() {
        let expected = with_ink(block(10, 20).chain(block(18, 20)));
        assert_eq!(render("#?#", 10, 20), expected);
        assert_eq!(render("#\r#", 10, 20), expected);
        assert_eq!(render("#é#", 10, 20), expected);
    }

    #[test]
    fn clips_left_and_top() {
        assert_eq!(render("#", -2, -2), with_ink([(0, 0)]));
    }

    #[test]
    fn clips_right_and_bottom() {
        let expected = with_ink([(318, 238), (319, 238), (318, 239), (319, 239)]);
        assert_eq!(render("#", 318, 238), expected);
    }

    #[test]
    fn extreme_positions_do_not_panic() {
        assert_eq!(
            render("##\n##", i32::MAX - 1, i32::MAX - 1),
            Framebuffer::new()
        );
        assert_eq!(render("##\n##", i32::MIN, i32::MIN), Framebuffer::new());
    }

    #[test]
    fn keeps_background_outside_set_bits() {
        let mut fb = Framebuffer::new();
        clear(&mut fb, 3);
        draw_text(&mut fb, &tiny_font(), "x", 10, 20, INK);

        let mut expected = Framebuffer::new();
        expected.clear(3);
        for (x, y) in [(10, 20), (12, 20), (11, 21), (10, 22), (12, 22)] {
            expected.set_pixel(x, y, INK);
        }
        assert_eq!(fb, expected);
    }

    #[test]
    fn uses_font_metrics() {
        // 2×2 glyphs in 3×2 cells: no vertical spacing row.
        let mut font = BitmapFont::new(FontMetrics::new(2, 2, 3, 2).unwrap());
        font.insert('o', &[0b11, 0b11]).unwrap();

        let mut fb = Framebuffer::new();
        draw_text(&mut fb, &font, "oo\no", 0, 0, INK);

        let expected = with_ink([
            (0, 0),
            (1, 0),
            (0, 1),
            (1, 1),
            (3, 0),
            (4, 0),
            (3, 1),
            (4, 1),
            (0, 2),
            (1, 2),
            (0, 3),
            (1, 3),
        ]);
        assert_eq!(fb, expected);
    }
}
