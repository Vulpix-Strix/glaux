//! A cartridge's video assets as machine state: its object tiles, their
//! attributes, its object palette banks and its master palette.

use glaux_machine::obj_palette::{ObjPalettes, ObjPalettesError};
use glaux_machine::palette::{Palette, PaletteBytesError};
use glaux_machine::sprite::{
    ObjTileAttributes, ObjTileAttributesError, ObjTileBank, ObjTileBankError, ObjVram,
};
use glaux_machine::system_rom::SystemRom;

use crate::cartridge::{CartridgeError, LoadedCartridge};

/// A cartridge video asset that is not valid native G1 data.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum VideoAssetError {
    #[error("Invalid sprites.bin: {0}")]
    ObjTiles(#[from] ObjTileBankError),
    #[error("Invalid objattr.bin: {0}")]
    ObjAttributes(#[from] ObjTileAttributesError),
    #[error("Invalid obp.bin: {0}")]
    ObjPalettes(#[from] ObjPalettesError),
    #[error("Invalid palette.bin: {0}")]
    Palette(#[from] PaletteBytesError),
}

/// Why a cartridge's video state could not be loaded.
#[derive(Debug, thiserror::Error)]
pub enum VideoLoadError {
    /// An asset file exists but could not be read.
    #[error(transparent)]
    Cartridge(CartridgeError),
    /// An asset was read but is not valid native data.
    #[error(transparent)]
    Asset(#[from] VideoAssetError),
}

/// The video state a cartridge starts with for each asset it does not
/// supply, taken from the System ROM.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CartridgeVideoDefaults {
    palette: Palette,
    obj_palettes: ObjPalettes,
}

impl CartridgeVideoDefaults {
    pub fn from_rom(rom: &SystemRom) -> Self {
        Self {
            palette: rom.palette().clone(),
            obj_palettes: rom.obj_palettes().clone(),
        }
    }
}

/// The raw bytes of a cartridge's optional video asset files, each `None`
/// when the cartridge does not supply it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct VideoFiles {
    pub obj_tiles: Option<Vec<u8>>,
    pub obj_attributes: Option<Vec<u8>>,
    pub obj_palettes: Option<Vec<u8>>,
    pub master_palette: Option<Vec<u8>>,
}

impl VideoFiles {
    /// Reads the cartridge's video asset files. They are only read.
    pub fn read(cartridge: &LoadedCartridge) -> Result<Self, CartridgeError> {
        Ok(Self {
            obj_tiles: cartridge.read_obj_tiles()?,
            obj_attributes: cartridge.read_obj_attributes()?,
            obj_palettes: cartridge.read_obj_palettes()?,
            master_palette: cartridge.read_master_palette()?,
        })
    }
}

/// A cartridge's video state, read from its asset files at one moment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CartridgeVideo {
    /// The master palette the cartridge's frames are presented with.
    pub palette: Palette,
    pub obj: ObjVram,
}

impl CartridgeVideo {
    /// Reads the cartridge's optional video assets, falling back to
    /// `defaults` for those it lacks. The files are only read.
    pub fn load(
        cartridge: &LoadedCartridge,
        defaults: &CartridgeVideoDefaults,
    ) -> Result<Self, VideoLoadError> {
        let files = VideoFiles::read(cartridge).map_err(VideoLoadError::Cartridge)?;
        Ok(Self::decode(&files, defaults)?)
    }

    /// Decodes `files` as native G1 data, falling back to `defaults` for
    /// the assets they lack.
    pub fn decode(
        files: &VideoFiles,
        defaults: &CartridgeVideoDefaults,
    ) -> Result<Self, VideoAssetError> {
        let tiles = match &files.obj_tiles {
            Some(bytes) => ObjTileBank::from_bytes(bytes)?,
            None => ObjTileBank::new(),
        };
        let attributes = match &files.obj_attributes {
            Some(bytes) => ObjTileAttributes::from_bytes(bytes)?,
            None => ObjTileAttributes::new(),
        };
        let palettes = match &files.obj_palettes {
            Some(bytes) => ObjPalettes::from_bytes(bytes)?,
            None => defaults.obj_palettes.clone(),
        };
        let palette = match &files.master_palette {
            Some(bytes) => Palette::from_rgb888(bytes)?,
            None => defaults.palette.clone(),
        };
        Ok(Self {
            palette,
            obj: ObjVram::new(tiles, attributes, palettes),
        })
    }
}
