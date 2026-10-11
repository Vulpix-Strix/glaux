//! A cartridge's video assets as machine state: its object tiles, their
//! attributes, its object palette banks and its master palette.

use std::borrow::Cow;

use glaux_machine::obj_palette::{ObjPalettes, ObjPalettesError};
use glaux_machine::palette::{Palette, PaletteBytesError};
use glaux_machine::sprite::{
    ObjTileAttributes, ObjTileAttributesError, ObjTileBank, ObjTileBankError, ObjVram,
};
use glaux_machine::system_rom::SystemRom;

use crate::cartridge::{CartridgeError, LoadedCartridge, VideoAsset};

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

    /// The master palette of a cartridge without `palette.bin`.
    pub fn palette(&self) -> &Palette {
        &self.palette
    }

    /// The object palette banks of a cartridge without `obp.bin`.
    pub fn obj_palettes(&self) -> &ObjPalettes {
        &self.obj_palettes
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
            obj_tiles: cartridge.read_asset(VideoAsset::ObjTiles)?,
            obj_attributes: cartridge.read_asset(VideoAsset::ObjAttributes)?,
            obj_palettes: cartridge.read_asset(VideoAsset::ObjPalettes)?,
            master_palette: cartridge.read_asset(VideoAsset::MasterPalette)?,
        })
    }

    /// The bytes of `asset`'s file, `None` when it is absent.
    pub fn get(&self, asset: VideoAsset) -> Option<&[u8]> {
        match asset {
            VideoAsset::ObjTiles => &self.obj_tiles,
            VideoAsset::ObjAttributes => &self.obj_attributes,
            VideoAsset::ObjPalettes => &self.obj_palettes,
            VideoAsset::MasterPalette => &self.master_palette,
        }
        .as_deref()
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
        let mut video = Self {
            palette: defaults.palette.clone(),
            obj: ObjVram::new(
                ObjTileBank::new(),
                ObjTileAttributes::new(),
                defaults.obj_palettes.clone(),
            ),
        };
        for asset in VideoAsset::ALL {
            video.decode_asset(asset, files.get(asset), defaults)?;
        }
        Ok(video)
    }

    /// Replaces the state of `asset` with `bytes` decoded as its native
    /// data, or with its default when `bytes` is `None`. On error nothing
    /// changes.
    pub fn decode_asset(
        &mut self,
        asset: VideoAsset,
        bytes: Option<&[u8]>,
        defaults: &CartridgeVideoDefaults,
    ) -> Result<(), VideoAssetError> {
        match asset {
            VideoAsset::ObjTiles => {
                *self.obj.tiles_mut() = match bytes {
                    Some(bytes) => ObjTileBank::from_bytes(bytes)?,
                    None => ObjTileBank::new(),
                };
            }
            VideoAsset::ObjAttributes => {
                *self.obj.attributes_mut() = match bytes {
                    Some(bytes) => ObjTileAttributes::from_bytes(bytes)?,
                    None => ObjTileAttributes::new(),
                };
            }
            VideoAsset::ObjPalettes => {
                *self.obj.palettes_mut() = match bytes {
                    Some(bytes) => ObjPalettes::from_bytes(bytes)?,
                    None => defaults.obj_palettes.clone(),
                };
            }
            VideoAsset::MasterPalette => {
                self.palette = match bytes {
                    Some(bytes) => Palette::from_rgb888(bytes)?,
                    None => defaults.palette.clone(),
                };
            }
        }
        Ok(())
    }

    /// The complete native encoding of `asset`. A file of the asset holds
    /// a prefix of it: the whole of it for the palettes, and possibly fewer
    /// tiles for the tile bank and its attributes.
    pub fn native_bytes(&self, asset: VideoAsset) -> Cow<'_, [u8]> {
        match asset {
            VideoAsset::ObjTiles => Cow::Borrowed(self.obj.tiles().as_bytes()),
            VideoAsset::ObjAttributes => Cow::Borrowed(self.obj.attributes().as_bytes()),
            VideoAsset::ObjPalettes => Cow::Borrowed(self.obj.palettes().as_bytes()),
            VideoAsset::MasterPalette => Cow::Owned(self.palette.to_rgb888().to_vec()),
        }
    }

    /// Takes the state of `asset` from `other`, keeping the rest.
    pub fn copy_asset(&mut self, other: &Self, asset: VideoAsset) {
        match asset {
            VideoAsset::ObjTiles => *self.obj.tiles_mut() = other.obj.tiles().clone(),
            VideoAsset::ObjAttributes => {
                *self.obj.attributes_mut() = other.obj.attributes().clone();
            }
            VideoAsset::ObjPalettes => *self.obj.palettes_mut() = other.obj.palettes().clone(),
            VideoAsset::MasterPalette => self.palette = other.palette.clone(),
        }
    }
}

/// Unsaved asset states that stand in for what a cartridge's files hold,
/// so a run can show edits not written back.
#[derive(Debug, Clone, Copy, Default)]
pub struct VideoOverrides<'a> {
    source: Option<&'a CartridgeVideo>,
    assets: [bool; VideoAsset::ALL.len()],
}

impl<'a> VideoOverrides<'a> {
    /// Overrides `assets` with their state in `source`.
    pub fn new(source: &'a CartridgeVideo, assets: impl IntoIterator<Item = VideoAsset>) -> Self {
        let mut overrides = Self {
            source: Some(source),
            assets: [false; VideoAsset::ALL.len()],
        };
        for asset in assets {
            overrides.assets[asset.index()] = true;
        }
        overrides
    }

    /// Whether no asset is overridden.
    pub fn is_empty(&self) -> bool {
        self.source.is_none() || !self.assets.contains(&true)
    }

    /// Replaces the overridden assets of `video`.
    pub fn apply(&self, video: &mut CartridgeVideo) {
        let Some(source) = self.source else {
            return;
        };
        for asset in VideoAsset::ALL {
            if self.assets[asset.index()] {
                video.copy_asset(source, asset);
            }
        }
    }
}
