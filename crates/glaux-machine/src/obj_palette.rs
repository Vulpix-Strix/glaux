//! G1 object palette banks (OBP0–OBP7).
//!
//! Each bank maps the 16 tile-local indices of a 4 bpp object tile to
//! master-palette indices. Local index 0 is always transparent, whatever its
//! bank entry holds.

/// Number of object palette banks in the G1 4 bpp profile.
pub const OBJ_PALETTE_BANKS: usize = 8;
/// Tile-local indices per bank, including the transparent index 0.
pub const OBJ_PALETTE_COLORS: usize = 16;
/// Size of the complete native object-palette state in bytes.
pub const OBJ_PALETTES_BYTES: usize = OBJ_PALETTE_BANKS * OBJ_PALETTE_COLORS;

/// An object palette bank number, always in `0..OBJ_PALETTE_BANKS`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ObjBank(u8);

impl ObjBank {
    /// OBP0.
    pub const ZERO: Self = Self(0);

    /// Returns the bank numbered `bank`, or `None` past the last bank.
    pub const fn new(bank: u8) -> Option<Self> {
        if (bank as usize) < OBJ_PALETTE_BANKS {
            Some(Self(bank))
        } else {
            None
        }
    }

    /// The bank number.
    pub const fn get(self) -> u8 {
        self.0
    }
}

/// A failure to construct object palettes from native G1 bytes.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ObjPalettesError {
    #[error("object palette data must be exactly {OBJ_PALETTES_BYTES} bytes, got {length}")]
    Length { length: usize },
    #[error(
        "object palette bank {bank} entry 0 must be 0, got {value}; local index 0 is transparent"
    )]
    ReservedEntry { bank: usize, value: u8 },
}

/// The eight G1 object palette banks.
///
/// Byte `bank * 16 + local` holds the master-palette index that tile-local
/// index `local` resolves to in that bank. Byte 0 of each bank is reserved
/// and always 0.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ObjPalettes {
    bytes: [u8; OBJ_PALETTES_BYTES],
}

impl ObjPalettes {
    /// Bank `n` maps local index `i` to master index `16 * n + i`.
    ///
    /// Bank 0 is therefore the identity, which is how object tiles resolved
    /// before palette banks existed.
    pub fn linear() -> Self {
        let mut bytes = [0; OBJ_PALETTES_BYTES];
        for (offset, byte) in bytes.iter_mut().enumerate() {
            if offset % OBJ_PALETTE_COLORS != 0 {
                // OBJ_PALETTES_BYTES is 128, so every offset fits in u8.
                *byte = offset as u8;
            }
        }
        Self { bytes }
    }

    /// Loads all banks from their native byte representation.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, ObjPalettesError> {
        let bytes: [u8; OBJ_PALETTES_BYTES] =
            bytes.try_into().map_err(|_| ObjPalettesError::Length {
                length: bytes.len(),
            })?;
        for bank in 0..OBJ_PALETTE_BANKS {
            let value = bytes[bank * OBJ_PALETTE_COLORS];
            if value != 0 {
                return Err(ObjPalettesError::ReservedEntry { bank, value });
            }
        }
        Ok(Self { bytes })
    }

    /// The master index `local` resolves to in `bank`.
    ///
    /// `None` means the pixel is transparent (local 0) or `local` is not a
    /// 4 bpp value.
    pub fn resolve(&self, bank: ObjBank, local: u8) -> Option<u8> {
        let local = usize::from(local);
        (1..OBJ_PALETTE_COLORS)
            .contains(&local)
            .then(|| self.bytes[usize::from(bank.get()) * OBJ_PALETTE_COLORS + local])
    }

    /// Returns the complete native byte representation of the banks.
    pub fn as_bytes(&self) -> &[u8; OBJ_PALETTES_BYTES] {
        &self.bytes
    }
}

impl Default for ObjPalettes {
    fn default() -> Self {
        Self::linear()
    }
}

#[cfg(test)]
mod tests;
