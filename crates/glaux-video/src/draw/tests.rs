use super::*;
use glaux_machine::font::FontMetrics;
use glaux_machine::obj_palette::{OBJ_PALETTES_BYTES, ObjPalettes};
use glaux_machine::sprite::{OBJ_TILE_BYTES, OBJ_TILE_CAPACITY, ObjTileAttributes, ObjTileBank};
use pretty_assertions::assert_eq;
use proptest::prelude::*;

/// `None` when both frames hold the same pixels, and otherwise how many pixels
/// differ and the first few as `(x, y): actual != expected`.
///
/// `assert_eq!` on framebuffers formats 76 800 bytes per side and
/// `pretty_assertions` then diffs them, which can exhaust memory when many
/// tests fail at once.
fn differences(actual: &Framebuffer, expected: &Framebuffer) -> Option<String> {
    use std::fmt::Write;
    let mut count = 0;
    let mut first = String::new();
    for (i, (a, e)) in actual
        .as_bytes()
        .iter()
        .zip(expected.as_bytes())
        .enumerate()
    {
        if a != e {
            if count < 5 {
                let _ = write!(first, " ({}, {}): {a} != {e};", i % WIDTH, i / WIDTH);
            }
            count += 1;
        }
    }
    (count > 0).then(|| format!("{count} pixels differ (actual != expected), first:{first}"))
}

/// Asserts that two framebuffers hold the same pixels with a bounded
/// failure report. An optional format string and arguments describe the
/// case.
macro_rules! assert_frame_eq {
    ($actual:expr, $expected:expr $(,)?) => {
        if let Some(diff) = differences(&$actual, &$expected) {
            panic!("frames differ: {diff}");
        }
    };
    ($actual:expr, $expected:expr, $($context:tt)+) => {
        if let Some(diff) = differences(&$actual, &$expected) {
            panic!("frames differ ({}): {diff}", format_args!($($context)+));
        }
    };
}

/// A framebuffer whose pixel `(x, y)` is `pixel(x, y)`.
fn frame_from(pixel: impl Fn(i32, i32) -> u8) -> Framebuffer {
    let mut fb = Framebuffer::new();
    for y in 0..HEIGHT as i32 {
        for x in 0..WIDTH as i32 {
            fb.set_pixel(x, y, pixel(x, y));
        }
    }
    fb
}

const INK: u8 = 7;

/// 3×3 glyphs in the top-left of 4×5 cells, and a `'='` two cells wide
/// whose 7×1 bar leaves the last column blank. `'?'` is deliberately
/// missing and the font has no fallback.
fn tiny_font() -> BitmapFont {
    let mut font = BitmapFont::new(FontMetrics::new(4, 5).unwrap());
    font.insert('x', 1, &[0b1010, 0b0100, 0b1010, 0, 0])
        .unwrap();
    font.insert('#', 1, &[0b1110, 0b1110, 0b1110, 0, 0])
        .unwrap();
    font.insert('=', 2, &[0b1111_1110, 0, 0, 0, 0]).unwrap();
    font
}

/// [`tiny_font`] drawing a single pixel at the top-left of the cell for
/// characters it lacks.
fn tiny_font_with_fallback() -> BitmapFont {
    let mut font = tiny_font();
    let dot = font.add_glyph(1, &[0b1000, 0, 0, 0, 0]).unwrap();
    font.set_fallback(dot).unwrap();
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
    assert_frame_eq!(fb, with_ink([(12, 34)]));
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
    assert_frame_eq!(fb, Framebuffer::new());
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
    assert_frame_eq!(fb, Framebuffer::new());

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
        let mut single = Framebuffer::new();
        single.set_pixel(expected.0, expected.1, 5);
        assert_frame_eq!(fb, single, "flip ({flip_x}, {flip_y})");
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
            let background = |fb: &Framebuffer| fb.as_bytes().iter().filter(|&&i| i == 9).count();
            assert_eq!(
                background(&flipped),
                background(&plain),
                "{columns}x{rows} flip ({flip_x}, {flip_y})"
            );
            if flip_x || flip_y {
                assert!(
                    flipped != plain,
                    "{columns}x{rows} flip ({flip_x}, {flip_y}) changed nothing"
                );
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
    assert_frame_eq!(fb, Framebuffer::new());

    draw_block(&mut fb, &tiles, 0, -4, 0, 1, 1, true, false);
    assert_frame_eq!(fb, with_ink((0..8).map(|y| (3, y))));

    // In a 2-wide block, a flip moves the left tile to the right half.
    let mut fb = Framebuffer::new();
    draw_block(&mut fb, &tiles, 0, -8, 0, 2, 1, true, true);
    assert_frame_eq!(fb, with_ink((0..8).map(|y| (7, y))));
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
    assert_frame_eq!(fb, Framebuffer::new());
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

/// Maps master `m` to `255 - m`, so tests can tell slots from masters.
fn reversed_slots() -> [u8; PALETTE_LEN] {
    std::array::from_fn(|m| 255 - m as u8)
}

#[test]
fn scaled_tiles_draw_square_blocks_through_slots_and_default_bank() {
    // Tile 1 has default bank 5: local 3 resolves to master 105.
    let vram = mixed_bank_vram();
    let mut fb = filled(9);
    draw_obj_tile_scaled(
        &mut fb,
        &vram,
        1,
        ObjPaletteChoice::TileDefault,
        (10, 20),
        3,
        &reversed_slots(),
    );

    let expected = 255 - 105;
    for y in 0..240 {
        for x in 0..320 {
            let inside = (10..34).contains(&x) && (20..44).contains(&y);
            let want = if inside { expected } else { 9 };
            assert_eq!(fb.pixel(x, y), Some(want), "({x}, {y})");
        }
    }
}

#[test]
fn scaled_tiles_can_draw_through_any_bank() {
    // Tile 1 defaults to bank 5; bank 2 resolves local 3 to master 102.
    let vram = mixed_bank_vram();
    let bank = ObjBank::new(2).unwrap();
    let mut fb = filled(9);
    draw_obj_tile_scaled(
        &mut fb,
        &vram,
        1,
        ObjPaletteChoice::Bank(bank),
        (0, 0),
        1,
        &reversed_slots(),
    );
    assert_eq!(fb.pixel(0, 0), Some(255 - 102));
    assert_eq!(fb.pixel(7, 7), Some(255 - 102));
    assert_eq!(fb.pixel(8, 8), Some(9));
    assert_eq!(
        vram.attributes().bank(1).get(),
        5,
        "the tile keeps its bank"
    );
}

#[test]
fn scaled_tiles_keep_local_zero_transparent_and_place_each_block() {
    // Row pattern 0,1,2,3,4,0,5,6 on every row, through OBP0 linear.
    let row = [0x01, 0x23, 0x40, 0x56];
    let bytes = row
        .into_iter()
        .cycle()
        .take(OBJ_TILE_BYTES)
        .collect::<Vec<_>>();
    let vram = linear_vram(&ObjTileBank::from_bytes(&bytes).unwrap());
    let identity: [u8; PALETTE_LEN] = std::array::from_fn(|m| m as u8);
    let mut fb = filled(200);
    draw_obj_tile_scaled(
        &mut fb,
        &vram,
        0,
        ObjPaletteChoice::TileDefault,
        (0, 0),
        2,
        &identity,
    );

    let expected_row = [200, 1, 2, 3, 4, 200, 5, 6];
    for (column, expected) in expected_row.into_iter().enumerate() {
        let x = column as i32 * 2;
        for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
            assert_eq!(fb.pixel(x + dx, 14 + dy), Some(expected), "column {column}");
        }
    }
    assert_eq!(fb.pixel(16, 0), Some(200));
    assert_eq!(fb.pixel(0, 16), Some(200));
}

#[test]
fn scaled_tiles_ignore_bad_scales_and_tiles_and_clip() {
    let vram = linear_vram(&solid_tiles(&[(0, 4)]));
    let identity: [u8; PALETTE_LEN] = std::array::from_fn(|m| m as u8);
    for (tile, scale) in [(0, 0), (0, -2), (OBJ_TILE_CAPACITY, 2)] {
        let mut fb = Framebuffer::new();
        draw_obj_tile_scaled(
            &mut fb,
            &vram,
            tile,
            ObjPaletteChoice::TileDefault,
            (0, 0),
            scale,
            &identity,
        );
        assert_frame_eq!(fb, Framebuffer::new(), "tile {tile} scale {scale}");
    }
    let mut fb = Framebuffer::new();
    draw_obj_tile_scaled(
        &mut fb,
        &vram,
        0,
        ObjPaletteChoice::TileDefault,
        (i32::MAX - 4, -10),
        12,
        &identity,
    );
    draw_obj_tile_scaled(
        &mut fb,
        &vram,
        0,
        ObjPaletteChoice::TileDefault,
        (316, 236),
        12,
        &identity,
    );
    assert_eq!(fb.as_bytes().iter().filter(|&&p| p == 4).count(), 16);
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
    assert_frame_eq!(fb, expected);
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
    assert_frame_eq!(rect(10, 20, 3, 2), with_rect(10, 20, 3, 2));
}

#[test]
fn fill_rect_clips_each_edge() {
    let (w, h) = (WIDTH as i32, HEIGHT as i32);
    assert_frame_eq!(rect(-5, 10, 8, 4), with_rect(0, 10, 3, 4), "left");
    assert_frame_eq!(rect(10, -5, 4, 8), with_rect(10, 0, 4, 3), "top");
    assert_frame_eq!(rect(w - 3, 10, 8, 4), with_rect(w - 3, 10, 3, 4), "right");
    assert_frame_eq!(rect(10, h - 3, 4, 8), with_rect(10, h - 3, 4, 3), "bottom");
    assert_frame_eq!(rect(-1, -1, 2, 2), with_ink([(0, 0)]), "top-left corner");
    assert_frame_eq!(
        rect(w - 1, h - 1, 5, 5),
        with_ink([(w - 1, h - 1)]),
        "bottom-right corner"
    );
}

#[test]
fn fill_rect_covers_the_whole_screen() {
    let (w, h) = (WIDTH as i32, HEIGHT as i32);
    assert_frame_eq!(rect(0, 0, w, h), filled(INK));
    assert_frame_eq!(rect(-50, -50, w + 100, h + 100), filled(INK));
}

#[test]
fn fill_rect_off_screen_draws_nothing() {
    let (w, h) = (WIDTH as i32, HEIGHT as i32);
    for (x, y) in [(-10, 10), (10, -10), (w, 10), (10, h), (-10, -10), (w, h)] {
        assert_frame_eq!(rect(x, y, 10, 10), Framebuffer::new(), "rect at ({x}, {y})");
    }
}

#[test]
fn fill_rect_with_empty_size_draws_nothing() {
    let mut fb = Framebuffer::new();
    for (w, h) in [(0, 5), (5, 0), (-3, 5), (5, -3)] {
        fill_rect(&mut fb, 10, 10, w, h, INK);
    }
    assert_frame_eq!(fb, Framebuffer::new());
}

#[test]
fn fill_rect_extreme_values_do_not_panic() {
    let mut fb = Framebuffer::new();
    fill_rect(&mut fb, i32::MAX, i32::MAX, i32::MAX, i32::MAX, INK);
    fill_rect(&mut fb, i32::MIN, i32::MIN, i32::MAX, i32::MAX, INK);
    fill_rect(&mut fb, i32::MIN, i32::MIN, i32::MIN, i32::MIN, INK);
    assert_frame_eq!(fb, Framebuffer::new());

    fill_rect(&mut fb, -10, -10, i32::MAX, i32::MAX, INK);
    assert_frame_eq!(fb, filled(INK));
}

#[test]
fn glyph_produces_exact_pixels() {
    let expected = with_ink([(10, 20), (12, 20), (11, 21), (10, 22), (12, 22)]);
    assert_frame_eq!(render("x", 10, 20), expected);
}

#[test]
fn characters_advance_by_cell_width() {
    let fb = render("##", 10, 20);
    assert_frame_eq!(fb, with_ink(block(10, 20).chain(block(14, 20))));
    assert!((20..23).all(|y| fb.pixel(13, y) == Some(0)));
}

#[test]
fn newline_moves_down_by_cell_height() {
    let fb = render("#\n#", 10, 20);
    assert_frame_eq!(fb, with_ink(block(10, 20).chain(block(10, 25))));
    assert!((10..13).all(|x| fb.pixel(x, 23) == Some(0) && fb.pixel(x, 24) == Some(0)));
}

#[test]
fn missing_glyph_leaves_cell_empty_but_advances() {
    let expected = with_ink(block(10, 20).chain(block(18, 20)));
    assert_frame_eq!(render("#?#", 10, 20), expected);
    assert_frame_eq!(render("#\r#", 10, 20), expected);
    assert_frame_eq!(render("#é#", 10, 20), expected);
}

#[test]
fn missing_glyphs_draw_the_fallback_but_control_characters_stay_empty() {
    let mut fb = Framebuffer::new();
    draw_text(&mut fb, &tiny_font_with_fallback(), "?\té𝔸#", 10, 20, INK);
    let fallbacks = [(10, 20), (18, 20), (22, 20)];
    assert_frame_eq!(fb, with_ink(fallbacks.into_iter().chain(block(26, 20))));
}

#[test]
fn double_width_glyphs_span_and_advance_two_cells() {
    let bar = (10..17).map(|x| (x, 20));
    assert_frame_eq!(render("=#", 10, 20), with_ink(bar.chain(block(18, 20))));
}

#[test]
fn single_glyphs_draw_by_id_at_any_position() {
    let font = tiny_font();
    let mut fb = Framebuffer::new();
    draw_glyph(&mut fb, &font, font.glyph_id('#').unwrap(), -1, 238, INK);
    draw_glyph(&mut fb, &font, font.glyph_id('=').unwrap(), 315, 0, INK);
    let expected = [(0, 238), (1, 238), (0, 239), (1, 239)]
        .into_iter()
        .chain((315..320).map(|x| (x, 0)));
    assert_frame_eq!(fb, with_ink(expected));
}

#[test]
fn clips_left_and_top() {
    assert_frame_eq!(render("#", -2, -2), with_ink([(0, 0)]));
}

#[test]
fn glyphs_after_off_screen_ones_still_draw() {
    assert_frame_eq!(render("##", -4, 0), with_ink(block(0, 0)));
    assert_frame_eq!(render("#\n#", 0, -5), with_ink(block(0, 0)));
    assert_frame_eq!(render("#", -3, 0), Framebuffer::new());
    assert_frame_eq!(render("#", 0, -3), Framebuffer::new());
}

#[test]
fn clips_right_and_bottom() {
    let expected = with_ink([(318, 238), (319, 238), (318, 239), (319, 239)]);
    assert_frame_eq!(render("#", 318, 238), expected);
}

#[test]
fn extreme_positions_do_not_panic() {
    assert_frame_eq!(
        render("##\n##", i32::MAX - 1, i32::MAX - 1),
        Framebuffer::new()
    );
    assert_frame_eq!(render("##\n##", i32::MIN, i32::MIN), Framebuffer::new());
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
    assert_frame_eq!(fb, expected);
}

#[test]
fn uses_font_metrics() {
    // 2×2 glyphs in 3×2 cells: no vertical spacing row.
    let mut font = BitmapFont::new(FontMetrics::new(3, 2).unwrap());
    font.insert('o', 1, &[0b110, 0b110]).unwrap();

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
    assert_frame_eq!(fb, expected);
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
    assert_frame_eq!(fb, expected);
}

#[test]
fn scaled_text_advances_by_scaled_cells() {
    let mut fb = Framebuffer::new();
    draw_text_scaled(&mut fb, &tiny_font(), "#\n#", 0, 0, 3, INK);
    let cell = |x: i32, y: i32| (y..y + 9).flat_map(move |py| (x..x + 9).map(move |px| (px, py)));
    assert_frame_eq!(fb, with_ink(cell(0, 0).chain(cell(0, 15))));

    let mut fb = Framebuffer::new();
    draw_text_scaled(&mut fb, &tiny_font(), "##", 0, 0, 3, INK);
    assert_frame_eq!(fb, with_ink(cell(0, 0).chain(cell(12, 0))));
}

#[test]
fn non_positive_scale_draws_nothing() {
    let mut fb = Framebuffer::new();
    draw_text_scaled(&mut fb, &tiny_font(), "##", 0, 0, 0, INK);
    draw_text_scaled(&mut fb, &tiny_font(), "##", 0, 0, -2, INK);
    assert_frame_eq!(fb, Framebuffer::new());
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
    assert_frame_eq!(fb, Framebuffer::new());
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
        assert_frame_eq!(fb, with_ink(expected), "origin ({x}, {y})");
    }
}

/// Whether `p` lies in `start..start + len`, computed without overflow.
fn spans(start: i32, len: i64, p: i32) -> bool {
    (i64::from(start)..i64::from(start) + len).contains(&i64::from(p))
}

/// What `sprite` shows at screen pixel `(x, y)`, worked out from the tile
/// data instead of by drawing: the block pixel there, read from the other
/// side of each flipped axis. `None` where the block does not reach or the
/// pixel is transparent. Assumes [`linear_vram`], under which local indices
/// are the drawn indices.
fn sprite_pixel(tiles: &ObjTileBank, sprite: &ObjSprite, x: i32, y: i32) -> Option<u8> {
    let (width, height) = (sprite.width as i64 * 8, sprite.height as i64 * 8);
    if !spans(sprite.x, width, x) || !spans(sprite.y, height, y) {
        return None;
    }
    let dx = i64::from(x) - i64::from(sprite.x);
    let dy = i64::from(y) - i64::from(sprite.y);
    let bx = (if sprite.flip_x { width - 1 - dx } else { dx }) as usize;
    let by = (if sprite.flip_y { height - 1 - dy } else { dy }) as usize;
    let tile = obj_grid_tile(sprite.first, bx / 8, by / 8)?;
    tiles
        .pixel(tile, bx % 8, by % 8)
        .filter(|&local| local != 0)
}

/// Coordinates around the screen, plus the full `i32` range.
fn screen_coordinate() -> impl Strategy<Value = i32> {
    prop_oneof![-400..720, any::<i32>()]
}

/// Sizes around the screen's, including non-positive ones, plus the full
/// `i32` range.
fn rect_size() -> impl Strategy<Value = i32> {
    prop_oneof![-3..400, any::<i32>()]
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(128))]

    #[test]
    fn fill_rect_covers_exactly_the_on_screen_part_of_the_rectangle(
        x in screen_coordinate(),
        y in screen_coordinate(),
        width in rect_size(),
        height in rect_size(),
    ) {
        let mut fb = filled(9);
        fill_rect(&mut fb, x, y, width, height, INK);
        let inside = |px, py| spans(x, width.into(), px) && spans(y, height.into(), py);
        let expected = frame_from(|px, py| if inside(px, py) { INK } else { 9 });
        prop_assert_eq!(differences(&fb, &expected), None);
    }

    #[test]
    fn sprites_show_their_block_mirrored_and_clipped_to_the_screen(
        seed: u64,
        first in 0..OBJ_TILE_CAPACITY,
        width in 1usize..=4,
        height in 1usize..=4,
        x in -40i32..360,
        y in -40i32..280,
        flip_x: bool,
        flip_y: bool,
    ) {
        let tiles = bank_from(|tile, px, py| {
            let mut h = seed ^ ((tile * 64 + py * 8 + px) as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15);
            h ^= h >> 31;
            (h.wrapping_mul(0xbf58_476d_1ce4_e5b9) >> 60) as u8
        });
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
        let mut fb = filled(9);
        draw_obj_sprite(&mut fb, &linear_vram(&tiles), &sprite);
        let expected = frame_from(|px, py| sprite_pixel(&tiles, &sprite, px, py).unwrap_or(9));
        prop_assert_eq!(differences(&fb, &expected), None);
    }
}
