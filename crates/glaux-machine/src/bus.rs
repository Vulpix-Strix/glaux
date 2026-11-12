//! The 20-bit memory bus over [`CartridgeHardware`].
//!
//! The bus owns no memory. Every access is dispatched to the device that
//! holds the bytes, and each device applies its own restrictions to writes.
//! Addresses that no region covers read as 0 and ignore writes. Addresses
//! outside the 20-bit space are an error.

use crate::hardware::CartridgeHardware;
use crate::memory_map::{
    ADDRESS_SPACE_LEN, BG_PALETTES, BG_TILES, BG0_TILEMAP, BG1_TILEMAP, BG2_TILEMAP, FRAMEBUFFER,
    MASTER_PALETTE, OAM, OBJ_ATTRIBUTES, OBJ_PALETTES, OBJ_TILES, REGISTERS, Region, WORK_RAM,
};
use crate::tilemap::BgLayer;

/// A bus access that leaves the 20-bit address space.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum BusError {
    #[error("{length} byte(s) at {address:#x} fall outside the 20-bit address space")]
    OutOfRange { address: u32, length: u32 },
}

#[derive(Clone, Copy)]
enum Device {
    WorkRam,
    Framebuffer,
    BgTiles,
    ObjTiles,
    Tilemap(BgLayer),
    MasterPalette,
    BgPalettes,
    ObjPalettes,
    ObjAttributes,
    Oam,
    Registers,
}

const DEVICES: [(Device, Region); 13] = [
    (Device::WorkRam, WORK_RAM),
    (Device::Framebuffer, FRAMEBUFFER),
    (Device::BgTiles, BG_TILES),
    (Device::ObjTiles, OBJ_TILES),
    (Device::Tilemap(BgLayer::Bg0), BG0_TILEMAP),
    (Device::Tilemap(BgLayer::Bg1), BG1_TILEMAP),
    (Device::Tilemap(BgLayer::Bg2), BG2_TILEMAP),
    (Device::MasterPalette, MASTER_PALETTE),
    (Device::BgPalettes, BG_PALETTES),
    (Device::ObjPalettes, OBJ_PALETTES),
    (Device::ObjAttributes, OBJ_ATTRIBUTES),
    (Device::Oam, OAM),
    (Device::Registers, REGISTERS),
];

/// The device behind `address` and the offset into it, or `None` if the
/// address is unmapped.
fn locate(address: u32) -> Option<(Device, usize)> {
    DEVICES
        .iter()
        .find(|(_, region)| region.contains(address))
        .map(|&(device, region)| (device, (address - region.start) as usize))
}

/// Checks that `length` bytes from `start` lie inside the address space.
/// An empty range still needs a valid `start`. The subtraction cannot
/// underflow after the first test, and nothing is added, so no input can wrap.
///
/// Public so a caller can reject a range before paying for the access.
pub fn check_range(start: u32, length: u32) -> Result<(), BusError> {
    if start < ADDRESS_SPACE_LEN && length <= ADDRESS_SPACE_LEN - start {
        Ok(())
    } else {
        Err(BusError::OutOfRange {
            address: start,
            length,
        })
    }
}

impl CartridgeHardware {
    /// Reads the byte at `address`.
    pub fn read(&self, address: u32) -> Result<u8, BusError> {
        check_range(address, 1)?;
        Ok(self.read_mapped(address))
    }

    /// Writes `value` at `address`, subject to the rules of the device that
    /// holds it.
    pub fn write(&mut self, address: u32, value: u8) -> Result<(), BusError> {
        check_range(address, 1)?;
        self.write_mapped(address, value);
        Ok(())
    }

    /// Reads a little-endian 16-bit value. The two bytes may belong to
    /// different regions.
    pub fn read16(&self, address: u32) -> Result<u16, BusError> {
        self.read_bytes(address).map(u16::from_le_bytes)
    }

    /// Reads a little-endian 32-bit value.
    pub fn read32(&self, address: u32) -> Result<u32, BusError> {
        self.read_bytes(address).map(u32::from_le_bytes)
    }

    /// Writes a little-endian 16-bit value, one byte at a time. The range is
    /// checked first, so a failed write changes nothing.
    pub fn write16(&mut self, address: u32, value: u16) -> Result<(), BusError> {
        self.write_bytes(address, value.to_le_bytes())
    }

    /// Writes a little-endian 32-bit value. See [`write16`](Self::write16).
    pub fn write32(&mut self, address: u32, value: u32) -> Result<(), BusError> {
        self.write_bytes(address, value.to_le_bytes())
    }

    /// Copies `length` bytes with `memmove` semantics: the result is as if
    /// the source had been read completely before anything was written.
    /// Both ranges are checked before the first write. Each byte goes through
    /// the same rules as a single [`write`](Self::write).
    pub fn copy(&mut self, destination: u32, source: u32, length: u32) -> Result<(), BusError> {
        check_range(destination, length)?;
        check_range(source, length)?;
        // Writing in the direction away from the overlap never touches a
        // source byte that has not been read yet.
        if destination <= source {
            for offset in 0..length {
                let byte = self.read_mapped(source + offset);
                self.write_mapped(destination + offset, byte);
            }
        } else {
            for offset in (0..length).rev() {
                let byte = self.read_mapped(source + offset);
                self.write_mapped(destination + offset, byte);
            }
        }
        Ok(())
    }

    /// Writes `value` to `length` consecutive bytes, with the rules of a
    /// single [`write`](Self::write) for each.
    pub fn fill(&mut self, destination: u32, value: u8, length: u32) -> Result<(), BusError> {
        check_range(destination, length)?;
        for offset in 0..length {
            self.write_mapped(destination + offset, value);
        }
        Ok(())
    }

    fn read_bytes<const N: usize>(&self, address: u32) -> Result<[u8; N], BusError> {
        check_range(address, N as u32)?;
        Ok(std::array::from_fn(|offset| {
            self.read_mapped(address + offset as u32)
        }))
    }

    fn write_bytes<const N: usize>(
        &mut self,
        address: u32,
        bytes: [u8; N],
    ) -> Result<(), BusError> {
        check_range(address, N as u32)?;
        for (offset, byte) in bytes.into_iter().enumerate() {
            self.write_mapped(address + offset as u32, byte);
        }
        Ok(())
    }

    /// Reads an address already known to be inside the address space.
    fn read_mapped(&self, address: u32) -> u8 {
        let Some((device, offset)) = locate(address) else {
            return 0;
        };
        match device {
            Device::WorkRam => self.work_ram[offset],
            Device::Framebuffer => self.framebuffer.as_bytes()[offset],
            Device::BgTiles => self.bg.tiles().as_bytes()[offset],
            Device::ObjTiles => self.obj.tiles().as_bytes()[offset],
            Device::Tilemap(layer) => self.tilemaps[layer.index()].as_bytes()[offset],
            Device::MasterPalette => self.palette.byte(offset).unwrap_or(0),
            Device::BgPalettes => self.bg.palettes().as_bytes()[offset],
            Device::ObjPalettes => self.obj.palettes().as_bytes()[offset],
            Device::ObjAttributes => self.obj.attributes().as_bytes()[offset],
            Device::Oam => self.oam.as_bytes()[offset],
            Device::Registers => self.registers.byte(offset).unwrap_or(0),
        }
    }

    /// Writes an address already known to be inside the address space. The
    /// devices that restrict their bytes do so in `set_byte`.
    fn write_mapped(&mut self, address: u32, value: u8) {
        let Some((device, offset)) = locate(address) else {
            return;
        };
        match device {
            Device::WorkRam => self.work_ram[offset] = value,
            Device::Framebuffer => self.framebuffer.as_bytes_mut()[offset] = value,
            Device::BgTiles => {
                self.bg.tiles_mut().set_byte(offset, value);
            }
            Device::ObjTiles => {
                self.obj.tiles_mut().set_byte(offset, value);
            }
            Device::Tilemap(layer) => {
                self.tilemaps[layer.index()].set_byte(offset, value);
            }
            Device::MasterPalette => {
                self.palette.set_byte(offset, value);
            }
            Device::BgPalettes => {
                self.bg.palettes_mut().set_byte(offset, value);
            }
            Device::ObjPalettes => {
                self.obj.palettes_mut().set_byte(offset, value);
            }
            Device::ObjAttributes => {
                self.obj.attributes_mut().set_byte(offset, value);
            }
            Device::Oam => {
                self.oam.set_byte(offset, value);
            }
            Device::Registers => {
                self.registers.set_byte(offset, value);
            }
        }
    }
}

#[cfg(test)]
mod tests;
