//! Converts a PNG spritesheet into native G1 object tiles, tile attributes,
//! object palettes and master-palette colors, keeping every color exactly
//! as drawn.
//!
//! This is the conversion only: reading the PNG and its sprite-group
//! sidecar, and fitting it to a palette. Where the result goes is up to the
//! caller. `glaux import sprites` publishes it into a cartridge's files,
//! and the sprite editor makes it an edit of its document.

mod decode;
mod palette_fit;

use std::fs::{self, File};
use std::io::BufReader;
use std::path::{Path, PathBuf};

use anyhow::{Context as _, bail};
use glaux_machine::obj_palette::ObjPalettes;
use glaux_machine::palette::{Palette, Rgb};
use serde::Deserialize;

pub(crate) use decode::{RgbaImage, decode_png};
pub(crate) use palette_fit::{CART_COLOR_START, FitError, FitSummary, Fitted, SpriteGroup};
use palette_fit::{FitInput, fit};

/// The contents of a sprite-group sidecar.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct GroupsFile {
    groups: Vec<SpriteGroup>,
}

/// A decoded PNG spritesheet with the sprite groups that go with it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SheetSource {
    pub(crate) image: RgbaImage,
    pub(crate) groups: Vec<SpriteGroup>,
    /// The sidecar the groups were read from, if any.
    pub(crate) groups_path: Option<PathBuf>,
}

impl SheetSource {
    /// Reads and decodes the PNG at `png`, and the sprite groups in
    /// `groups`, which must exist, or else in `<PNG name>.sprites.ron` next
    /// to the PNG, when present.
    pub(crate) fn read(png: &Path, groups: Option<&Path>) -> anyhow::Result<Self> {
        let file = File::open(png)
            .with_context(|| format!("failed to open input PNG {}", png.display()))?;
        let image = decode_png(BufReader::new(file))
            .with_context(|| format!("failed to read PNG {}", png.display()))?;
        let groups_path = groups_path(png, groups)?;
        let groups = match &groups_path {
            Some(path) => read_groups(path)?,
            None => Vec::new(),
        };
        Ok(Self {
            image,
            groups,
            groups_path,
        })
    }

    /// Fits the sheet, with opaque pixels of color `key` transparent, to a
    /// cartridge whose master palette is `current`. The master entries from
    /// [`CART_COLOR_START`] up and the banks the sheet does not use start
    /// from the System ROM's `rom_palette` and `rom_obj_palettes`.
    pub(crate) fn fit(
        &self,
        key: Option<Rgb>,
        current: &Palette,
        rom_palette: &Palette,
        rom_obj_palettes: &ObjPalettes,
    ) -> Result<Fitted, FitError> {
        fit(&FitInput {
            sheet: &self.image.sheet(key)?,
            groups: &self.groups,
            current,
            rom_palette,
            rom_obj_palettes,
        })
    }
}

/// The sidecar to read: `explicit`, which must exist, or the default one
/// next to `png`, when present.
fn groups_path(png: &Path, explicit: Option<&Path>) -> anyhow::Result<Option<PathBuf>> {
    if let Some(path) = explicit {
        if !path.is_file() {
            bail!("sprite group file {} does not exist", path.display());
        }
        return Ok(Some(path.to_path_buf()));
    }
    let stem = png.file_stem().context("the input path has no file name")?;
    let mut name = stem.to_os_string();
    name.push(".sprites.ron");
    let path = png.with_file_name(name);
    Ok(path.is_file().then_some(path))
}

fn read_groups(path: &Path) -> anyhow::Result<Vec<SpriteGroup>> {
    let text = fs::read_to_string(path)
        .with_context(|| format!("failed to read sprite groups {}", path.display()))?;
    let file: GroupsFile = ron::from_str(&text)
        .with_context(|| format!("invalid sprite groups {}", path.display()))?;
    Ok(file.groups)
}

#[cfg(test)]
pub(crate) mod fixture;

#[cfg(test)]
mod tests;
