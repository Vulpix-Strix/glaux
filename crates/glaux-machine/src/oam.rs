//! 128 entries of 8 bytes.
//!
//! | Offset  | Field                                                       |
//! |---------|-------------------------------------------------------------|
//! | +0..+1  | X, signed 16 bits                                           |
//! | +2..+3  | Y, signed 16 bits                                           |
//! | +4..+5  | first tile in bits 0-9, bits 10-15 reserved                 |
//! | +6      | bits 0-2 OBP bank, 3 flip X, 4 flip Y, 5-6 priority, 7 reserved |
//! | +7      | bits 0-2 size code, 3 enabled, 4-7 reserved                 |

use crate::obj_palette::ObjBank;
use crate::sprite::OBJ_TILE_CAPACITY;

/// Number of objects.
pub const OAM_ENTRIES: usize = 128;
/// Size of one entry in bytes.
pub const OAM_ENTRY_BYTES: usize = 8;
/// Size of the whole table in bytes.
pub const OAM_BYTES: usize = OAM_ENTRIES * OAM_ENTRY_BYTES;

/// The bits each byte of an entry can hold.
const BYTE_MASKS: [u8; OAM_ENTRY_BYTES] = [0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x03, 0x7F, 0x0F];

const TILE_MASK: u16 = (OBJ_TILE_CAPACITY - 1) as u16;
const PRIORITY_MASK: u8 = 0b11;
const SIZE_MASK: u8 = 0b111;
const FLIP_X: u8 = 1 << 3;
const FLIP_Y: u8 = 1 << 4;
const PRIORITY_SHIFT: u8 = 5;
const ENABLED: u8 = 1 << 3;

const _: () = assert!(OBJ_TILE_CAPACITY.is_power_of_two());

/// One object, decoded from or encoded into its 8 bytes.
///
/// Packing keeps every field inside its own bits: `tile` keeps its low 10
/// bits, `priority` its low 2 and `size` its low 3. Size code 7 is stored as
/// given. It marks the object invisible when objects are drawn.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct OamEntry {
    pub x: i16,
    pub y: i16,
    /// The first tile of the object in the OBJ bank.
    pub tile: u16,
    pub bank: ObjBank,
    pub flip_x: bool,
    pub flip_y: bool,
    /// 0 is in front, 3 is behind.
    pub priority: u8,
    pub size: u8,
    pub enabled: bool,
}

impl OamEntry {
    fn to_bytes(self) -> [u8; OAM_ENTRY_BYTES] {
        let [x0, x1] = self.x.to_le_bytes();
        let [y0, y1] = self.y.to_le_bytes();
        let [tile0, tile1] = (self.tile & TILE_MASK).to_le_bytes();
        let mut attributes = self.bank.get() | (self.priority & PRIORITY_MASK) << PRIORITY_SHIFT;
        if self.flip_x {
            attributes |= FLIP_X;
        }
        if self.flip_y {
            attributes |= FLIP_Y;
        }
        let mut size = self.size & SIZE_MASK;
        if self.enabled {
            size |= ENABLED;
        }
        [x0, x1, y0, y1, tile0, tile1, attributes, size]
    }

    fn from_bytes(bytes: [u8; OAM_ENTRY_BYTES]) -> Self {
        let [x0, x1, y0, y1, tile0, tile1, attributes, size] = bytes;
        Self {
            x: i16::from_le_bytes([x0, x1]),
            y: i16::from_le_bytes([y0, y1]),
            tile: u16::from_le_bytes([tile0, tile1]),
            bank: ObjBank::new(attributes & 0b111).unwrap_or(ObjBank::ZERO),
            flip_x: attributes & FLIP_X != 0,
            flip_y: attributes & FLIP_Y != 0,
            priority: (attributes >> PRIORITY_SHIFT) & PRIORITY_MASK,
            size: size & SIZE_MASK,
            enabled: size & ENABLED != 0,
        }
    }
}

/// The 128 object entries as bytes. A new table is all zero, so every object
/// is disabled.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Oam {
    bytes: Box<[u8; OAM_BYTES]>,
}

impl Oam {
    pub fn new() -> Self {
        Self {
            bytes: Box::new([0; OAM_BYTES]),
        }
    }

    /// The object at `index`, or `None` past the last one.
    pub fn entry(&self, index: usize) -> Option<OamEntry> {
        if index >= OAM_ENTRIES {
            return None;
        }
        let start = index * OAM_ENTRY_BYTES;
        let bytes = self.bytes[start..start + OAM_ENTRY_BYTES].try_into().ok()?;
        Some(OamEntry::from_bytes(bytes))
    }

    /// Writes the object at `index`. Returns `false` past the last one.
    /// Each byte goes through [`set_byte`](Self::set_byte), like a bus write.
    pub fn set_entry(&mut self, index: usize, entry: OamEntry) -> bool {
        if index >= OAM_ENTRIES {
            return false;
        }
        let start = index * OAM_ENTRY_BYTES;
        for (offset, byte) in entry.to_bytes().into_iter().enumerate() {
            self.set_byte(start + offset, byte);
        }
        true
    }

    /// Writes byte `offset` of the table, dropping the reserved bits of its
    /// position in the entry. Returns `false` past the end.
    pub fn set_byte(&mut self, offset: usize, value: u8) -> bool {
        let Some(byte) = self.bytes.get_mut(offset) else {
            return false;
        };
        *byte = value & BYTE_MASKS[offset % OAM_ENTRY_BYTES];
        true
    }

    pub fn as_bytes(&self) -> &[u8; OAM_BYTES] {
        &self.bytes
    }
}

impl Default for Oam {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests;
