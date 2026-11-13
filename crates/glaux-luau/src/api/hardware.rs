//! The read-only `hardware` table: the addresses, sizes and bit layouts that
//! scripts need to use the memory functions without magic numbers.
//!
//! Every value comes from a constant in `glaux-machine`, so the table cannot
//! drift from the machine. It lives in the VM and not in memory, so nothing
//! on the bus can change it.

use glaux_machine::bg::{BG_PALETTE_COLORS, BG_TILE_BYTES};
use glaux_machine::framebuffer::{HEIGHT, WIDTH};
use glaux_machine::memory_map::{
    ADDRESS_SPACE_LEN, BACKDROP_COLOR, BG_PALETTES, BG_PRIORITY, BG_REGISTER_BLOCKS, BG_SCROLL_X,
    BG_SCROLL_Y, BG_TILES, BG0_TILEMAP, BG1_TILEMAP, BG2_TILEMAP, DISPLAY_ENABLE, FRAMEBUFFER,
    MASTER_PALETTE, OAM, OBJ_ATTRIBUTES, OBJ_PALETTES, OBJ_TILES, WORK_RAM,
};
use glaux_machine::oam::{self, OAM_ENTRIES, OAM_ENTRY_BYTES};
use glaux_machine::obj_palette::OBJ_PALETTE_BANKS;
use glaux_machine::palette::PALETTE_LEN;
use glaux_machine::registers::{
    BG_PRIORITY_MASK, DISPLAY_BG0, DISPLAY_BG1, DISPLAY_BG2, DISPLAY_ENABLE_MASK, DISPLAY_OBJ,
};
use glaux_machine::sprite::{
    OBJ_GRID_COLUMNS, OBJ_TILE_BYTES, OBJ_TILE_CAPACITY, OBJ_TILE_HEIGHT, OBJ_TILE_WIDTH,
};
use glaux_machine::tile_window::TILE_WINDOW_BYTES;
use glaux_machine::tilemap::{
    BG_LAYERS, BG_PALETTE_BANKS, BG_TILE_CAPACITY, ENTRY_BANK_MASK, ENTRY_BANK_SHIFT, ENTRY_FLIP_X,
    ENTRY_FLIP_Y, ENTRY_INDEX_MASK, ENTRY_PRIORITY, TILE_ENTRY_BYTES, TILE_WINDOW_SIDE,
};
use mlua::{Lua, Table};

/// A read-only table of integers.
fn group(lua: &Lua, members: &[(&str, i64)]) -> mlua::Result<Table> {
    let table = lua.create_table_with_capacity(0, members.len())?;
    for &(name, value) in members {
        table.raw_set(name, value)?;
    }
    table.set_readonly(true);
    Ok(table)
}

/// Installs the `hardware` table.
pub(crate) fn install(lua: &Lua) -> mlua::Result<()> {
    let hardware = lua.create_table()?;
    let addr = |region: glaux_machine::memory_map::Region| i64::from(region.start);
    let len = |region: glaux_machine::memory_map::Region| i64::from(region.len);

    hardware.raw_set(
        "addr",
        group(
            lua,
            &[
                ("WORK_RAM", addr(WORK_RAM)),
                ("FRAMEBUFFER", addr(FRAMEBUFFER)),
                ("BG_TILES", addr(BG_TILES)),
                ("OBJ_TILES", addr(OBJ_TILES)),
                ("MASTER_PALETTE", addr(MASTER_PALETTE)),
                ("BG_PALETTES", addr(BG_PALETTES)),
                ("OBJ_PALETTES", addr(OBJ_PALETTES)),
                ("OBJ_ATTRIBUTES", addr(OBJ_ATTRIBUTES)),
                ("OAM", addr(OAM)),
                ("DISPLAY_ENABLE", i64::from(DISPLAY_ENABLE)),
                ("BACKDROP_COLOR", i64::from(BACKDROP_COLOR)),
            ],
        )?,
    )?;

    hardware.raw_set(
        "size",
        group(
            lua,
            &[
                ("ADDRESS_SPACE", i64::from(ADDRESS_SPACE_LEN)),
                ("WORK_RAM", len(WORK_RAM)),
                ("FRAMEBUFFER", len(FRAMEBUFFER)),
                ("BG_TILES", len(BG_TILES)),
                ("OBJ_TILES", len(OBJ_TILES)),
                ("TILEMAP", TILE_WINDOW_BYTES as i64),
                ("MASTER_PALETTE", len(MASTER_PALETTE)),
                ("BG_PALETTES", len(BG_PALETTES)),
                ("OBJ_PALETTES", len(OBJ_PALETTES)),
                ("OBJ_ATTRIBUTES", len(OBJ_ATTRIBUTES)),
                ("OAM", len(OAM)),
            ],
        )?,
    )?;

    let bg = group(
        lua,
        &[
            ("LAYERS", BG_LAYERS as i64),
            ("MAP_SIDE", TILE_WINDOW_SIDE as i64),
            ("TILE_BYTES", BG_TILE_BYTES as i64),
            ("TILE_CAPACITY", BG_TILE_CAPACITY as i64),
            ("PALETTE_BANKS", BG_PALETTE_BANKS as i64),
            ("PRIORITY_MASK", i64::from(BG_PRIORITY_MASK)),
        ],
    )?;
    bg.set_readonly(false);
    let tilemaps = [BG0_TILEMAP, BG1_TILEMAP, BG2_TILEMAP];
    for (layer, tilemap) in tilemaps.into_iter().enumerate() {
        let block = i64::from(BG_REGISTER_BLOCKS[layer]);
        let registers = group(
            lua,
            &[
                ("TILEMAP", addr(tilemap)),
                ("SCROLL_X", block + i64::from(BG_SCROLL_X)),
                ("SCROLL_Y", block + i64::from(BG_SCROLL_Y)),
                ("PRIORITY", block + i64::from(BG_PRIORITY)),
            ],
        )?;
        bg.raw_set(layer as i64, registers)?;
    }
    bg.set_readonly(true);
    hardware.raw_set("bg", bg)?;

    hardware.raw_set(
        "display",
        group(
            lua,
            &[
                ("WIDTH", WIDTH as i64),
                ("HEIGHT", HEIGHT as i64),
                ("BG0", i64::from(DISPLAY_BG0)),
                ("BG1", i64::from(DISPLAY_BG1)),
                ("BG2", i64::from(DISPLAY_BG2)),
                ("OBJ", i64::from(DISPLAY_OBJ)),
                ("MASK", i64::from(DISPLAY_ENABLE_MASK)),
            ],
        )?,
    )?;

    hardware.raw_set(
        "tile",
        group(
            lua,
            &[
                ("ENTRY_BYTES", TILE_ENTRY_BYTES as i64),
                ("INDEX_MASK", i64::from(ENTRY_INDEX_MASK)),
                ("FLIP_X", i64::from(ENTRY_FLIP_X)),
                ("FLIP_Y", i64::from(ENTRY_FLIP_Y)),
                ("PRIORITY", i64::from(ENTRY_PRIORITY)),
                ("BANK_SHIFT", i64::from(ENTRY_BANK_SHIFT)),
                ("BANK_MASK", i64::from(ENTRY_BANK_MASK)),
            ],
        )?,
    )?;

    hardware.raw_set(
        "oam",
        group(
            lua,
            &[
                ("ENTRIES", OAM_ENTRIES as i64),
                ("ENTRY_BYTES", OAM_ENTRY_BYTES as i64),
                ("FIELD_X", oam::FIELD_X as i64),
                ("FIELD_Y", oam::FIELD_Y as i64),
                ("FIELD_TILE", oam::FIELD_TILE as i64),
                ("FIELD_ATTRIBUTES", oam::FIELD_ATTRIBUTES as i64),
                ("FIELD_SIZE", oam::FIELD_SIZE as i64),
                ("TILE_MASK", i64::from(oam::TILE_MASK)),
                ("ATTR_BANK_MASK", i64::from(oam::ATTR_BANK_MASK)),
                ("ATTR_FLIP_X", i64::from(oam::ATTR_FLIP_X)),
                ("ATTR_FLIP_Y", i64::from(oam::ATTR_FLIP_Y)),
                ("ATTR_PRIORITY_SHIFT", i64::from(oam::ATTR_PRIORITY_SHIFT)),
                ("ATTR_PRIORITY_MASK", i64::from(oam::ATTR_PRIORITY_MASK)),
                ("SIZE_MASK", i64::from(oam::SIZE_MASK)),
                ("ENABLED", i64::from(oam::ENABLED)),
                ("SIZE_8X8", i64::from(oam::SIZE_8X8)),
                ("SIZE_16X16", i64::from(oam::SIZE_16X16)),
                ("SIZE_32X32", i64::from(oam::SIZE_32X32)),
                ("SIZE_8X16", i64::from(oam::SIZE_8X16)),
                ("SIZE_16X8", i64::from(oam::SIZE_16X8)),
                ("SIZE_16X32", i64::from(oam::SIZE_16X32)),
                ("SIZE_32X16", i64::from(oam::SIZE_32X16)),
                ("SIZE_RESERVED", i64::from(oam::SIZE_RESERVED)),
            ],
        )?,
    )?;

    hardware.raw_set(
        "obj",
        group(
            lua,
            &[
                ("TILE_WIDTH", OBJ_TILE_WIDTH as i64),
                ("TILE_HEIGHT", OBJ_TILE_HEIGHT as i64),
                ("TILE_BYTES", OBJ_TILE_BYTES as i64),
                ("TILE_CAPACITY", OBJ_TILE_CAPACITY as i64),
                ("GRID_COLUMNS", OBJ_GRID_COLUMNS as i64),
                ("PALETTE_BANKS", OBJ_PALETTE_BANKS as i64),
            ],
        )?,
    )?;

    hardware.raw_set(
        "palette",
        group(
            lua,
            &[
                ("COLORS", PALETTE_LEN as i64),
                ("BANK_COLORS", BG_PALETTE_COLORS as i64),
            ],
        )?,
    )?;

    hardware.set_readonly(true);
    lua.globals().raw_set("hardware", hardware)
}
