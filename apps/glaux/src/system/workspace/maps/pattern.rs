//! Pure operations on patterns, clips and tile attributes.
use glaux_machine::tilemap::{BgBank, TileEntry, TileLayer};

use super::frame::STRIP_COLUMNS;
use super::tools::Selection;

pub(super) const TILES: usize = 1024;

/// Tile 0 and tiles past the sheet's right edge are skipped.
pub(super) fn stamp(anchor: TileEntry, size: (usize, usize)) -> Vec<((usize, usize), TileEntry)> {
    let first = usize::from(anchor.tile());
    let column = first % STRIP_COLUMNS;
    let mut cells = Vec::with_capacity(size.0 * size.1);
    for dy in 0..size.1 {
        for dx in 0..size.0 {
            let tile = first + dy * STRIP_COLUMNS + dx;
            if column + dx >= STRIP_COLUMNS || tile >= TILES || tile == 0 {
                continue;
            }
            let entry = u16::try_from(tile).ok().and_then(|tile| {
                TileEntry::new(
                    tile,
                    anchor.flip_x(),
                    anchor.flip_y(),
                    anchor.priority(),
                    anchor.bank(),
                )
            });
            cells.extend(entry.map(|entry| ((dx, dy), entry)));
        }
    }
    cells
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Clip {
    pub(super) width: usize,
    pub(super) height: usize,
    entries: Vec<TileEntry>,
}

impl Clip {
    pub(super) fn copy(layer: &TileLayer, selection: Selection) -> Self {
        let (width, height) = (selection.width(), selection.height());
        let mut entries = Vec::with_capacity(width * height);
        for y in selection.top..=selection.bottom {
            for x in selection.left..=selection.right {
                entries.push(layer.get(x, y).unwrap_or(TileEntry::EMPTY));
            }
        }
        Self {
            width,
            height,
            entries,
        }
    }

    pub(super) fn cells(&self) -> impl Iterator<Item = ((usize, usize), TileEntry)> + '_ {
        self.entries
            .iter()
            .enumerate()
            .map(|(index, &entry)| ((index % self.width, index / self.width), entry))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Attr {
    Bank(BgBank),
    FlipX(bool),
    FlipY(bool),
    Priority(bool),
}

impl Attr {
    pub(super) fn apply(self, entry: TileEntry) -> TileEntry {
        let (mut flip_x, mut flip_y) = (entry.flip_x(), entry.flip_y());
        let (mut priority, mut bank) = (entry.priority(), entry.bank());
        match self {
            Self::Bank(value) => bank = value,
            Self::FlipX(value) => flip_x = value,
            Self::FlipY(value) => flip_y = value,
            Self::Priority(value) => priority = value,
        }
        TileEntry::new(entry.tile(), flip_x, flip_y, priority, bank).unwrap_or(entry)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Shared<T> {
    Same(T),
    Mixed,
}

impl<T: Copy + PartialEq> Shared<T> {
    fn of(mut values: impl Iterator<Item = T>) -> Option<Self> {
        let first = values.next()?;
        Some(if values.all(|value| value == first) {
            Self::Same(first)
        } else {
            Self::Mixed
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Summary {
    pub(super) bank: Shared<BgBank>,
    pub(super) flip_x: Shared<bool>,
    pub(super) flip_y: Shared<bool>,
    pub(super) priority: Shared<bool>,
    pub(super) cells: usize,
}

pub(super) fn summarize(layer: &TileLayer, selection: Selection) -> Option<Summary> {
    let entries = || tiled(layer, selection);
    Some(Summary {
        bank: Shared::of(entries().map(TileEntry::bank))?,
        flip_x: Shared::of(entries().map(TileEntry::flip_x))?,
        flip_y: Shared::of(entries().map(TileEntry::flip_y))?,
        priority: Shared::of(entries().map(TileEntry::priority))?,
        cells: entries().count(),
    })
}

fn tiled(layer: &TileLayer, selection: Selection) -> impl Iterator<Item = TileEntry> + '_ {
    (selection.top..=selection.bottom)
        .flat_map(move |y| (selection.left..=selection.right).map(move |x| (x, y)))
        .filter_map(|(x, y)| layer.get(x, y))
        .filter(|entry| entry.tile() != 0)
}

pub(super) fn uses(layer: &TileLayer, tile: u16) -> usize {
    (0..layer.cell_count())
        .filter_map(|index| layer.get_index(index))
        .filter(|entry| entry.tile() == tile)
        .count()
}

pub(super) fn bounds_of(layer: &TileLayer, tile: u16) -> Option<Selection> {
    let mut found: Option<Selection> = None;
    for y in 0..layer.height() {
        for x in 0..layer.width() {
            if layer.get(x, y).is_some_and(|entry| entry.tile() == tile) {
                found = Some(match found {
                    None => Selection {
                        left: x,
                        top: y,
                        right: x,
                        bottom: y,
                    },
                    Some(s) => Selection {
                        left: s.left.min(x),
                        top: s.top.min(y),
                        right: s.right.max(x),
                        bottom: s.bottom.max(y),
                    },
                });
            }
        }
    }
    found
}

#[cfg(test)]
mod tests;
