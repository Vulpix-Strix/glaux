//! Graphics registers.

use crate::memory_map::{
    BACKDROP_COLOR, BG_PRIORITY, BG_REGISTER_BLOCKS, BG_SCROLL_X, BG_SCROLL_Y, DISPLAY_ENABLE,
    REGISTER_BYTES,
};
use crate::tilemap::{BG_LAYERS, BgLayer};

/// Bits of `DISPLAY_ENABLE` that switch on BG0, BG1, BG2 and the objects.
pub const DISPLAY_BG0: u8 = 1 << 0;
pub const DISPLAY_BG1: u8 = 1 << 1;
pub const DISPLAY_BG2: u8 = 1 << 2;
pub const DISPLAY_OBJ: u8 = 1 << 3;
/// The writable bits of `DISPLAY_ENABLE`. Bits 4-7 are reserved.
pub const DISPLAY_ENABLE_MASK: u8 = DISPLAY_BG0 | DISPLAY_BG1 | DISPLAY_BG2 | DISPLAY_OBJ;
/// Only bits 0-1 of a BG priority mean anything.
pub const BG_PRIORITY_MASK: u8 = 0b11;
/// Priorities at power-on: BG2 in front of BG1 in front of BG0.
const INITIAL_PRIORITIES: [u8; BG_LAYERS] = [3, 2, 1];

/// Offset of `address` from the first register.
const fn offset(address: u32) -> usize {
    (address - DISPLAY_ENABLE) as usize
}

fn bg_offset(layer: BgLayer, field: u32) -> usize {
    offset(BG_REGISTER_BLOCKS[layer.index()] + field)
}

/// The bits of the register at `offset` that can be written, or `None` if
/// no register is there.
fn writable_mask(register: usize) -> Option<u8> {
    if register == offset(DISPLAY_ENABLE) {
        return Some(DISPLAY_ENABLE_MASK);
    }
    if register == offset(BACKDROP_COLOR) {
        return Some(0xFF);
    }
    BgLayer::ALL.into_iter().find_map(|layer| {
        let field = register.checked_sub(bg_offset(layer, 0))?;
        match field {
            0..BG_PRIORITY_FIELD => Some(0xFF),
            BG_PRIORITY_FIELD => Some(BG_PRIORITY_MASK),
            _ => None,
        }
    })
}

const BG_PRIORITY_FIELD: usize = BG_PRIORITY as usize;

/// The register file. Offsets are counted from `DISPLAY_ENABLE`, and the
/// bytes between registers are never written.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GraphicsRegisters {
    bytes: [u8; REGISTER_BYTES],
}

impl GraphicsRegisters {
    /// The power-on state: everything off and zero, with the initial BG
    /// priorities.
    pub fn new() -> Self {
        let mut bytes = [0; REGISTER_BYTES];
        for layer in BgLayer::ALL {
            bytes[bg_offset(layer, BG_PRIORITY)] = INITIAL_PRIORITIES[layer.index()];
        }
        Self { bytes }
    }

    /// Byte `register` counted from `DISPLAY_ENABLE`, or `None` where no
    /// register exists.
    pub fn byte(&self, register: usize) -> Option<u8> {
        writable_mask(register)?;
        self.bytes.get(register).copied()
    }

    /// Writes byte `register`, dropping the reserved bits. Returns `false`
    /// where no register exists.
    pub fn set_byte(&mut self, register: usize, value: u8) -> bool {
        let Some(mask) = writable_mask(register) else {
            return false;
        };
        match self.bytes.get_mut(register) {
            Some(byte) => {
                *byte = value & mask;
                true
            }
            None => false,
        }
    }

    /// The layer enable bits: BG0, BG1, BG2 and OBJ in bits 0 to 3.
    pub fn display_enable(&self) -> u8 {
        self.bytes[offset(DISPLAY_ENABLE)]
    }

    /// The master palette index the frame starts from.
    pub fn backdrop_color(&self) -> u8 {
        self.bytes[offset(BACKDROP_COLOR)]
    }

    /// The horizontal scroll as written, without wrapping.
    pub fn bg_scroll_x(&self, layer: BgLayer) -> u16 {
        self.scroll(bg_offset(layer, BG_SCROLL_X))
    }

    /// The vertical scroll as written, without wrapping.
    pub fn bg_scroll_y(&self, layer: BgLayer) -> u16 {
        self.scroll(bg_offset(layer, BG_SCROLL_Y))
    }

    /// 0 is in front, 3 is behind.
    pub fn bg_priority(&self, layer: BgLayer) -> u8 {
        self.bytes[bg_offset(layer, BG_PRIORITY)]
    }

    fn scroll(&self, start: usize) -> u16 {
        u16::from_le_bytes([self.bytes[start], self.bytes[start + 1]])
    }
}

impl Default for GraphicsRegisters {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests;
