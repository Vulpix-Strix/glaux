//! Drawing primitives over the logical framebuffer.
//!
//! All primitives clip to the framebuffer: anything that falls outside it is
//! silently dropped.

use glaux_machine::font::BitmapFont;
use glaux_machine::framebuffer::{Framebuffer, HEIGHT, WIDTH};
use glaux_machine::ink_image::{INK_SLOTS, InkImage};
use glaux_machine::obj_palette::ObjBank;
use glaux_machine::palette::PALETTE_LEN;
use glaux_machine::sprite::{OBJ_TILE_HEIGHT, OBJ_TILE_WIDTH, ObjVram, obj_grid_tile};

/// Fills the whole framebuffer with a palette index.
pub fn clear(framebuffer: &mut Framebuffer, color: u8) {
    framebuffer.clear(color);
}

/// Writes one pixel; positions outside the framebuffer are ignored.
pub fn pixel(framebuffer: &mut Framebuffer, x: i32, y: i32, color: u8) {
    framebuffer.set_pixel(x, y, color);
}

/// Which object palette bank a sprite resolves its tile-local indices
/// through.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ObjPaletteChoice {
    /// Each tile uses the default bank stored in its attributes.
    #[default]
    TileDefault,
    /// Every tile of the sprite uses this bank.
    Bank(ObjBank),
}

/// A `width`x`height` block of object tiles from the object-tile grid with
/// `first` at its top-left and `(x, y)` as its top-left corner.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ObjSprite {
    pub first: usize,
    pub x: i32,
    pub y: i32,
    pub width: usize,
    pub height: usize,
    /// Mirrors the whole composed block horizontally: the order of its tile
    /// columns and the pixels within each tile are both reversed.
    pub flip_x: bool,
    /// Mirrors the whole composed block vertically.
    pub flip_y: bool,
    pub palette: ObjPaletteChoice,
}

impl ObjSprite {
    /// A single unflipped tile drawn with its default palette bank.
    pub fn tile(tile: usize, x: i32, y: i32) -> Self {
        Self {
            first: tile,
            x,
            y,
            width: 1,
            height: 1,
            flip_x: false,
            flip_y: false,
            palette: ObjPaletteChoice::TileDefault,
        }
    }
}

/// Draws one native 8x8 object tile at `(x, y)` with its default palette
/// bank.
pub fn draw_obj_tile(framebuffer: &mut Framebuffer, vram: &ObjVram, tile: usize, x: i32, y: i32) {
    draw_obj_sprite(framebuffer, vram, &ObjSprite::tile(tile, x, y));
}

/// Draws `sprite` from `vram`.
///
/// Tiles are placed at 8-pixel offsets. Block positions that fall outside
/// the grid (see [`obj_grid_tile`]) are skipped. Tile-local index 0 is
/// transparent; every other local index is written as the master index its
/// tile's bank maps it to. The bank is chosen per tile by
/// [`ObjPaletteChoice`]. The tile bank itself is never modified.
pub fn draw_obj_sprite(framebuffer: &mut Framebuffer, vram: &ObjVram, sprite: &ObjSprite) {
    for row in 0..sprite.height {
        for column in 0..sprite.width {
            let Some(tile) = obj_grid_tile(sprite.first, column, row) else {
                continue;
            };
            let dest_column = if sprite.flip_x {
                sprite.width - 1 - column
            } else {
                column
            };
            let dest_row = if sprite.flip_y {
                sprite.height - 1 - row
            } else {
                row
            };
            let bank = match sprite.palette {
                ObjPaletteChoice::TileDefault => vram.attributes().bank(tile),
                ObjPaletteChoice::Bank(bank) => bank,
            };
            let tile_x = i64::from(sprite.x) + (dest_column * OBJ_TILE_WIDTH) as i64;
            let tile_y = i64::from(sprite.y) + (dest_row * OBJ_TILE_HEIGHT) as i64;
            blit_obj_tile(framebuffer, vram, tile, bank, (tile_x, tile_y), sprite);
        }
    }
}

/// Draws one object tile through `bank`, mirrored on the axes `sprite`
/// flags, with its top-left corner at `origin`. The corner may lie outside
/// the `i32` range, as the far tiles of a block near `i32::MAX` do.
fn blit_obj_tile(
    framebuffer: &mut Framebuffer,
    vram: &ObjVram,
    tile: usize,
    bank: ObjBank,
    (x, y): (i64, i64),
    sprite: &ObjSprite,
) {
    for local_y in 0..OBJ_TILE_HEIGHT {
        for local_x in 0..OBJ_TILE_WIDTH {
            let source_x = if sprite.flip_x {
                OBJ_TILE_WIDTH - 1 - local_x
            } else {
                local_x
            };
            let source_y = if sprite.flip_y {
                OBJ_TILE_HEIGHT - 1 - local_y
            } else {
                local_y
            };
            let Some(local) = vram.tiles().pixel(tile, source_x, source_y) else {
                return;
            };
            let Some(color) = vram.palettes().resolve(bank, local) else {
                continue;
            };
            let px = x + local_x as i64;
            let py = y + local_y as i64;
            if let (Ok(px), Ok(py)) = (i32::try_from(px), i32::try_from(py)) {
                framebuffer.set_pixel(px, py, color);
            }
        }
    }
}

/// Replaces every pixel's palette index `i` with `table[i]`.
pub fn remap(framebuffer: &mut Framebuffer, table: &[u8; PALETTE_LEN]) {
    for index in framebuffer.as_bytes_mut() {
        *index = table[usize::from(*index)];
    }
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
    // Clip in i64 so that `x + width` cannot overflow; every bound then lies
    // in `0..=WIDTH` or `0..=HEIGHT`.
    let clip = |start: i32, size: i32, limit: usize| {
        let end = (i64::from(start) + i64::from(size)).clamp(0, limit as i64) as usize;
        let start = i64::from(start).clamp(0, limit as i64) as usize;
        start..end.max(start)
    };
    let columns = clip(x, width, WIDTH);
    let rows = clip(y, height, HEIGHT);
    if columns.is_empty() {
        return;
    }
    let pixels = framebuffer.as_bytes_mut();
    for row in rows {
        let offset = row * WIDTH;
        pixels[offset + columns.start..offset + columns.end].fill(color);
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
    draw_text_scaled(framebuffer, font, text, x, y, 1, color);
}

/// Draws `text` like [`draw_text`], with every glyph bit enlarged to a
/// `scale`×`scale` block and cell advances multiplied by `scale`.
///
/// A non-positive `scale` draws nothing.
pub fn draw_text_scaled(
    framebuffer: &mut Framebuffer,
    font: &BitmapFont,
    text: &str,
    x: i32,
    y: i32,
    scale: i32,
    color: u8,
) {
    if scale <= 0 {
        return;
    }
    let metrics = font.metrics();
    let (glyph_width, glyph_height) = (metrics.glyph_width(), metrics.glyph_height());
    let scale64 = i64::from(scale);

    // The pen is tracked in i64 so that advancing it near i32::MAX cannot
    // overflow; pixels that no longer fit in i32 are off-screen anyway.
    let (mut pen_x, mut pen_y) = (i64::from(x), i64::from(y));

    for ch in text.chars() {
        if ch == '\n' {
            pen_x = i64::from(x);
            pen_y += i64::from(metrics.cell_height()) * scale64;
            continue;
        }

        if let Some(glyph) = font.glyph(ch)
            && glyph_visible(pen_x, pen_y, glyph_width, glyph_height, scale64)
        {
            for gy in 0..glyph_height {
                for gx in 0..glyph_width {
                    if !glyph.is_set(usize::from(gx), usize::from(gy)) {
                        continue;
                    }
                    let px = i32::try_from(pen_x + i64::from(gx) * scale64);
                    let py = i32::try_from(pen_y + i64::from(gy) * scale64);
                    if let (Ok(px), Ok(py)) = (px, py) {
                        if scale == 1 {
                            framebuffer.set_pixel(px, py, color);
                        } else {
                            fill_rect(framebuffer, px, py, scale, scale, color);
                        }
                    }
                }
            }
        }

        pen_x += i64::from(metrics.cell_width()) * scale64;
    }
}

/// Draws `image` with its top-left corner at `(x, y)`, writing ink slot
/// `n` as `inks[n - 1]` and leaving transparent pixels untouched. Parts
/// outside the framebuffer are clipped.
pub fn draw_ink_image(
    framebuffer: &mut Framebuffer,
    image: &InkImage,
    x: i32,
    y: i32,
    inks: &[u8; INK_SLOTS],
) {
    for iy in 0..image.height() {
        for ix in 0..image.width() {
            let slot = image.pixel(ix, iy);
            if slot == 0 {
                continue;
            }
            // Images are a few pixels across, so offsets fit in i32; pixels
            // pushed past i32 by an extreme origin are off-screen anyway.
            let px = i64::from(x) + ix as i64;
            let py = i64::from(y) + iy as i64;
            if let (Ok(px), Ok(py)) = (i32::try_from(px), i32::try_from(py)) {
                framebuffer.set_pixel(px, py, inks[usize::from(slot) - 1]);
            }
        }
    }
}

/// Whether any part of a glyph drawn at the pen position can land on the
/// framebuffer. Skipping the rest keeps long off-screen text cheap.
fn glyph_visible(pen_x: i64, pen_y: i64, glyph_width: u8, glyph_height: u8, scale: i64) -> bool {
    let right = pen_x + i64::from(glyph_width) * scale;
    let bottom = pen_y + i64::from(glyph_height) * scale;
    right > 0 && bottom > 0 && pen_x < WIDTH as i64 && pen_y < HEIGHT as i64
}

#[cfg(test)]
mod tests {
    use super::*;
    use glaux_machine::font::FontMetrics;
    use glaux_machine::obj_palette::{OBJ_PALETTES_BYTES, ObjPalettes};
    use glaux_machine::sprite::{
        OBJ_TILE_BYTES, OBJ_TILE_CAPACITY, ObjTileAttributes, ObjTileBank,
    };
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

    fn filled(color: u8) -> Framebuffer {
        let mut fb = Framebuffer::new();
        fb.clear(color);
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
    fn object_tile_draws_exact_indices_and_keeps_zero_transparent() {
        let row = [0x01, 0x23, 0x40, 0x56];
        let bytes = row
            .into_iter()
            .cycle()
            .take(OBJ_TILE_BYTES)
            .collect::<Vec<_>>();
        let tiles = ObjTileBank::from_bytes(&bytes).unwrap();
        let mut fb = filled(9);

        draw_tile(&mut fb, &tiles, 0, 10, 20);

        let expected_row = [9, 1, 2, 3, 4, 9, 5, 6];
        for y in 20..28 {
            for (offset, expected) in expected_row.into_iter().enumerate() {
                assert_eq!(fb.pixel(10 + offset as i32, y), Some(expected));
            }
        }
        assert_eq!(fb.pixel(9, 20), Some(9));
        assert_eq!(fb.pixel(18, 20), Some(9));
    }

    #[test]
    fn object_tile_clips_at_every_edge() {
        let tiles = ObjTileBank::from_bytes(&[0x77; OBJ_TILE_BYTES]).unwrap();
        let cases = [
            ((-7, -7), Some((0, 0))),
            ((319, 239), Some((319, 239))),
            ((-8, 10), None),
            ((10, -8), None),
            ((WIDTH as i32, 10), None),
            ((10, HEIGHT as i32), None),
        ];
        for ((x, y), expected) in cases {
            let mut fb = Framebuffer::new();
            draw_tile(&mut fb, &tiles, 0, x, y);
            assert_eq!(
                fb.as_bytes().iter().filter(|&&index| index == 7).count(),
                usize::from(expected.is_some()),
                "sprite at ({x}, {y})"
            );
            if let Some((px, py)) = expected {
                assert_eq!(fb.pixel(px, py), Some(7));
            }
        }
    }

    #[test]
    fn object_tile_handles_extreme_coordinates_and_invalid_tiles() {
        let tiles = ObjTileBank::from_bytes(&[0x77; OBJ_TILE_BYTES]).unwrap();
        let mut fb = Framebuffer::new();
        for (tile, x, y) in [
            (0, i32::MIN, i32::MIN),
            (0, i32::MAX, i32::MAX),
            (OBJ_TILE_CAPACITY, 0, 0),
        ] {
            draw_tile(&mut fb, &tiles, tile, x, y);
        }
        assert_eq!(fb, Framebuffer::new());
    }

    /// Draws one tile from `tiles` with default attributes and linear
    /// palettes, under which local indices resolve to themselves.
    fn draw_tile(fb: &mut Framebuffer, tiles: &ObjTileBank, tile: usize, x: i32, y: i32) {
        draw_obj_tile(fb, &linear_vram(tiles), tile, x, y);
    }

    /// Draws a block like [`draw_tile`] does a tile.
    #[expect(clippy::too_many_arguments, reason = "mirrors spr()'s arguments")]
    fn draw_block(
        fb: &mut Framebuffer,
        tiles: &ObjTileBank,
        first: usize,
        x: i32,
        y: i32,
        width: usize,
        height: usize,
        flip_x: bool,
        flip_y: bool,
    ) {
        let sprite = ObjSprite {
            first,
            x,
            y,
            width,
            height,
            flip_x,
            flip_y,
            palette: ObjPaletteChoice::TileDefault,
        };
        draw_obj_sprite(fb, &linear_vram(tiles), &sprite);
    }

    fn linear_vram(tiles: &ObjTileBank) -> ObjVram {
        ObjVram::new(
            tiles.clone(),
            ObjTileAttributes::new(),
            ObjPalettes::linear(),
        )
    }

    /// A bank whose tile `n` is solid index `color` for each `(n, color)`.
    fn solid_tiles(tiles: &[(usize, u8)]) -> ObjTileBank {
        let mut bytes = vec![0; OBJ_TILE_CAPACITY * OBJ_TILE_BYTES];
        for &(tile, color) in tiles {
            bytes[tile * OBJ_TILE_BYTES..(tile + 1) * OBJ_TILE_BYTES].fill((color << 4) | color);
        }
        ObjTileBank::from_bytes(&bytes).unwrap()
    }

    /// The single index filling the 8x8 cell at `(x, y)`, if it is uniform.
    fn cell(fb: &Framebuffer, x: i32, y: i32) -> Option<u8> {
        let first = fb.pixel(x, y)?;
        (0..8)
            .flat_map(|dy| (0..8).map(move |dx| (x + dx, y + dy)))
            .all(|(px, py)| fb.pixel(px, py) == Some(first))
            .then_some(first)
    }

    #[test]
    fn a_two_by_two_sprite_uses_consecutive_grid_rows() {
        let tiles = solid_tiles(&[(24, 1), (25, 2), (40, 3), (41, 4), (26, 9), (27, 9)]);
        let mut fb = Framebuffer::new();

        draw_block(&mut fb, &tiles, 24, 10, 20, 2, 2, false, false);

        assert_eq!(cell(&fb, 10, 20), Some(1));
        assert_eq!(cell(&fb, 18, 20), Some(2));
        assert_eq!(cell(&fb, 10, 28), Some(3));
        assert_eq!(cell(&fb, 18, 28), Some(4));
        assert_eq!(
            fb.as_bytes().iter().filter(|&&index| index != 0).count(),
            4 * 64
        );
    }

    #[test]
    fn a_three_by_two_sprite_draws_row_major_blocks() {
        let tiles = solid_tiles(&[(5, 1), (6, 2), (7, 3), (21, 4), (22, 5), (23, 6)]);
        let mut fb = Framebuffer::new();

        draw_block(&mut fb, &tiles, 5, 0, 0, 3, 2, false, false);

        for (index, expected) in (1..=6).enumerate() {
            let (column, row) = ((index % 3) as i32, (index / 3) as i32);
            assert_eq!(cell(&fb, column * 8, row * 8), Some(expected));
        }
        assert_eq!(fb.pixel(24, 0), Some(0));
        assert_eq!(fb.pixel(0, 16), Some(0));
    }

    #[test]
    fn a_one_by_one_sprite_matches_a_single_tile() {
        let row = [0x01, 0x23, 0x40, 0x56];
        let bytes = row
            .into_iter()
            .cycle()
            .take(OBJ_TILE_BYTES)
            .collect::<Vec<_>>();
        let tiles = ObjTileBank::from_bytes(&bytes).unwrap();
        let (mut single, mut block) = (filled(9), filled(9));

        draw_tile(&mut single, &tiles, 0, -3, 5);
        draw_block(&mut block, &tiles, 0, -3, 5, 1, 1, false, false);

        assert_eq!(single, block);
    }

    #[test]
    fn sprite_blocks_keep_index_zero_transparent() {
        // Tile 1 stays all zero, so the block's top-right cell shows through.
        let tiles = solid_tiles(&[(0, 5), (16, 6), (17, 7)]);
        let mut fb = filled(9);

        draw_block(&mut fb, &tiles, 0, 0, 0, 2, 2, false, false);

        assert_eq!(cell(&fb, 0, 0), Some(5));
        assert_eq!(cell(&fb, 8, 0), Some(9));
        assert_eq!(cell(&fb, 0, 8), Some(6));
        assert_eq!(cell(&fb, 8, 8), Some(7));
    }

    #[test]
    fn sprite_blocks_clip_at_every_edge() {
        let tiles = solid_tiles(&[(0, 7), (1, 7), (16, 7), (17, 7)]);
        let (w, h) = (WIDTH as i32, HEIGHT as i32);
        let cases = [
            ((-15, -15), 1, Some((0, 0))),
            ((w - 1, h - 1), 1, Some((w - 1, h - 1))),
            ((-8, 0), 128, Some((0, 15))),
            ((0, h - 4), 64, Some((15, h - 1))),
            ((-16, 0), 0, None),
            ((0, -16), 0, None),
            ((w, 0), 0, None),
            ((0, h), 0, None),
        ];
        for ((x, y), count, inside) in cases {
            let mut fb = Framebuffer::new();
            draw_block(&mut fb, &tiles, 0, x, y, 2, 2, false, false);
            assert_eq!(
                fb.as_bytes().iter().filter(|&&index| index == 7).count(),
                count,
                "sprite at ({x}, {y})"
            );
            if let Some((px, py)) = inside {
                assert_eq!(fb.pixel(px, py), Some(7), "sprite at ({x}, {y})");
            }
        }
    }

    #[test]
    fn sprite_blocks_handle_extreme_coordinates_and_off_grid_tiles() {
        let tiles = solid_tiles(&[(15, 7), (1023, 7)]);
        let mut fb = Framebuffer::new();
        draw_block(&mut fb, &tiles, 0, i32::MAX, i32::MAX, 16, 64, false, false);
        draw_block(&mut fb, &tiles, 0, i32::MIN, i32::MIN, 16, 64, false, false);
        assert_eq!(fb, Framebuffer::new());

        // Positions past the grid's right or bottom edge draw nothing.
        draw_block(&mut fb, &tiles, 15, 0, 0, 2, 1, false, false);
        draw_block(&mut fb, &tiles, 1023, 100, 100, 1, 2, false, false);
        assert_eq!(
            fb.as_bytes().iter().filter(|&&index| index == 7).count(),
            128
        );
    }

    /// Every `(flip_x, flip_y)` combination.
    const FLIPS: [(bool, bool); 4] = [(false, false), (true, false), (false, true), (true, true)];

    /// A full bank whose tile `t` has index `pixel(t, x, y)` at `(x, y)`.
    fn bank_from(pixel: impl Fn(usize, usize, usize) -> u8) -> ObjTileBank {
        let mut bytes = Vec::with_capacity(OBJ_TILE_CAPACITY * OBJ_TILE_BYTES);
        for tile in 0..OBJ_TILE_CAPACITY {
            for y in 0..8 {
                for x in (0..8).step_by(2) {
                    bytes.push((pixel(tile, x, y) << 4) | pixel(tile, x + 1, y));
                }
            }
        }
        ObjTileBank::from_bytes(&bytes).unwrap()
    }

    /// Tiles with no mirror symmetry, different from each other and with
    /// transparent holes, so any wrong flip, tile order or skipped hole shows.
    fn asymmetric_tiles() -> ObjTileBank {
        bank_from(|tile, x, y| ((tile * 3 + x + 2 * y) % 16) as u8)
    }

    /// Nonzero pixels of `fb` as `((x, y), index)`.
    fn ink(fb: &Framebuffer) -> Vec<((i32, i32), u8)> {
        (0..HEIGHT as i32)
            .flat_map(|y| (0..WIDTH as i32).map(move |x| (x, y)))
            .filter_map(|(x, y)| fb.pixel(x, y).filter(|&i| i != 0).map(|i| ((x, y), i)))
            .collect()
    }

    #[test]
    fn flipping_a_tile_mirrors_its_pixels() {
        let tiles = bank_from(|tile, x, y| u8::from(tile == 0 && (x, y) == (1, 0)) * 5);
        for ((flip_x, flip_y), expected) in
            FLIPS
                .into_iter()
                .zip([(11, 20), (16, 20), (11, 27), (16, 27)])
        {
            let mut fb = Framebuffer::new();
            draw_block(&mut fb, &tiles, 0, 10, 20, 1, 1, flip_x, flip_y);
            assert_eq!(ink(&fb), [(expected, 5)], "flip ({flip_x}, {flip_y})");
        }
    }

    #[test]
    fn flipping_a_block_reverses_its_tile_order() {
        let tiles = solid_tiles(&[(5, 1), (6, 2), (7, 3), (21, 4), (22, 5), (23, 6)]);
        let cases = [
            ((false, false), [[1, 2, 3], [4, 5, 6]]),
            ((true, false), [[3, 2, 1], [6, 5, 4]]),
            ((false, true), [[4, 5, 6], [1, 2, 3]]),
            ((true, true), [[6, 5, 4], [3, 2, 1]]),
        ];
        for ((flip_x, flip_y), rows) in cases {
            let mut fb = Framebuffer::new();
            draw_block(&mut fb, &tiles, 5, 0, 0, 3, 2, flip_x, flip_y);
            for (row, colors) in rows.into_iter().enumerate() {
                for (column, color) in colors.into_iter().enumerate() {
                    assert_eq!(
                        cell(&fb, column as i32 * 8, row as i32 * 8),
                        Some(color),
                        "flip ({flip_x}, {flip_y}) cell ({column}, {row})"
                    );
                }
            }
            assert_eq!(ink(&fb).len(), 6 * 64);
        }
    }

    #[test]
    fn flips_mirror_the_whole_composed_sprite() {
        let tiles = asymmetric_tiles();
        let (x, y) = (40, 50);
        for (first, columns, rows) in [(18, 1, 1), (18, 3, 2)] {
            let mut plain = filled(9);
            draw_block(&mut plain, &tiles, first, x, y, columns, rows, false, false);
            let (w, h) = (columns as i32 * 8, rows as i32 * 8);
            for (flip_x, flip_y) in FLIPS {
                let mut flipped = filled(9);
                draw_block(
                    &mut flipped,
                    &tiles,
                    first,
                    x,
                    y,
                    columns,
                    rows,
                    flip_x,
                    flip_y,
                );
                for dy in 0..h {
                    for dx in 0..w {
                        let sx = if flip_x { w - 1 - dx } else { dx };
                        let sy = if flip_y { h - 1 - dy } else { dy };
                        assert_eq!(
                            flipped.pixel(x + dx, y + dy),
                            plain.pixel(x + sx, y + sy),
                            "{columns}x{rows} flip ({flip_x}, {flip_y}) at ({dx}, {dy})"
                        );
                    }
                }
                // Holes keep the background, and nothing lands outside the block.
                let background =
                    |fb: &Framebuffer| fb.as_bytes().iter().filter(|&&i| i == 9).count();
                assert_eq!(
                    background(&flipped),
                    background(&plain),
                    "{columns}x{rows} flip ({flip_x}, {flip_y})"
                );
                if flip_x || flip_y {
                    assert_ne!(flipped, plain, "{columns}x{rows} flip ({flip_x}, {flip_y})");
                }
            }
        }
    }

    #[test]
    fn flipping_happens_before_clipping() {
        // Only the tile's left column is set.
        let tiles = bank_from(|tile, x, _| u8::from(tile == 0 && x == 0) * 7);
        let mut fb = Framebuffer::new();
        draw_block(&mut fb, &tiles, 0, -4, 0, 1, 1, false, false);
        assert_eq!(ink(&fb), []);

        draw_block(&mut fb, &tiles, 0, -4, 0, 1, 1, true, false);
        assert_eq!(ink(&fb), (0..8).map(|y| ((3, y), 7)).collect::<Vec<_>>());

        // In a 2-wide block, a flip moves the left tile to the right half.
        let mut fb = Framebuffer::new();
        draw_block(&mut fb, &tiles, 0, -8, 0, 2, 1, true, true);
        assert_eq!(ink(&fb), (0..8).map(|y| ((7, y), 7)).collect::<Vec<_>>());
    }

    #[test]
    fn flipped_sprites_handle_extreme_coordinates() {
        let tiles = solid_tiles(&[(0, 7)]);
        let mut fb = Framebuffer::new();
        for (flip_x, flip_y) in FLIPS {
            draw_block(
                &mut fb,
                &tiles,
                0,
                i32::MAX,
                i32::MAX,
                16,
                64,
                flip_x,
                flip_y,
            );
            draw_block(
                &mut fb,
                &tiles,
                0,
                i32::MIN,
                i32::MIN,
                16,
                64,
                flip_x,
                flip_y,
            );
        }
        assert_eq!(fb, Framebuffer::new());
    }

    /// Palettes where bank `n` maps every local index to master `100 + n`,
    /// so a pixel's value names the bank that drew it.
    fn bank_marking_palettes() -> ObjPalettes {
        let mut bytes = [0; OBJ_PALETTES_BYTES];
        for (offset, byte) in bytes.iter_mut().enumerate() {
            if offset % 16 != 0 {
                *byte = 100 + (offset / 16) as u8;
            }
        }
        ObjPalettes::from_bytes(&bytes).unwrap()
    }

    /// Solid tiles 0, 1, 16 and 17 (a 2x2 block at tile 0), local index 3,
    /// with default banks 2, 5, 0 and 7.
    fn mixed_bank_vram() -> ObjVram {
        let tiles = solid_tiles(&[(0, 3), (1, 3), (16, 3), (17, 3)]);
        let mut attributes = [0; 18];
        attributes[0] = 2;
        attributes[1] = 5;
        attributes[17] = 7;
        ObjVram::new(
            tiles,
            ObjTileAttributes::from_bytes(&attributes).unwrap(),
            bank_marking_palettes(),
        )
    }

    fn block_2x2(palette: ObjPaletteChoice, flip_x: bool) -> ObjSprite {
        ObjSprite {
            first: 0,
            x: 0,
            y: 0,
            width: 2,
            height: 2,
            flip_x,
            flip_y: false,
            palette,
        }
    }

    #[test]
    fn tile_default_palettes_resolve_per_tile() {
        let mut fb = Framebuffer::new();
        draw_obj_sprite(
            &mut fb,
            &mixed_bank_vram(),
            &block_2x2(ObjPaletteChoice::TileDefault, false),
        );
        assert_eq!(cell(&fb, 0, 0), Some(102));
        assert_eq!(cell(&fb, 8, 0), Some(105));
        assert_eq!(cell(&fb, 0, 8), Some(100));
        assert_eq!(cell(&fb, 8, 8), Some(107));
    }

    #[test]
    fn an_explicit_bank_overrides_every_tile_of_the_block() {
        let bank = ObjBank::new(4).unwrap();
        let mut fb = Framebuffer::new();
        draw_obj_sprite(
            &mut fb,
            &mixed_bank_vram(),
            &block_2x2(ObjPaletteChoice::Bank(bank), false),
        );
        for (x, y) in [(0, 0), (8, 0), (0, 8), (8, 8)] {
            assert_eq!(cell(&fb, x, y), Some(104), "cell ({x}, {y})");
        }
    }

    #[test]
    fn a_tile_keeps_its_default_bank_when_flipped_into_another_position() {
        let mut fb = Framebuffer::new();
        draw_obj_sprite(
            &mut fb,
            &mixed_bank_vram(),
            &block_2x2(ObjPaletteChoice::TileDefault, true),
        );
        assert_eq!(cell(&fb, 0, 0), Some(105));
        assert_eq!(cell(&fb, 8, 0), Some(102));
        assert_eq!(cell(&fb, 0, 8), Some(107));
        assert_eq!(cell(&fb, 8, 8), Some(100));
    }

    #[test]
    fn local_zero_stays_transparent_whatever_the_bank() {
        let tiles = bank_from(|_, x, _| if x < 4 { 0 } else { 9 });
        let vram = ObjVram::new(tiles, ObjTileAttributes::new(), bank_marking_palettes());
        for bank in 0..8 {
            let mut fb = filled(1);
            let sprite = ObjSprite {
                palette: ObjPaletteChoice::Bank(ObjBank::new(bank).unwrap()),
                ..ObjSprite::tile(0, 0, 0)
            };
            draw_obj_sprite(&mut fb, &vram, &sprite);
            assert_eq!(fb.pixel(3, 0), Some(1), "bank {bank}");
            assert_eq!(fb.pixel(4, 0), Some(100 + bank), "bank {bank}");
        }
    }

    #[test]
    fn remap_rewrites_every_pixel_through_the_table() {
        let mut table = [0; PALETTE_LEN];
        for (i, entry) in table.iter_mut().enumerate() {
            *entry = (i as u8).wrapping_add(10);
        }
        let mut fb = Framebuffer::new();
        pixel(&mut fb, 3, 4, 250);
        remap(&mut fb, &table);

        let mut expected = Framebuffer::new();
        clear(&mut expected, 10);
        pixel(&mut expected, 3, 4, 4);
        assert_eq!(fb, expected);
    }

    /// A blank framebuffer with the `width`×`height` region at `(x, y)` set
    /// to `INK`; every pixel of the region must be on screen.
    fn with_rect(x: i32, y: i32, width: i32, height: i32) -> Framebuffer {
        with_ink((y..y + height).flat_map(move |py| (x..x + width).map(move |px| (px, py))))
    }

    fn rect(x: i32, y: i32, width: i32, height: i32) -> Framebuffer {
        let mut fb = Framebuffer::new();
        fill_rect(&mut fb, x, y, width, height, INK);
        fb
    }

    #[test]
    fn fill_rect_covers_exact_region() {
        assert_eq!(rect(10, 20, 3, 2), with_rect(10, 20, 3, 2));
    }

    #[test]
    fn fill_rect_draws_a_single_pixel() {
        assert_eq!(rect(12, 34, 1, 1), with_ink([(12, 34)]));
    }

    #[test]
    fn fill_rect_clips_each_edge() {
        let (w, h) = (WIDTH as i32, HEIGHT as i32);
        assert_eq!(rect(-5, 10, 8, 4), with_rect(0, 10, 3, 4), "left");
        assert_eq!(rect(10, -5, 4, 8), with_rect(10, 0, 4, 3), "top");
        assert_eq!(rect(w - 3, 10, 8, 4), with_rect(w - 3, 10, 3, 4), "right");
        assert_eq!(rect(10, h - 3, 4, 8), with_rect(10, h - 3, 4, 3), "bottom");
        assert_eq!(rect(-1, -1, 2, 2), with_ink([(0, 0)]), "top-left corner");
        assert_eq!(
            rect(w - 1, h - 1, 5, 5),
            with_ink([(w - 1, h - 1)]),
            "bottom-right corner"
        );
    }

    #[test]
    fn fill_rect_covers_the_whole_screen() {
        let (w, h) = (WIDTH as i32, HEIGHT as i32);
        assert_eq!(rect(0, 0, w, h), filled(INK));
        assert_eq!(rect(-50, -50, w + 100, h + 100), filled(INK));
    }

    #[test]
    fn fill_rect_off_screen_draws_nothing() {
        let (w, h) = (WIDTH as i32, HEIGHT as i32);
        for (x, y) in [(-10, 10), (10, -10), (w, 10), (10, h), (-10, -10), (w, h)] {
            assert_eq!(rect(x, y, 10, 10), Framebuffer::new(), "rect at ({x}, {y})");
        }
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
        fill_rect(&mut fb, i32::MIN, i32::MIN, i32::MIN, i32::MIN, INK);
        assert_eq!(fb, Framebuffer::new());

        fill_rect(&mut fb, -10, -10, i32::MAX, i32::MAX, INK);
        assert_eq!(fb, filled(INK));
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
    fn glyphs_after_off_screen_ones_still_draw() {
        assert_eq!(render("##", -4, 0), with_ink(block(0, 0)));
        assert_eq!(render("#\n#", 0, -5), with_ink(block(0, 0)));
        assert_eq!(render("#", -3, 0), Framebuffer::new());
        assert_eq!(render("#", 0, -3), Framebuffer::new());
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

    #[test]
    fn scaled_glyph_enlarges_each_bit() {
        let mut fb = Framebuffer::new();
        draw_text_scaled(&mut fb, &tiny_font(), "x", 10, 20, 2, INK);
        // 'x' sets the four corners and the center of its 3×3 grid.
        let bits = [(0, 0), (2, 0), (1, 1), (0, 2), (2, 2)];
        let expected = with_ink(bits.into_iter().flat_map(|(bx, by)| {
            (0..2).flat_map(move |dy| (0..2).map(move |dx| (10 + bx * 2 + dx, 20 + by * 2 + dy)))
        }));
        assert_eq!(fb, expected);
    }

    #[test]
    fn scaled_text_advances_by_scaled_cells() {
        let mut fb = Framebuffer::new();
        draw_text_scaled(&mut fb, &tiny_font(), "#\n#", 0, 0, 3, INK);
        let cell =
            |x: i32, y: i32| (y..y + 9).flat_map(move |py| (x..x + 9).map(move |px| (px, py)));
        assert_eq!(fb, with_ink(cell(0, 0).chain(cell(0, 15))));

        let mut fb = Framebuffer::new();
        draw_text_scaled(&mut fb, &tiny_font(), "##", 0, 0, 3, INK);
        assert_eq!(fb, with_ink(cell(0, 0).chain(cell(12, 0))));
    }

    #[test]
    fn scale_one_matches_draw_text() {
        let mut fb = Framebuffer::new();
        draw_text_scaled(&mut fb, &tiny_font(), "x#\n#x", 10, 20, 1, INK);
        assert_eq!(fb, render("x#\n#x", 10, 20));
    }

    #[test]
    fn non_positive_scale_draws_nothing() {
        let mut fb = Framebuffer::new();
        draw_text_scaled(&mut fb, &tiny_font(), "##", 0, 0, 0, INK);
        draw_text_scaled(&mut fb, &tiny_font(), "##", 0, 0, -2, INK);
        assert_eq!(fb, Framebuffer::new());
    }

    #[test]
    fn scaled_extreme_positions_do_not_panic() {
        let mut fb = Framebuffer::new();
        draw_text_scaled(
            &mut fb,
            &tiny_font(),
            "##\n##",
            i32::MAX - 1,
            i32::MAX - 1,
            8,
            INK,
        );
        draw_text_scaled(&mut fb, &tiny_font(), "##\n##", i32::MIN, i32::MIN, 8, INK);
        assert_eq!(fb, Framebuffer::new());
    }

    #[test]
    fn ink_images_resolve_slots_and_skip_transparency() {
        let image = InkImage::new(3, 2, vec![0, 1, 2, 3, 0, 1]).unwrap();
        let mut fb = filled(9);
        draw_ink_image(&mut fb, &image, 10, 20, &[4, 5, 6]);
        let row = |y| {
            (10..13)
                .map(|x| fb.pixel(x, y).unwrap())
                .collect::<Vec<_>>()
        };
        assert_eq!(row(20), [9, 4, 5]);
        assert_eq!(row(21), [6, 9, 4]);
        assert_eq!(fb.pixel(13, 20), Some(9));
    }

    #[test]
    fn ink_images_clip_at_every_edge() {
        let image = InkImage::new(2, 2, vec![1; 4]).unwrap();
        let (w, h) = (WIDTH as i32, HEIGHT as i32);
        for (x, y, expected) in [
            (-1, -1, vec![(0, 0)]),
            (w - 1, h - 1, vec![(w - 1, h - 1)]),
            (-1, h - 1, vec![(0, h - 1)]),
            (w - 1, -1, vec![(w - 1, 0)]),
            (w, 0, vec![]),
            (i32::MIN, i32::MAX, vec![]),
        ] {
            let mut fb = Framebuffer::new();
            draw_ink_image(&mut fb, &image, x, y, &[INK, INK, INK]);
            assert_eq!(fb, with_ink(expected), "origin ({x}, {y})");
        }
    }
}
