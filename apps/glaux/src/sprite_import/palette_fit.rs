//! Assigns the colors of a true-color spritesheet to a cartridge master
//! palette and the G1 object palette banks, without recoloring anything.
//!
//! Everything here is pure: callers decode images and read and write files.

use std::cmp::Reverse;
use std::collections::{BTreeSet, HashMap};
use std::ops::RangeInclusive;

use glaux_machine::obj_palette::{OBJ_PALETTE_BANKS, OBJ_PALETTE_COLORS, ObjBank, ObjPalettes};
use glaux_machine::palette::{PALETTE_LEN, Palette, Rgb};
use glaux_machine::sprite::{
    OBJ_GRID_COLUMNS, OBJ_SHEET_MAX_HEIGHT, OBJ_SHEET_MAX_WIDTH, OBJ_TILE_BYTES, OBJ_TILE_HEIGHT,
    OBJ_TILE_WIDTH, ObjTileAttributes, ObjTileBank, ObjTileEncodingError, encode_obj_tiles,
};
use serde::Deserialize;

/// First master-palette entry the importer allocates. Entries below it keep
/// the colors the cartridge already gives them. Entries from it up are
/// rebuilt by every import.
pub(crate) const CART_COLOR_START: usize = 32;

/// Opaque colors one object palette bank holds next to transparent index 0.
const BANK_COLORS: usize = OBJ_PALETTE_COLORS - 1;

/// Most opaque colors all object palette banks can show at once.
const MAX_SHEET_COLORS: usize = OBJ_PALETTE_BANKS * BANK_COLORS;

// Every color that can be shown must find a slot in the importer region.
const _: () = assert!(MAX_SHEET_COLORS <= PALETTE_LEN - CART_COLOR_START);

/// Why a sheet cannot be imported as it is. Messages speak to the person who
/// drew the sheet, in pixel and tile coordinates of the image.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub(crate) enum FitError {
    #[error(
        "{count} pixels are partly transparent, the first at ({x}, {y}) with alpha {alpha}; \
         sprite pixels must be fully opaque or fully transparent"
    )]
    SemiTransparent {
        x: usize,
        y: usize,
        alpha: u8,
        count: usize,
    },
    #[error("sprite sheet dimensions must be divisible by 8, got {width}x{height}")]
    InvalidDimensions { width: usize, height: usize },
    #[error(
        "sprite sheet is {width}x{height}; the G1 object-tile grid holds at most \
         {OBJ_SHEET_MAX_WIDTH}x{OBJ_SHEET_MAX_HEIGHT}"
    )]
    TooLarge { width: usize, height: usize },
    #[error(
        "sprite sheet uses {colors} colors; G1 shows at most {MAX_SHEET_COLORS} sprite colors \
         ({OBJ_PALETTE_BANKS} sprite palettes of {BANK_COLORS} colors)"
    )]
    TooManyColors { colors: usize },
    #[error(
        "tile ({tile_x}, {tile_y}), pixels ({x}, {y}) to ({}, {}), uses {colors} colors; \
         an 8x8 sprite tile can use at most {BANK_COLORS} plus transparency",
        x + OBJ_TILE_WIDTH - 1,
        y + OBJ_TILE_HEIGHT - 1
    )]
    TileTooColorful {
        tile_x: usize,
        tile_y: usize,
        x: usize,
        y: usize,
        colors: usize,
    },
    #[error(
        "sprite group \"{name}\" uses {colors} colors; one sprite palette holds {BANK_COLORS}. \
         Split the group or reduce its colors"
    )]
    GroupTooColorful { name: String, colors: usize },
    #[error(
        "the sheet needs more than {OBJ_PALETTE_BANKS} sprite palettes: {unit} ({colors} colors) \
         does not fit next to the colors already placed (palettes hold {} colors). \
         Reduce the colors of the most colorful sprites, or share more colors between them",
        bank_sizes.iter().map(usize::to_string).collect::<Vec<_>>().join(", ")
    )]
    TooManyPalettes {
        unit: String,
        colors: usize,
        bank_sizes: Vec<usize>,
    },
    #[error("a sprite group has an empty name")]
    EmptyGroupName,
    #[error("sprite group \"{0}\" is defined more than once")]
    DuplicateGroup(String),
    #[error("sprite group \"{0}\" covers no tiles")]
    EmptyGroup(String),
    #[error("sprite group \"{0}\" reaches outside the sheet")]
    GroupOutsideSheet(String),
    #[error("sprite groups \"{0}\" and \"{1}\" overlap")]
    GroupsOverlap(String, String),
    #[error("internal error: pixel ({x}, {y}) would not keep its color; nothing was written")]
    RoundTrip { x: usize, y: usize },
    #[error(transparent)]
    Encoding(#[from] ObjTileEncodingError),
}

/// A sheet's pixels in row-major order: `None` is transparent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Sheet {
    width: usize,
    height: usize,
    pixels: Vec<Option<Rgb>>,
}

impl Sheet {
    /// Classifies RGBA8 pixels: alpha 0 is transparent, alpha 255 opaque,
    /// and any other alpha an error. Opaque pixels of color `key` are
    /// transparent too.
    ///
    /// `rgba` must hold exactly `width * height` pixels.
    pub(crate) fn from_rgba(
        width: usize,
        height: usize,
        rgba: &[u8],
        key: Option<Rgb>,
    ) -> Result<Self, FitError> {
        let (texels, _) = rgba.as_chunks::<4>();
        debug_assert_eq!(texels.len(), width * height);
        let mut first_partial = None;
        let mut partial = 0;
        let mut pixels = Vec::with_capacity(texels.len());
        for (offset, &[r, g, b, alpha]) in texels.iter().enumerate() {
            let color = Rgb { r, g, b };
            pixels.push(match alpha {
                0 => None,
                255 if Some(color) == key => None,
                255 => Some(color),
                _ => {
                    first_partial.get_or_insert((offset, alpha));
                    partial += 1;
                    None
                }
            });
        }
        if let Some((offset, alpha)) = first_partial {
            return Err(FitError::SemiTransparent {
                x: offset % width,
                y: offset / width,
                alpha,
                count: partial,
            });
        }
        Ok(Self {
            width,
            height,
            pixels,
        })
    }

    fn pixel(&self, x: usize, y: usize) -> Option<Rgb> {
        self.pixels[y * self.width + x]
    }
}

/// A named rectangle of sheet tiles whose tiles must share one palette, so
/// that drawing it with an explicit palette recolors all of it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SpriteGroup {
    pub(crate) name: String,
    /// Top-left tile, in sheet tile coordinates.
    pub(crate) tile: (usize, usize),
    /// Width and height in tiles.
    pub(crate) size: (usize, usize),
}

/// What [`fit`] works from.
pub(crate) struct FitInput<'a> {
    pub(crate) sheet: &'a Sheet,
    pub(crate) groups: &'a [SpriteGroup],
    /// The cartridge's current master palette. Entries below
    /// [`CART_COLOR_START`] are kept. The rest are rebuilt.
    pub(crate) current: &'a Palette,
    /// The System ROM palette, which the rebuilt entries start from.
    pub(crate) rom_palette: &'a Palette,
    /// The System ROM object palette banks, kept for banks the sheet does
    /// not use.
    pub(crate) rom_obj_palettes: &'a ObjPalettes,
}

/// The native assets a sheet imports to, and facts about them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Fitted {
    /// `sprites.bin`: whole grid rows of 4 bpp tiles.
    pub(crate) tiles: Vec<u8>,
    /// `objattr.bin`: one attribute per tile of `tiles`.
    pub(crate) attributes: Vec<u8>,
    pub(crate) obj_palettes: ObjPalettes,
    pub(crate) palette: Palette,
    pub(crate) summary: FitSummary,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FitSummary {
    pub(crate) source_tiles: usize,
    /// The sheet's size in tiles.
    pub(crate) tiles_wide: usize,
    pub(crate) tiles_high: usize,
    /// Distinct opaque colors in the sheet.
    pub(crate) colors: usize,
    /// Colors found among the kept master entries.
    pub(crate) reused: usize,
    /// Master entries the other colors were written to.
    pub(crate) allocated: Option<RangeInclusive<usize>>,
    pub(crate) banks_used: usize,
    /// Each group's bank, in sidecar order.
    pub(crate) group_banks: Vec<(String, ObjBank)>,
}

/// Tiles that must share one bank: a sprite group or a lone tile.
struct Unit {
    label: String,
    first_tile: usize,
    tiles: Vec<usize>,
    colors: BTreeSet<u8>,
}

/// Assigns every opaque color of the sheet a master entry and every tile a
/// bank, and encodes the result.
///
/// Colors are matched exactly. A color already among the kept master
/// entries reuses the lowest such entry, and the others take consecutive
/// entries from [`CART_COLOR_START`] in the order they first appear, row by
/// row.
///
/// Banks are packed from units, largest color set first. Each unit joins
/// the bank it adds the fewest colors to without exceeding 15, with ties
/// going to the lowest bank, or opens a new bank. Within a bank, colors take
/// local indices from 1 in the order they join it.
pub(crate) fn fit(input: &FitInput) -> Result<Fitted, FitError> {
    let sheet = input.sheet;
    let (width, height) = (sheet.width, sheet.height);
    if !width.is_multiple_of(OBJ_TILE_WIDTH) || !height.is_multiple_of(OBJ_TILE_HEIGHT) {
        return Err(FitError::InvalidDimensions { width, height });
    }
    if width > OBJ_SHEET_MAX_WIDTH || height > OBJ_SHEET_MAX_HEIGHT {
        return Err(FitError::TooLarge { width, height });
    }
    let tiles_x = width / OBJ_TILE_WIDTH;
    let tiles_y = height / OBJ_TILE_HEIGHT;
    let group_of = locate_groups(input.groups, tiles_x, tiles_y)?;

    let mut distinct = Vec::new();
    let mut seen = BTreeSet::new();
    for color in sheet.pixels.iter().flatten() {
        if seen.insert((color.r, color.g, color.b)) {
            distinct.push(*color);
        }
    }
    if distinct.len() > MAX_SHEET_COLORS {
        return Err(FitError::TooManyColors {
            colors: distinct.len(),
        });
    }

    let mut entries = *input.rom_palette.entries();
    entries[..CART_COLOR_START].copy_from_slice(&input.current.entries()[..CART_COLOR_START]);
    let mut master_of = HashMap::new();
    let mut next = CART_COLOR_START;
    for &color in &distinct {
        let master = match entries[..CART_COLOR_START]
            .iter()
            .position(|&entry| entry == color)
        {
            Some(kept) => kept,
            None => {
                entries[next] = color;
                next += 1;
                next - 1
            }
        };
        // Every master index is below PALETTE_LEN and fits in u8.
        master_of.insert(color, master as u8);
    }
    let allocated = (next > CART_COLOR_START).then(|| CART_COLOR_START..=next - 1);

    let tile_index = |tx: usize, ty: usize| ty * tiles_x + tx;
    let mut tile_colors = vec![BTreeSet::new(); tiles_x * tiles_y];
    for ty in 0..tiles_y {
        for tx in 0..tiles_x {
            let colors = &mut tile_colors[tile_index(tx, ty)];
            for y in ty * OBJ_TILE_HEIGHT..(ty + 1) * OBJ_TILE_HEIGHT {
                for x in tx * OBJ_TILE_WIDTH..(tx + 1) * OBJ_TILE_WIDTH {
                    if let Some(color) = sheet.pixel(x, y) {
                        colors.insert(master_of[&color]);
                    }
                }
            }
            if colors.len() > BANK_COLORS {
                return Err(FitError::TileTooColorful {
                    tile_x: tx,
                    tile_y: ty,
                    x: tx * OBJ_TILE_WIDTH,
                    y: ty * OBJ_TILE_HEIGHT,
                    colors: colors.len(),
                });
            }
        }
    }

    let mut units: Vec<Unit> = input
        .groups
        .iter()
        .map(|group| Unit {
            label: format!("group \"{}\"", group.name),
            first_tile: tile_index(group.tile.0, group.tile.1),
            tiles: Vec::new(),
            colors: BTreeSet::new(),
        })
        .collect();
    let mut tile_unit = vec![None; tiles_x * tiles_y];
    for ty in 0..tiles_y {
        for tx in 0..tiles_x {
            let tile = tile_index(tx, ty);
            let unit = match group_of[tile] {
                Some(group) => group,
                None if tile_colors[tile].is_empty() => continue,
                None => {
                    units.push(Unit {
                        label: format!("tile ({tx}, {ty})"),
                        first_tile: tile,
                        tiles: Vec::new(),
                        colors: BTreeSet::new(),
                    });
                    units.len() - 1
                }
            };
            units[unit].tiles.push(tile);
            units[unit].colors.extend(&tile_colors[tile]);
            tile_unit[tile] = Some(unit);
        }
    }
    for (group, unit) in input.groups.iter().zip(&units) {
        if unit.colors.len() > BANK_COLORS {
            return Err(FitError::GroupTooColorful {
                name: group.name.clone(),
                colors: unit.colors.len(),
            });
        }
    }

    let mut order: Vec<usize> = (0..units.len()).collect();
    order.sort_by_key(|&unit| (Reverse(units[unit].colors.len()), units[unit].first_tile));
    let mut banks: Vec<Vec<u8>> = Vec::new();
    let mut unit_bank = vec![0; units.len()];
    for unit in order {
        let colors = &units[unit].colors;
        if colors.is_empty() {
            continue;
        }
        let best = banks
            .iter()
            .enumerate()
            .map(|(index, bank)| {
                let extra = colors.iter().filter(|c| !bank.contains(c)).count();
                (extra, index)
            })
            .filter(|&(extra, index)| banks[index].len() + extra <= BANK_COLORS)
            .min();
        let bank = match best {
            Some((_, index)) => index,
            None if banks.len() < OBJ_PALETTE_BANKS => {
                banks.push(Vec::new());
                banks.len() - 1
            }
            None => {
                return Err(FitError::TooManyPalettes {
                    unit: units[unit].label.clone(),
                    colors: colors.len(),
                    bank_sizes: banks.iter().map(Vec::len).collect(),
                });
            }
        };
        for &color in colors {
            if !banks[bank].contains(&color) {
                banks[bank].push(color);
            }
        }
        unit_bank[unit] = bank;
    }

    let tile_bank = |tile: usize| tile_unit[tile].map_or(0, |unit| unit_bank[unit]);
    // local_of[bank][master] is the local index of `master` in `bank`.
    let mut local_of = vec![[0u8; PALETTE_LEN]; banks.len()];
    for (bank, colors) in banks.iter().enumerate() {
        for (local, &master) in colors.iter().enumerate() {
            // A bank holds at most 15 colors, so locals run from 1 to 15.
            local_of[bank][usize::from(master)] = local as u8 + 1;
        }
    }
    let mut locals = vec![0; width * height];
    for y in 0..height {
        for x in 0..width {
            if let Some(color) = sheet.pixel(x, y) {
                let bank = tile_bank(tile_index(x / OBJ_TILE_WIDTH, y / OBJ_TILE_HEIGHT));
                locals[y * width + x] = local_of[bank][usize::from(master_of[&color])];
            }
        }
    }
    let tiles = encode_obj_tiles(width, height, &locals)?;

    let mut attributes = vec![0; tiles.len() / OBJ_TILE_BYTES];
    for ty in 0..tiles_y {
        for tx in 0..tiles_x {
            // Banks number at most OBJ_PALETTE_BANKS, so they fit the
            // attribute's bank bits.
            attributes[ty * OBJ_GRID_COLUMNS + tx] = tile_bank(tile_index(tx, ty)) as u8;
        }
    }
    let mut obp = *input.rom_obj_palettes.as_bytes();
    for (bank, colors) in banks.iter().enumerate() {
        let row = &mut obp[bank * OBJ_PALETTE_COLORS..(bank + 1) * OBJ_PALETTE_COLORS];
        row.fill(0);
        row[1..=colors.len()].copy_from_slice(colors);
    }
    let obj_palettes = ObjPalettes::from_bytes(&obp)
        .expect("entry 0 of every bank stays 0 in ROM and fitted banks");
    let palette = Palette::new(entries);

    let fitted = Fitted {
        tiles,
        attributes,
        obj_palettes,
        palette,
        summary: FitSummary {
            source_tiles: tiles_x * tiles_y,
            tiles_wide: tiles_x,
            tiles_high: tiles_y,
            colors: distinct.len(),
            reused: distinct.len() - allocated.as_ref().map_or(0, |range| range.clone().count()),
            allocated,
            banks_used: banks.len(),
            group_banks: input
                .groups
                .iter()
                .enumerate()
                .map(|(unit, group)| {
                    let bank = ObjBank::new(unit_bank[unit] as u8).unwrap_or(ObjBank::ZERO);
                    (group.name.clone(), bank)
                })
                .collect(),
        },
    };
    verify(sheet, &fitted)?;
    Ok(fitted)
}

/// Maps every sheet tile to the group covering it, after checking that the
/// groups are named, distinct, inside the sheet and disjoint.
fn locate_groups(
    groups: &[SpriteGroup],
    tiles_x: usize,
    tiles_y: usize,
) -> Result<Vec<Option<usize>>, FitError> {
    let mut group_of = vec![None; tiles_x * tiles_y];
    let mut names = BTreeSet::new();
    for (index, group) in groups.iter().enumerate() {
        if group.name.trim().is_empty() {
            return Err(FitError::EmptyGroupName);
        }
        if !names.insert(group.name.as_str()) {
            return Err(FitError::DuplicateGroup(group.name.clone()));
        }
        let ((x, y), (w, h)) = (group.tile, group.size);
        if w == 0 || h == 0 {
            return Err(FitError::EmptyGroup(group.name.clone()));
        }
        let inside = x.checked_add(w).is_some_and(|right| right <= tiles_x)
            && y.checked_add(h).is_some_and(|bottom| bottom <= tiles_y);
        if !inside {
            return Err(FitError::GroupOutsideSheet(group.name.clone()));
        }
        for ty in y..y + h {
            for tx in x..x + w {
                let slot = &mut group_of[ty * tiles_x + tx];
                if let Some(other) = *slot {
                    let other: &SpriteGroup = &groups[other];
                    return Err(FitError::GroupsOverlap(
                        other.name.clone(),
                        group.name.clone(),
                    ));
                }
                *slot = Some(index);
            }
        }
    }
    Ok(group_of)
}

/// Decodes `fitted` through the machine's own representations and checks
/// that every pixel shows the sheet's color.
fn verify(sheet: &Sheet, fitted: &Fitted) -> Result<(), FitError> {
    let mismatch = |x, y| FitError::RoundTrip { x, y };
    let tiles = ObjTileBank::from_bytes(&fitted.tiles).map_err(|_| mismatch(0, 0))?;
    let attributes =
        ObjTileAttributes::from_bytes(&fitted.attributes).map_err(|_| mismatch(0, 0))?;
    for y in 0..sheet.height {
        for x in 0..sheet.width {
            let tile = (y / OBJ_TILE_HEIGHT) * OBJ_GRID_COLUMNS + x / OBJ_TILE_WIDTH;
            let shown = tiles
                .pixel(tile, x % OBJ_TILE_WIDTH, y % OBJ_TILE_HEIGHT)
                .and_then(|local| fitted.obj_palettes.resolve(attributes.bank(tile), local))
                .map(|master| fitted.palette.get(master));
            if shown != sheet.pixel(x, y) {
                return Err(mismatch(x, y));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
