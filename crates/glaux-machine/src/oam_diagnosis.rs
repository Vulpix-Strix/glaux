//! Explains why an object does not draw as its entry suggests.
//!
//! A diagnosis is data, produced when asked for. Nothing here logs, and
//! composing a frame never runs it. It reads only the entry and the rules of
//! the object-tile grid, the same ones the compositor and `spr()` use, so it
//! cannot disagree with what is drawn.
//!
//! The position of an object does not matter. One that is off screen or
//! clipped is configured correctly.

use crate::oam::{OAM_ENTRIES, Oam, OamEntry, SIZE_MASK, TILE_MASK};
use crate::sprite::{GridFault, obj_grid_tile_checked};

/// What is wrong, if anything, with the configuration of one object.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Diagnosis {
    /// The object is switched off, which is not a mistake.
    Disabled,
    /// Every tile of the object can be addressed.
    Valid,
    /// The object is enabled with the reserved size code, so none of it is
    /// drawn.
    ReservedSize,
    /// Some tiles cannot be addressed and the others draw normally. Faults
    /// come in row order, then column order.
    PartiallyInvalid(Vec<TileFault>),
}

impl Diagnosis {
    /// Whether the object is enabled and does not draw as configured.
    pub fn is_problem(&self) -> bool {
        matches!(self, Self::ReservedSize | Self::PartiallyInvalid(_))
    }
}

/// A tile an object cannot draw, named by its offset from the base tile.
///
/// Flips only change where a tile lands on screen, so the offset is the same
/// with or without them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TileFault {
    pub column: usize,
    pub row: usize,
    pub cause: GridFault,
}

impl OamEntry {
    /// Diagnoses the entry as OAM would store it.
    ///
    /// Fields outside their range are reduced to their width first, as when
    /// the entry is written to the table. A tile of `1024 + 5` is tile 5 and
    /// a size of `9` is size 1. This differs from [`tiles`](Self::tiles),
    /// which does not reduce the size.
    pub fn diagnose(&self) -> Diagnosis {
        if !self.enabled {
            return Diagnosis::Disabled;
        }
        let stored = OamEntry {
            tile: self.tile & TILE_MASK,
            size: self.size & SIZE_MASK,
            ..*self
        };
        let Some((columns, rows)) = stored.tiles() else {
            return Diagnosis::ReservedSize;
        };
        let first = usize::from(stored.tile);
        let mut faults = Vec::new();
        for row in 0..rows {
            for column in 0..columns {
                if let Err(cause) = obj_grid_tile_checked(first, column, row) {
                    faults.push(TileFault { column, row, cause });
                }
            }
        }
        if faults.is_empty() {
            Diagnosis::Valid
        } else {
            Diagnosis::PartiallyInvalid(faults)
        }
    }
}

impl Oam {
    /// The diagnosis of the object at `index`, or `None` past the last one.
    pub fn diagnose(&self, index: usize) -> Option<Diagnosis> {
        self.entry(index).map(|entry| entry.diagnose())
    }

    /// The diagnosis of every object, in index order.
    pub fn diagnoses(&self) -> impl Iterator<Item = (usize, Diagnosis)> + '_ {
        (0..OAM_ENTRIES).filter_map(|index| Some((index, self.diagnose(index)?)))
    }
}

#[cfg(test)]
mod tests;
