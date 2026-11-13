//! Object Attribute Memory (OAM).
//!
//! 128 objects, 8 bytes per entry.
//!
//! ```text
//! Byte   +0    +1    +2    +3    +4    +5    +6    +7
//!       +-----------+-----------+-----------+-----+-----+
//!       |     X     |     Y     | Base tile | Attr| Size|
//!       +-----------+-----------+-----------+-----+-----+
//!         i16 LE      i16 LE      u16 LE
//!
//! Base tile (+4..+5)
//!  15          10 9                   0
//! +--------------+---------------------+
//! |   Reserved   |      Tile index     |
//! +--------------+---------------------+
//!
//! Attributes (+6)
//!   7    6    5    4    3    2    1    0
//! +----+---------+----+----+--------------+
//! | R  | Priority| FY | FX |   OBP bank   |
//! +----+---------+----+----+--------------+
//!
//! Size and control (+7)
//!   7    6    5    4    3    2    1    0
//! +-------------------+----+--------------+
//! |     Reserved      | EN |  Size code   |
//! +-------------------+----+--------------+
//!
//! FX = Flip X    FY = Flip Y
//! EN = Enabled   R  = Reserved
//! ```
use crate::obj_palette::{OBJ_PALETTE_BANKS, ObjBank};
use crate::sprite::OBJ_TILE_CAPACITY;

/// Number of objects.
pub const OAM_ENTRIES: usize = 128;
/// Size of one entry in bytes.
pub const OAM_ENTRY_BYTES: usize = 8;
/// Size of the whole table in bytes.
pub const OAM_BYTES: usize = OAM_ENTRIES * OAM_ENTRY_BYTES;

/// The bits each byte of an entry can hold.
const BYTE_MASKS: [u8; OAM_ENTRY_BYTES] = [0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x03, 0x7F, 0x0F];

/// Offsets of the fields within an entry.
pub const FIELD_X: usize = 0;
pub const FIELD_Y: usize = 2;
pub const FIELD_TILE: usize = 4;
pub const FIELD_ATTRIBUTES: usize = 6;
pub const FIELD_SIZE: usize = 7;

/// Mask of the tile index in the base tile field.
pub const TILE_MASK: u16 = (OBJ_TILE_CAPACITY - 1) as u16;
/// Mask of the palette bank in the attributes byte.
pub const ATTR_BANK_MASK: u8 = (OBJ_PALETTE_BANKS - 1) as u8;
pub const ATTR_FLIP_X: u8 = 1 << 3;
pub const ATTR_FLIP_Y: u8 = 1 << 4;
pub const ATTR_PRIORITY_SHIFT: u8 = 5;
/// Mask of the priority once shifted down by [`ATTR_PRIORITY_SHIFT`].
pub const ATTR_PRIORITY_MASK: u8 = 0b11;
/// Mask of the size code in the size byte.
pub const SIZE_MASK: u8 = 0b111;
/// Set in the size byte for an object that takes part in drawing.
pub const ENABLED: u8 = 1 << 3;

/// Size codes, named by the pixel size of the object.
pub const SIZE_8X8: u8 = 0;
pub const SIZE_16X16: u8 = 1;
pub const SIZE_32X32: u8 = 2;
pub const SIZE_8X16: u8 = 3;
pub const SIZE_16X8: u8 = 4;
pub const SIZE_16X32: u8 = 5;
pub const SIZE_32X16: u8 = 6;
/// Reserved code. An object with it is invisible even when enabled.
pub const SIZE_RESERVED: u8 = 7;

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
    /// The size of the object as `(columns, rows)` of 8x8 tiles, or `None`
    /// when the size code makes the object invisible. Code 7 is reserved, and
    /// so is any value that does not fit in the 3 bits of the field.
    pub const fn tiles(&self) -> Option<(usize, usize)> {
        match self.size {
            SIZE_8X8 => Some((1, 1)),
            SIZE_16X16 => Some((2, 2)),
            SIZE_32X32 => Some((4, 4)),
            SIZE_8X16 => Some((1, 2)),
            SIZE_16X8 => Some((2, 1)),
            SIZE_16X32 => Some((2, 4)),
            SIZE_32X16 => Some((4, 2)),
            _ => None,
        }
    }

    fn to_bytes(self) -> [u8; OAM_ENTRY_BYTES] {
        let mut bytes = [0; OAM_ENTRY_BYTES];
        bytes[FIELD_X..FIELD_X + 2].copy_from_slice(&self.x.to_le_bytes());
        bytes[FIELD_Y..FIELD_Y + 2].copy_from_slice(&self.y.to_le_bytes());
        bytes[FIELD_TILE..FIELD_TILE + 2].copy_from_slice(&(self.tile & TILE_MASK).to_le_bytes());
        let mut attributes =
            self.bank.get() | (self.priority & ATTR_PRIORITY_MASK) << ATTR_PRIORITY_SHIFT;
        if self.flip_x {
            attributes |= ATTR_FLIP_X;
        }
        if self.flip_y {
            attributes |= ATTR_FLIP_Y;
        }
        bytes[FIELD_ATTRIBUTES] = attributes;
        let mut size = self.size & SIZE_MASK;
        if self.enabled {
            size |= ENABLED;
        }
        bytes[FIELD_SIZE] = size;
        bytes
    }

    fn from_bytes(bytes: [u8; OAM_ENTRY_BYTES]) -> Self {
        let word = |field: usize| [bytes[field], bytes[field + 1]];
        let attributes = bytes[FIELD_ATTRIBUTES];
        let size = bytes[FIELD_SIZE];
        Self {
            x: i16::from_le_bytes(word(FIELD_X)),
            y: i16::from_le_bytes(word(FIELD_Y)),
            tile: u16::from_le_bytes(word(FIELD_TILE)),
            bank: ObjBank::new(attributes & ATTR_BANK_MASK).unwrap_or(ObjBank::ZERO),
            flip_x: attributes & ATTR_FLIP_X != 0,
            flip_y: attributes & ATTR_FLIP_Y != 0,
            priority: (attributes >> ATTR_PRIORITY_SHIFT) & ATTR_PRIORITY_MASK,
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
