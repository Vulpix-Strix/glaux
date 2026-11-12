//! The G1 address map for the regions the machine implements.
//!
//! Addresses are 20 bits wide. Any address that no region covers is
//! unmapped.

use crate::bg::{BG_PALETTES_BYTES, BG_TILE_BANK_BYTES};
use crate::framebuffer;
use crate::obj_palette::OBJ_PALETTES_BYTES;
use crate::palette::PALETTE_RGB888_BYTES;
use crate::sprite::{OBJ_TILE_ATTRIBUTES_BYTES, OBJ_TILE_BANK_BYTES};

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
/// The master palette as consecutive `R, G, B` bytes.
pub const MASTER_PALETTE: Region = Region::new(0x3A000, PALETTE_RGB888_BYTES);
pub const BG_PALETTES: Region = Region::new(0x3A300, BG_PALETTES_BYTES);
pub const OBJ_PALETTES: Region = Region::new(0x3A340, OBJ_PALETTES_BYTES);
pub const OBJ_ATTRIBUTES: Region = Region::new(0x3A400, OBJ_TILE_ATTRIBUTES_BYTES);

/// Every region in ascending address order.
const REGIONS: [Region; 8] = [
    WORK_RAM,
    FRAMEBUFFER,
    BG_TILES,
    OBJ_TILES,
    MASTER_PALETTE,
    BG_PALETTES,
    OBJ_PALETTES,
    OBJ_ATTRIBUTES,
];

const _: () = {
    let mut index = 1;
    while index < REGIONS.len() {
        assert!(REGIONS[index - 1].end() <= REGIONS[index].start);
        index += 1;
    }
    assert!(OBJ_ATTRIBUTES.end() <= ADDRESS_SPACE_LEN);
};
