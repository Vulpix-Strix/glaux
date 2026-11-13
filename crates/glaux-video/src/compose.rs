//! Composition of the background layers and the objects onto the working
//! framebuffer.
//!
//! The compositor reads the tiles, tilemaps, palettes, object table and
//! graphics registers of a [`CartridgeHardware`] and writes the winning master
//! index of every pixel straight into its framebuffer. It allocates nothing.
//!
//! # Depth
//!
//! Every candidate pixel carries a one-byte depth and the smaller depth wins.
//! A background pixel's depth is `effective_priority * 8 + rank`, where the
//! effective priority is the layer priority reduced by one when the tile's
//! boost bit is set, saturating at zero. An object's depth is its priority
//! times 8 plus [`OBJECT_RANK`].
//!
//! Within one effective priority, ranks run from front to back:
//!
//! ```text
//!   FRONT (smaller depth wins)
//!     |
//!     v
//!   +----------------------+
//!   | 0  Boosted BG2       |
//!   | 1  Boosted BG1       |
//!   | 2  Boosted BG0       |
//!   +----------------------+
//!   | 3  Objects           |
//!   +----------------------+
//!   | 4  Normal BG2        |
//!   | 5  Normal BG1        |
//!   | 6  Normal BG0        |
//!   +----------------------+
//!     |
//!     v
//!   BACK
//! ```
//!
//! The diagram already orders every source, so comparing two depths settles
//! any tie. A candidate takes over a pixel only when its depth is strictly
//! smaller, which means that when two depths are equal the one considered
//! first keeps the pixel. Objects are considered in OAM order, so among objects
//! of the same priority the lowest index wins. An object never ties with a
//! background pixel, because their ranks differ.

use glaux_machine::bg::{BgPalettes, BgTileBank};
use glaux_machine::framebuffer::{HEIGHT, WIDTH};
use glaux_machine::hardware::CartridgeHardware;
use glaux_machine::oam::{OAM_ENTRIES, Oam};
use glaux_machine::obj_palette::ObjBank;
use glaux_machine::registers::{DISPLAY_BG0, DISPLAY_BG1, DISPLAY_BG2, DISPLAY_OBJ};
use glaux_machine::sprite::{ObjVram, obj_grid_tile};
use glaux_machine::tile_window::TileWindow;
use glaux_machine::tilemap::{BG_LAYERS, BgLayer, TILE_WINDOW_SIDE};

const TILE_SIDE: usize = 8;

/// Side of the wrapping map in pixels.
const MAP_SIDE: usize = TILE_WINDOW_SIDE * TILE_SIDE;
const MAP_MASK: usize = MAP_SIDE - 1;

/// Rank values per effective priority level.
const RANKS: u8 = 8;
const NORMAL_RANK_BASE: u8 = 4;

/// The rank that objects take between boosted and normal background tiles.
pub const OBJECT_RANK: u8 = 3;

/// Depth of a pixel no layer has claimed yet.
const BEHIND_ALL: u8 = u8::MAX;

/// Bits of `DISPLAY_ENABLE` that switch BG0, BG1 and BG2 on.
const BACKGROUND_BITS: u8 = DISPLAY_BG0 | DISPLAY_BG1 | DISPLAY_BG2;

/// The bit of `DISPLAY_ENABLE` that switches objects on.
const OBJECT_BIT: u8 = DISPLAY_OBJ;

/// The depth of a pixel of `layer` whose tile has the given `boost` bit,
/// where `priority` is the layer's priority register (0 in front, 3 behind).
pub fn background_depth(priority: u8, boost: bool, layer: BgLayer) -> u8 {
    let effective = priority.saturating_sub(u8::from(boost));
    // BG2 is the frontmost layer of a tie.
    let order = (BG_LAYERS - 1 - layer.index()) as u8;
    let rank = if boost {
        order
    } else {
        NORMAL_RANK_BASE + order
    };
    effective * RANKS + rank
}

/// The depth of a pixel of an object with the given priority register value
/// (0 in front, 3 behind).
pub fn object_depth(priority: u8) -> u8 {
    priority * RANKS + OBJECT_RANK
}

/// A background layer as the registers configure it.
struct Layer<'a> {
    which: BgLayer,
    window: &'a TileWindow,
    scroll_x: usize,
    scroll_y: usize,
    priority: u8,
}

/// An object that can show on screen, decoded once per frame from its OAM
/// entry.
#[derive(Clone, Copy, Default)]
struct Object {
    x: i32,
    y: i32,
    tile: usize,
    columns: usize,
    rows: usize,
    flip_x: bool,
    flip_y: bool,
    bank: ObjBank,
    depth: u8,
}

/// Fills `objects` with the enabled entries that have a valid size and reach
/// the screen, in OAM order, and returns how many there are.
fn visible_objects(oam: &Oam, objects: &mut [Object; OAM_ENTRIES]) -> usize {
    let mut count = 0;
    for index in 0..OAM_ENTRIES {
        let Some(entry) = oam.entry(index) else {
            continue;
        };
        let Some((columns, rows)) = entry.tiles() else {
            continue;
        };
        let (x, y) = (i32::from(entry.x), i32::from(entry.y));
        let reaches_x = x < WIDTH as i32 && x + (columns * TILE_SIDE) as i32 > 0;
        let reaches_y = y < HEIGHT as i32 && y + (rows * TILE_SIDE) as i32 > 0;
        if entry.enabled && reaches_x && reaches_y {
            objects[count] = Object {
                x,
                y,
                tile: usize::from(entry.tile),
                columns,
                rows,
                flip_x: entry.flip_x,
                flip_y: entry.flip_y,
                bank: entry.bank,
                depth: object_depth(entry.priority),
            };
            count += 1;
        }
    }
    count
}

/// Draws the enabled background layers and objects over whatever the
/// framebuffer holds.
///
/// The framebuffer is expected to hold the backdrop already. Pixels no layer
/// or object claims keep their value. With none of the four enable bits of
/// `DISPLAY_ENABLE` set the framebuffer is left untouched.
///
/// Scroll registers keep the value the script wrote and are reduced modulo
/// 512 here, so the maps wrap on both axes. Objects are clipped to the screen
/// and never wrap. Tile 0 of a map and tile-local index 0 are transparent. A
/// non-zero local index is opaque even when its palette entry is master
/// index 0.
pub fn compose_layers(hardware: &mut CartridgeHardware) {
    let enabled = hardware.registers.display_enable();
    if enabled & (BACKGROUND_BITS | OBJECT_BIT) == 0 {
        return;
    }
    let CartridgeHardware {
        framebuffer,
        bg,
        obj,
        tilemaps,
        oam,
        registers,
        ..
    } = hardware;

    let layers = BgLayer::ALL.map(|which| {
        (enabled & (1 << which.index()) != 0).then(|| Layer {
            which,
            window: &tilemaps[which.index()],
            scroll_x: usize::from(registers.bg_scroll_x(which)) & MAP_MASK,
            scroll_y: usize::from(registers.bg_scroll_y(which)),
            priority: registers.bg_priority(which),
        })
    });

    let mut object_buffer = [Object::default(); OAM_ENTRIES];
    let object_count = if enabled & OBJECT_BIT != 0 {
        visible_objects(oam, &mut object_buffer)
    } else {
        0
    };
    let objects = &object_buffer[..object_count];

    let mut depths = [BEHIND_ALL; WIDTH];
    let (rows, _) = framebuffer.as_bytes_mut().as_chunks_mut::<WIDTH>();
    for (y, row) in rows.iter_mut().enumerate() {
        depths.fill(BEHIND_ALL);
        for layer in layers.iter().flatten() {
            compose_row(layer, y, bg.tiles(), bg.palettes(), row, &mut depths);
        }
        compose_objects_row(objects, y, obj, row, &mut depths);
    }
}

/// Composes one framebuffer row of `layer` in tile-wide spans.
fn compose_row(
    layer: &Layer<'_>,
    y: usize,
    tiles: &BgTileBank,
    palettes: &BgPalettes,
    row: &mut [u8; WIDTH],
    depths: &mut [u8; WIDTH],
) {
    let map_y = (y + layer.scroll_y) & MAP_MASK;
    let tile_row = map_y / TILE_SIDE;
    let line = map_y % TILE_SIDE;

    let mut x = 0;
    let mut map_x = layer.scroll_x;
    while x < WIDTH {
        let offset = map_x % TILE_SIDE;
        let span = (TILE_SIDE - offset).min(WIDTH - x);
        if let Some(entry) = layer.window.entry(map_x / TILE_SIDE, tile_row)
            && entry.tile() != 0
        {
            let depth = background_depth(layer.priority, entry.priority(), layer.which);
            let source_y = if entry.flip_y() {
                TILE_SIDE - 1 - line
            } else {
                line
            };
            for step in 0..span {
                let column = x + step;
                if depth >= depths[column] {
                    continue;
                }
                let local_x = offset + step;
                let source_x = if entry.flip_x() {
                    TILE_SIDE - 1 - local_x
                } else {
                    local_x
                };
                let Some(local) = tiles.pixel(usize::from(entry.tile()), source_x, source_y) else {
                    continue;
                };
                if let Some(color) = palettes.resolve(entry.bank(), local) {
                    row[column] = color;
                    depths[column] = depth;
                }
            }
        }
        x += span;
        map_x = (map_x + span) & MAP_MASK;
    }
}

/// Composes one framebuffer row of `objects`, which are in OAM order.
///
/// The mirroring matches `draw::draw_obj_sprite`, which `spr()` uses: a flip
/// reverses the order of the tile columns or rows of the whole object and
/// also mirrors the pixels inside each tile. A tile that the sheet addressing
/// cannot reach is transparent.
fn compose_objects_row(
    objects: &[Object],
    y: usize,
    vram: &ObjVram,
    row: &mut [u8; WIDTH],
    depths: &mut [u8; WIDTH],
) {
    for object in objects {
        let height = (object.rows * TILE_SIDE) as i32;
        let local_y = y as i32 - object.y;
        if !(0..height).contains(&local_y) {
            continue;
        }
        let local_y = if object.flip_y {
            height - 1 - local_y
        } else {
            local_y
        } as usize;
        let tile_row = local_y / TILE_SIDE;
        let line = local_y % TILE_SIDE;

        for column in 0..object.columns {
            let left = object.x + (column * TILE_SIDE) as i32;
            if left >= WIDTH as i32 || left + TILE_SIDE as i32 <= 0 {
                continue;
            }
            let source_column = if object.flip_x {
                object.columns - 1 - column
            } else {
                column
            };
            let Some(tile) = obj_grid_tile(object.tile, source_column, tile_row) else {
                continue;
            };
            for step in 0..TILE_SIDE {
                let Ok(target) = usize::try_from(left + step as i32) else {
                    continue;
                };
                if target >= WIDTH {
                    break;
                }
                if object.depth >= depths[target] {
                    continue;
                }
                let source_x = if object.flip_x {
                    TILE_SIDE - 1 - step
                } else {
                    step
                };
                let Some(local) = vram.tiles().pixel(tile, source_x, line) else {
                    continue;
                };
                if let Some(color) = vram.palettes().resolve(object.bank, local) {
                    row[target] = color;
                    depths[target] = object.depth;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests;
