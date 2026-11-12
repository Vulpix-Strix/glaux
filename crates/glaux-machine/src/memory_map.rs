//! The G1 address map for the regions the machine implements.
//!
//! Addresses are 20 bits wide. Any address that no region covers is
//! unmapped.

use crate::bg::{BG_PALETTES_BYTES, BG_TILE_BANK_BYTES};
use crate::framebuffer;
use crate::oam::OAM_BYTES;
use crate::obj_palette::OBJ_PALETTES_BYTES;
use crate::palette::PALETTE_RGB888_BYTES;
use crate::sprite::{OBJ_TILE_ATTRIBUTES_BYTES, OBJ_TILE_BANK_BYTES};
use crate::tile_window::TILE_WINDOW_BYTES;
use crate::tilemap::BG_LAYERS;

/// Number of addresses: `0x00000` to `0xFFFFF`.
pub const ADDRESS_SPACE_LEN: u32 = 0x10_0000;

/// Size of Work RAM in bytes.
pub const WORK_RAM_LEN: usize = 64 * 1024;

/// A contiguous run of mapped addresses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Region {
    pub start: u32,
    pub len: u32,
}

impl Region {
    const fn new(start: u32, len: usize) -> Self {
        assert!(len <= ADDRESS_SPACE_LEN as usize);
        Self {
            start,
            len: len as u32,
        }
    }

    /// The first address after the region.
    pub const fn end(self) -> u32 {
        self.start + self.len
    }

    /// The last address of the region.
    pub const fn last(self) -> u32 {
        self.end() - 1
    }

    pub const fn contains(self, address: u32) -> bool {
        self.start <= address && address < self.end()
    }
}

pub const WORK_RAM: Region = Region::new(0x00000, WORK_RAM_LEN);
/// The cartridge's work framebuffer, one palette index per pixel.
pub const FRAMEBUFFER: Region = Region::new(0x10000, framebuffer::LEN);
pub const BG_TILES: Region = Region::new(0x24000, BG_TILE_BANK_BYTES);
pub const OBJ_TILES: Region = Region::new(0x2C000, OBJ_TILE_BANK_BYTES);
pub const BG0_TILEMAP: Region = Region::new(0x34000, TILE_WINDOW_BYTES);
pub const BG1_TILEMAP: Region = Region::new(0x36000, TILE_WINDOW_BYTES);
pub const BG2_TILEMAP: Region = Region::new(0x38000, TILE_WINDOW_BYTES);
/// The master palette as consecutive `R, G, B` bytes.
pub const MASTER_PALETTE: Region = Region::new(0x3A000, PALETTE_RGB888_BYTES);
pub const BG_PALETTES: Region = Region::new(0x3A300, BG_PALETTES_BYTES);
pub const OBJ_PALETTES: Region = Region::new(0x3A340, OBJ_PALETTES_BYTES);
pub const OBJ_ATTRIBUTES: Region = Region::new(0x3A400, OBJ_TILE_ATTRIBUTES_BYTES);
pub const OAM: Region = Region::new(0x3B000, OAM_BYTES);

/// Size of the register page, of which only a few bytes hold registers.
pub const REGISTER_PAGE_LEN: u32 = 0x1000;
pub const DISPLAY_ENABLE: u32 = 0x3C000;
pub const BACKDROP_COLOR: u32 = 0x3C001;
/// First byte of the register block of each BG layer, in layer order.
pub const BG_REGISTER_BLOCKS: [u32; BG_LAYERS] = [0x3C010, 0x3C020, 0x3C030];
/// Offsets of a BG layer's registers from the start of its block.
pub const BG_SCROLL_X: u32 = 0;
pub const BG_SCROLL_Y: u32 = 2;
pub const BG_PRIORITY: u32 = 4;
/// Bytes from `DISPLAY_ENABLE` through the last BG register. Only 17 of
/// them are registers, and the rest are unmapped.
pub const REGISTER_BYTES: usize =
    (BG_REGISTER_BLOCKS[BG_LAYERS - 1] + BG_PRIORITY + 1 - DISPLAY_ENABLE) as usize;
/// The extent of the registers, holes included.
pub const REGISTERS: Region = Region::new(DISPLAY_ENABLE, REGISTER_BYTES);

/// Every region in ascending address order.
const REGIONS: [Region; 13] = [
    WORK_RAM,
    FRAMEBUFFER,
    BG_TILES,
    OBJ_TILES,
    BG0_TILEMAP,
    BG1_TILEMAP,
    BG2_TILEMAP,
    MASTER_PALETTE,
    BG_PALETTES,
    OBJ_PALETTES,
    OBJ_ATTRIBUTES,
    OAM,
    REGISTERS,
];

const _: () = {
    let mut index = 1;
    while index < REGIONS.len() {
        assert!(REGIONS[index - 1].end() <= REGIONS[index].start);
        index += 1;
    }
    assert!(REGISTERS.len <= REGISTER_PAGE_LEN);
    assert!(REGISTERS.start + REGISTER_PAGE_LEN <= ADDRESS_SPACE_LEN);
};
