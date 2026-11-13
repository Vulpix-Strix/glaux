//! The read-only `hardware` table. The values come from `glaux-machine`, so
//! these tests pin them to the approved memory map and check that scripts can
//! build the packed fields with them.

use glaux_machine::oam::OamEntry;
use glaux_machine::obj_palette::ObjBank;
use glaux_machine::tilemap::{BgBank, BgLayer, TileEntry};
use pretty_assertions::assert_eq;

use super::map::store;
use super::*;

/// Every constant as `name = value` with the value the approved spec gives.
const APPROVED: &[(&str, u32)] = &[
    ("addr.WORK_RAM", 0x00000),
    ("addr.FRAMEBUFFER", 0x10000),
    ("addr.BG_TILES", 0x24000),
    ("addr.OBJ_TILES", 0x2C000),
    ("addr.MASTER_PALETTE", 0x3A000),
    ("addr.BG_PALETTES", 0x3A300),
    ("addr.OBJ_PALETTES", 0x3A340),
    ("addr.OBJ_ATTRIBUTES", 0x3A400),
    ("addr.OAM", 0x3B000),
    ("addr.DISPLAY_ENABLE", 0x3C000),
    ("addr.BACKDROP_COLOR", 0x3C001),
    ("size.ADDRESS_SPACE", 0x100000),
    ("size.WORK_RAM", 65_536),
    ("size.FRAMEBUFFER", 76_800),
    ("size.BG_TILES", 32_768),
    ("size.OBJ_TILES", 32_768),
    ("size.TILEMAP", 8_192),
    ("size.MASTER_PALETTE", 768),
    ("size.BG_PALETTES", 64),
    ("size.OBJ_PALETTES", 128),
    ("size.OBJ_ATTRIBUTES", 1_024),
    ("size.OAM", 1_024),
    ("bg[0].TILEMAP", 0x34000),
    ("bg[1].TILEMAP", 0x36000),
    ("bg[2].TILEMAP", 0x38000),
    ("bg[0].SCROLL_X", 0x3C010),
    ("bg[0].SCROLL_Y", 0x3C012),
    ("bg[0].PRIORITY", 0x3C014),
    ("bg[1].SCROLL_X", 0x3C020),
    ("bg[1].SCROLL_Y", 0x3C022),
    ("bg[1].PRIORITY", 0x3C024),
    ("bg[2].SCROLL_X", 0x3C030),
    ("bg[2].SCROLL_Y", 0x3C032),
    ("bg[2].PRIORITY", 0x3C034),
    ("bg.LAYERS", 3),
    ("bg.MAP_SIDE", 64),
    ("bg.TILE_BYTES", 32),
    ("bg.TILE_CAPACITY", 1_024),
    ("bg.PALETTE_BANKS", 4),
    ("bg.PRIORITY_MASK", 0b11),
    ("display.WIDTH", 320),
    ("display.HEIGHT", 240),
    ("display.BG0", 1),
    ("display.BG1", 2),
    ("display.BG2", 4),
    ("display.OBJ", 8),
    ("display.MASK", 15),
    ("tile.ENTRY_BYTES", 2),
    ("tile.INDEX_MASK", 0x03FF),
    ("tile.FLIP_X", 0x0400),
    ("tile.FLIP_Y", 0x0800),
    ("tile.PRIORITY", 0x1000),
    ("tile.BANK_SHIFT", 13),
    ("tile.BANK_MASK", 0b11),
    ("oam.ENTRIES", 128),
    ("oam.ENTRY_BYTES", 8),
    ("oam.FIELD_X", 0),
    ("oam.FIELD_Y", 2),
    ("oam.FIELD_TILE", 4),
    ("oam.FIELD_ATTRIBUTES", 6),
    ("oam.FIELD_SIZE", 7),
    ("oam.TILE_MASK", 0x03FF),
    ("oam.ATTR_BANK_MASK", 0b111),
    ("oam.ATTR_FLIP_X", 0x08),
    ("oam.ATTR_FLIP_Y", 0x10),
    ("oam.ATTR_PRIORITY_SHIFT", 5),
    ("oam.ATTR_PRIORITY_MASK", 0b11),
    ("oam.SIZE_MASK", 0b111),
    ("oam.ENABLED", 0x08),
    ("oam.SIZE_8X8", 0),
    ("oam.SIZE_16X16", 1),
    ("oam.SIZE_32X32", 2),
    ("oam.SIZE_8X16", 3),
    ("oam.SIZE_16X8", 4),
    ("oam.SIZE_16X32", 5),
    ("oam.SIZE_32X16", 6),
    ("oam.SIZE_RESERVED", 7),
    ("obj.TILE_WIDTH", 8),
    ("obj.TILE_HEIGHT", 8),
    ("obj.TILE_BYTES", 32),
    ("obj.TILE_CAPACITY", 1_024),
    ("obj.GRID_COLUMNS", 16),
    ("obj.PALETTE_BANKS", 8),
    ("palette.COLORS", 256),
    ("palette.BANK_COLORS", 16),
];

fn with_maps(source: &str) -> Runtime {
    match Runtime::start_on_scratch_with_maps(
        "main.luau",
        source.as_bytes(),
        Files::default(),
        Budget::default(),
        font(),
        store(),
    ) {
        Ok(runtime) => runtime,
        Err(failure) => panic!("expected the script to start, got {failure:?}"),
    }
}

fn ticked(source: &str) -> Runtime {
    let mut runtime = with_maps(source);
    runtime.tick(NO_INPUT).unwrap();
    runtime
}

#[test]
fn every_constant_has_the_approved_value() {
    let lines: String = APPROVED
        .iter()
        .map(|(name, _)| format!("log.info(hardware.{name})\n"))
        .collect();
    let mut runtime = started(&format!("function tick()\n{lines}end"));
    runtime.tick(NO_INPUT).unwrap();
    let shown = texts(runtime.take_output());
    let expected: Vec<String> = APPROVED
        .iter()
        .map(|(_, value)| value.to_string())
        .collect();
    assert_eq!(shown, expected);
}

#[test]
fn nothing_in_the_table_can_be_changed() {
    let mut runtime = started(
        "function tick()
            local attempts = {
                function() hardware.addr.OAM = 1 end,
                function() hardware.addr.EXTRA = 1 end,
                function() hardware.bg[0].SCROLL_X = 1 end,
                function() hardware.bg[3] = {} end,
                function() hardware.bg.LAYERS = 1 end,
                function() hardware.oam = nil end,
                function() hardware.EXTRA = 1 end,
                function() rawset(hardware.size, 'OAM', 1) end,
                function() rawset(hardware.bg[2], 'TILEMAP', 1) end,
                function() hardware.display.BG0 = 0 end,
            }
            local changed = 0
            for _, attempt in attempts do
                if pcall(attempt) then changed += 1 end
            end
            log.info(changed, hardware.addr.OAM, hardware.bg[0].SCROLL_X, hardware.bg.LAYERS)
        end",
    );
    runtime.tick(NO_INPUT).unwrap();
    assert_eq!(texts(runtime.take_output()), ["0\t241664\t245776\t3"]);
}

#[test]
fn every_region_can_be_read_from_its_first_to_its_last_byte() {
    let mut runtime = started(
        "function tick()
            local regions = { 'WORK_RAM', 'FRAMEBUFFER', 'BG_TILES', 'OBJ_TILES',
                'MASTER_PALETTE', 'BG_PALETTES', 'OBJ_PALETTES', 'OBJ_ATTRIBUTES', 'OAM' }
            for _, name in regions do
                local first = hardware.addr[name]
                peek(first)
                peek(first + hardware.size[name] - 1)
            end
            for layer = 0, hardware.bg.LAYERS - 1 do
                peek(hardware.bg[layer].TILEMAP)
                peek(hardware.bg[layer].TILEMAP + hardware.size.TILEMAP - 1)
            end
            log.info(hardware.size.OAM // hardware.oam.ENTRY_BYTES == hardware.oam.ENTRIES)
            log.info(hardware.size.TILEMAP == hardware.bg.MAP_SIDE ^ 2 * hardware.tile.ENTRY_BYTES)
        end",
    );
    runtime.tick(NO_INPUT).unwrap();
    assert_eq!(texts(runtime.take_output()), ["true", "true"]);
}

#[test]
fn a_background_is_set_up_with_the_constants() {
    let mut runtime = with_maps(
        "function tick()
            map.load('town', 0)
            local cell = bit32.bor(
                7,
                hardware.tile.FLIP_X,
                bit32.lshift(2, hardware.tile.BANK_SHIFT))
            poke16(hardware.bg[0].TILEMAP + 2 * (3 * hardware.bg.MAP_SIDE + 5), cell)
            poke(hardware.addr.DISPLAY_ENABLE,
                bit32.bor(hardware.display.BG0, hardware.display.BG1, hardware.display.OBJ))
            poke16(hardware.bg[1].SCROLL_X, 40)
            poke(hardware.bg[1].PRIORITY, 1)
        end",
    );
    runtime.tick(NO_INPUT).unwrap();
    let hardware = runtime.scratch();
    let edited = TileEntry::new(7, true, false, false, BgBank::new(2).unwrap()).unwrap();
    let window = &hardware.tilemaps[0];
    assert_eq!(window.entry(5, 3), Some(edited));
    // The rest of the loaded map is still there: tile 3 * 6 + 4 of `town`.
    let loaded = TileEntry::new(3 * 6 + 4, false, false, false, BgBank::new(1).unwrap()).unwrap();
    assert_eq!(window.entry(4, 3), Some(loaded));
    assert_eq!(hardware.registers.display_enable(), 0b1011);
    assert_eq!(hardware.registers.bg_scroll_x(BgLayer::Bg1), 40);
    assert_eq!(hardware.registers.bg_priority(BgLayer::Bg1), 1);
}

#[test]
fn an_object_is_set_up_with_the_constants() {
    let runtime = ticked(
        "function tick()
            local e = hardware.addr.OAM + 3 * hardware.oam.ENTRY_BYTES
            poke16(e + hardware.oam.FIELD_X, 120)
            poke16(e + hardware.oam.FIELD_Y, 80)
            poke16(e + hardware.oam.FIELD_TILE, 20)
            poke(e + hardware.oam.FIELD_ATTRIBUTES,
                bit32.bor(1, hardware.oam.ATTR_FLIP_X,
                    bit32.lshift(2, hardware.oam.ATTR_PRIORITY_SHIFT)))
            poke(e + hardware.oam.FIELD_SIZE,
                bit32.bor(hardware.oam.SIZE_16X32, hardware.oam.ENABLED))
        end",
    );
    let object = runtime.scratch().oam.entry(3).unwrap();
    assert_eq!(
        object,
        OamEntry {
            x: 120,
            y: 80,
            tile: 20,
            bank: ObjBank::new(1).unwrap(),
            flip_x: true,
            flip_y: false,
            priority: 2,
            size: 5,
            enabled: true,
        }
    );
    assert_eq!(object.tiles(), Some((2, 4)));
}

#[test]
fn the_background_indices_are_the_ones_the_map_functions_take() {
    let mut runtime = with_maps(
        "function tick()
            for layer = 0, hardware.bg.LAYERS - 1 do
                map.load('town', layer)
                log.info(peek16(hardware.bg[layer].TILEMAP + hardware.tile.ENTRY_BYTES))
            end
        end",
    );
    runtime.tick(NO_INPUT).unwrap();
    let shown = texts(runtime.take_output());
    let hardware = runtime.scratch();
    let expected: Vec<String> = BgLayer::ALL
        .iter()
        .map(|layer| {
            let entry = hardware.tilemaps[layer.index()].entry(1, 0).unwrap();
            assert!(!entry.is_empty());
            entry.raw().to_string()
        })
        .collect();
    assert_eq!(shown, expected);
}

#[test]
fn the_constants_are_available_while_the_script_starts() {
    let runtime = with_maps(
        "poke(hardware.addr.BACKDROP_COLOR, 9)
        function boot() poke(hardware.bg[2].PRIORITY, 0) end",
    );
    assert_eq!(runtime.scratch().registers.backdrop_color(), 9);
    assert_eq!(runtime.scratch().registers.bg_priority(BgLayer::Bg2), 0);
}

#[test]
fn reading_the_constants_changes_nothing_in_the_hardware() {
    let mut runtime = started(
        "function tick()
            for _, group in { hardware.addr, hardware.size, hardware.oam, hardware.tile } do
                for _ in group do end
            end
        end",
    );
    let before = runtime.scratch().clone();
    runtime.tick(NO_INPUT).unwrap();
    assert_eq!(*runtime.scratch(), before);
}
