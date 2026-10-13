//! Drawing primitives over the logical framebuffer.
//!
//! All primitives clip to the framebuffer: anything that falls outside it is
//! silently dropped.

use glaux_machine::font::{BitmapFont, GlyphId};
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

/// Draws object tile `tile` through the bank `palette` chooses with its
/// top-left corner at `(x, y)`, each pixel enlarged to a `scale`×`scale`
/// block. Master index `m` is written as `slots[m]`, which lets the tile be
/// shown in a frame whose palette places the master colors elsewhere.
///
/// Tile-local index 0 is transparent. A non-positive `scale` or an invalid
/// tile draws nothing.
pub fn draw_obj_tile_scaled(
    framebuffer: &mut Framebuffer,
    vram: &ObjVram,
    tile: usize,
    palette: ObjPaletteChoice,
    (x, y): (i32, i32),
    scale: i32,
    slots: &[u8; PALETTE_LEN],
) {
    if scale <= 0 {
        return;
    }
    let bank = match palette {
        ObjPaletteChoice::TileDefault => vram.attributes().bank(tile),
        ObjPaletteChoice::Bank(bank) => bank,
    };
    for local_y in 0..OBJ_TILE_HEIGHT {
        for local_x in 0..OBJ_TILE_WIDTH {
            let Some(local) = vram.tiles().pixel(tile, local_x, local_y) else {
                return;
            };
            let Some(master) = vram.palettes().resolve(bank, local) else {
                continue;
            };
            // Clip in i64: a block far from the origin may not fit in i32.
            let px = i64::from(x) + local_x as i64 * i64::from(scale);
            let py = i64::from(y) + local_y as i64 * i64::from(scale);
            if let (Ok(px), Ok(py)) = (i32::try_from(px), i32::try_from(py)) {
                fill_rect(
                    framebuffer,
                    px,
                    py,
                    scale,
                    scale,
                    slots[usize::from(master)],
                );
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
/// - Each character advances the pen by the cells it takes in `font`
///   ([`BitmapFont::cells`]), so a double-width glyph takes two.
/// - `'\n'` returns the pen to `x` and moves it down by `cell_height`.
/// - Characters without a glyph of their own draw the font's fallback
///   glyph; other control characters, such as `'\r'` and `'\t'`, leave
///   their cell empty but still advance the pen.
/// - No substitutions apply: every character draws its own glyph.
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
        if let Some(glyph) = font.resolve(ch) {
            blit_glyph(framebuffer, font, glyph, pen_x, pen_y, scale, color);
        }
        pen_x += font.cells(ch) as i64 * i64::from(metrics.cell_width()) * scale64;
    }
}

/// Draws `glyph` of `font` with the top-left corner of its first cell at
/// `(x, y)`, writing only its set bits. Parts outside the framebuffer are
/// clipped, and glyphs `font` doesn't have draw nothing.
pub fn draw_glyph(
    framebuffer: &mut Framebuffer,
    font: &BitmapFont,
    glyph: GlyphId,
    x: i32,
    y: i32,
    color: u8,
) {
    blit_glyph(
        framebuffer,
        font,
        glyph,
        i64::from(x),
        i64::from(y),
        1,
        color,
    );
}

fn blit_glyph(
    framebuffer: &mut Framebuffer,
    font: &BitmapFont,
    glyph: GlyphId,
    pen_x: i64,
    pen_y: i64,
    scale: i32,
    color: u8,
) {
    let Some(glyph) = font.glyph(glyph) else {
        return;
    };
    let (width, height) = (glyph.width(), font.metrics().cell_height());
    let scale64 = i64::from(scale);
    if !glyph_visible(pen_x, pen_y, width, height, scale64) {
        return;
    }
    for gy in 0..height {
        for gx in 0..width {
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
mod tests;
