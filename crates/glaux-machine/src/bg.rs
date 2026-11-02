//! The tiles and palette banks that the entries of [`crate::tilemap`] point
//! at.
//!
//! Background tiles use the same 8x8, 4 bpp encoding as object tiles, in a
//! bank of their own. Palette index 0 is transparent in every tile.

use crate::sprite::{OBJ_TILE_BANK_BYTES, OBJ_TILE_CAPACITY, ObjTileBank, ObjTileBankError};
use crate::tilemap::{BG_PALETTE_BANKS, BG_TILE_CAPACITY, BgBank};

/// Encoded size of one 4 bpp background tile.
pub const BG_TILE_BYTES: usize = 32;
/// Size of the complete native background tile bank in bytes.
pub const BG_TILE_BANK_BYTES: usize = BG_TILE_CAPACITY * BG_TILE_BYTES;
/// Tile-local indices per background palette bank, including the
/// transparent index 0.
pub const BG_PALETTE_COLORS: usize = 16;
/// Size of the complete native background-palette state in bytes.
pub const BG_PALETTES_BYTES: usize = BG_PALETTE_BANKS * BG_PALETTE_COLORS;

const _: () = assert!(BG_TILE_BANK_BYTES == OBJ_TILE_BANK_BYTES);
const _: () = assert!(BG_TILE_CAPACITY == OBJ_TILE_CAPACITY);

/// A failure to construct a background tile bank from native G1 bytes.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum BgTileBankError {
    #[error(
        "background tile data length must be a multiple of {BG_TILE_BYTES} bytes, got {length}"
    )]
    Misaligned { length: usize },
    #[error(
        "background tile data is too large: got {length} bytes, maximum is {BG_TILE_BANK_BYTES}"
    )]
    TooLarge { length: usize },
}

/// The native G1 background tile bank, holding 1,024 consecutive 8x8, 4 bpp
/// tiles.
///
/// It shares the object tile encoding, so the pixel layout is defined in one
/// place. Bytes beyond loaded tile data remain zeroed.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BgTileBank(ObjTileBank);

impl BgTileBank {
    /// A bank containing only transparent pixels.
    pub fn new() -> Self {
        Self::default()
    }

    /// Loads consecutive native tiles at the beginning of a new bank.
    ///
    /// `bytes` must contain whole tiles and fit within the 32 KiB bank.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, BgTileBankError> {
        ObjTileBank::from_bytes(bytes)
            .map(Self)
            .map_err(|err| match err {
                ObjTileBankError::Misaligned { length } => BgTileBankError::Misaligned { length },
                ObjTileBankError::TooLarge { length } => BgTileBankError::TooLarge { length },
            })
    }

    /// One tile-local palette index, or `None` for an invalid tile or
    /// coordinate. See [`ObjTileBank::pixel`].
    pub fn pixel(&self, tile: usize, x: usize, y: usize) -> Option<u8> {
        self.0.pixel(tile, x, y)
    }

    /// Writes one tile-local palette index and returns the one it replaced.
    /// See [`ObjTileBank::set_pixel`].
    pub fn set_pixel(&mut self, tile: usize, x: usize, y: usize, index: u8) -> Option<u8> {
        self.0.set_pixel(tile, x, y, index)
    }

    /// The complete native byte representation of the bank.
    pub fn as_bytes(&self) -> &[u8; BG_TILE_BANK_BYTES] {
        self.0.as_bytes()
    }
}

/// A failure to construct background palettes from native G1 bytes.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum BgPalettesError {
    #[error("background palette data must be exactly {BG_PALETTES_BYTES} bytes, got {length}")]
    Length { length: usize },
    #[error(
        "background palette bank {bank} entry 0 must be 0, got {value}; local index 0 is transparent"
    )]
    ReservedEntry { bank: usize, value: u8 },
}

/// The four G1 background palette banks (BGP0 to BGP3).
///
/// Byte `bank * 16 + local` holds the master-palette index that tile-local
/// index `local` resolves to in that bank. Byte 0 of each bank is reserved
/// and always 0.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BgPalettes {
    bytes: [u8; BG_PALETTES_BYTES],
}

impl BgPalettes {
    /// Bank `n` maps local index `i` to master index `16 * n + i`.
    pub fn linear() -> Self {
        let mut bytes = [0; BG_PALETTES_BYTES];
        for (offset, byte) in bytes.iter_mut().enumerate() {
            if offset % BG_PALETTE_COLORS != 0 {
                // BG_PALETTES_BYTES is 64, so every offset fits in u8.
                *byte = offset as u8;
            }
        }
        Self { bytes }
    }

    /// Loads all banks from their native byte representation.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, BgPalettesError> {
        let bytes: [u8; BG_PALETTES_BYTES] =
            bytes.try_into().map_err(|_| BgPalettesError::Length {
                length: bytes.len(),
            })?;
        for bank in 0..BG_PALETTE_BANKS {
            let value = bytes[bank * BG_PALETTE_COLORS];
            if value != 0 {
                return Err(BgPalettesError::ReservedEntry { bank, value });
            }
        }
        Ok(Self { bytes })
    }

    /// The master index `local` resolves to in `bank`.
    ///
    /// `None` means the pixel is transparent (local 0) or `local` is not a
    /// 4 bpp value.
    pub fn resolve(&self, bank: BgBank, local: u8) -> Option<u8> {
        let local = usize::from(local);
        (1..BG_PALETTE_COLORS)
            .contains(&local)
            .then(|| self.bytes[usize::from(bank.get()) * BG_PALETTE_COLORS + local])
    }

    /// Maps `local` in `bank` to master index `master` and returns the
    /// master it mapped to before.
    ///
    /// `None` means `local` is the reserved transparent index 0 or not a
    /// 4 bpp value. Nothing is written then.
    pub fn set(&mut self, bank: BgBank, local: u8, master: u8) -> Option<u8> {
        let previous = self.resolve(bank, local)?;
        self.bytes[usize::from(bank.get()) * BG_PALETTE_COLORS + usize::from(local)] = master;
        Some(previous)
    }

    /// The complete native byte representation of the banks.
    pub fn as_bytes(&self) -> &[u8; BG_PALETTES_BYTES] {
        &self.bytes
    }
}

impl Default for BgPalettes {
    fn default() -> Self {
        Self::linear()
    }
}

/// A cartridge's background tiles together with the palette banks they
/// resolve through.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BgVram {
    tiles: BgTileBank,
    palettes: BgPalettes,
}

impl BgVram {
    pub fn new(tiles: BgTileBank, palettes: BgPalettes) -> Self {
        Self { tiles, palettes }
    }

    pub fn tiles(&self) -> &BgTileBank {
        &self.tiles
    }

    pub fn tiles_mut(&mut self) -> &mut BgTileBank {
        &mut self.tiles
    }

    pub fn palettes(&self) -> &BgPalettes {
        &self.palettes
    }

    pub fn palettes_mut(&mut self) -> &mut BgPalettes {
        &mut self.palettes
    }
}

#[cfg(test)]
mod tests;
