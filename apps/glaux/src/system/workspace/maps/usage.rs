//! How much the cartridge's maps use each background tile and palette bank.

use glaux_machine::tilemap::{BG_PALETTE_BANKS, BgBank, BgLayer};

use super::pattern::TILES;
use crate::system::map_document::MapDocument;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MapUsage {
    tiles: Vec<u32>,
    banks: [u32; BG_PALETTE_BANKS],
}

impl MapUsage {
    pub fn of(document: &MapDocument) -> Self {
        let mut usage = Self {
            tiles: vec![0; TILES],
            banks: [0; BG_PALETTE_BANKS],
        };
        for layer in BgLayer::ALL.map(|layer| document.layer(layer)) {
            let entries = (0..layer.cell_count()).filter_map(|index| layer.get_index(index));
            for entry in entries.filter(|entry| entry.tile() != 0) {
                usage.tiles[usize::from(entry.tile())] += 1;
                usage.banks[usize::from(entry.bank().get())] += 1;
            }
        }
        usage
    }

    fn tile(&self, tile: u16) -> usize {
        self.tiles.get(usize::from(tile)).map_or(0, |&n| n as usize)
    }

    fn bank(&self, bank: BgBank) -> usize {
        self.banks[usize::from(bank.get())] as usize
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct Uses {
    pub(super) cells: usize,
    pub(super) maps: usize,
}

/// The open map as it is edited, with the cartridge's other maps as saved.
pub(super) struct Census<'a> {
    pub(super) open: MapUsage,
    pub(super) others: &'a [MapUsage],
}

impl Census<'_> {
    fn count(&self, cells: impl Fn(&MapUsage) -> usize) -> Uses {
        std::iter::once(&self.open)
            .chain(self.others)
            .map(cells)
            .filter(|&cells| cells > 0)
            .fold(Uses::default(), |uses, cells| Uses {
                cells: uses.cells + cells,
                maps: uses.maps + 1,
            })
    }

    pub(super) fn tile(&self, tile: u16) -> Uses {
        self.count(|usage| usage.tile(tile))
    }

    pub(super) fn bank(&self, bank: BgBank) -> Uses {
        self.count(|usage| usage.bank(bank))
    }

    pub(super) fn banks(&self, banks: &[BgBank]) -> Uses {
        self.count(|usage| banks.iter().map(|&bank| usage.bank(bank)).sum())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::system::workspace::maps::fixture::document;
    use glaux_machine::tilemap::{BgLayer, TileEntry};
    use pretty_assertions::assert_eq;

    fn map(cells: &[(BgLayer, usize, u16, u8)]) -> MapUsage {
        let mut document = document(4, 4);
        for &(layer, x, tile, bank) in cells {
            let entry = TileEntry::new(tile, false, false, false, BgBank::new(bank).unwrap());
            document.set_cell(layer, x, 0, entry.unwrap());
        }
        MapUsage::of(&document)
    }

    #[test]
    fn uses_add_up_over_layers_and_maps_and_skip_maps_without_any() {
        let census = Census {
            open: map(&[(BgLayer::Bg0, 0, 7, 1), (BgLayer::Bg2, 0, 7, 2)]),
            others: &[
                map(&[(BgLayer::Bg1, 3, 7, 1)]),
                map(&[(BgLayer::Bg1, 1, 9, 3)]),
            ],
        };
        assert_eq!(census.tile(7), Uses { cells: 3, maps: 2 });
        assert_eq!(census.tile(8), Uses::default());
        assert_eq!(
            census.bank(BgBank::new(1).unwrap()),
            Uses { cells: 2, maps: 2 }
        );
        let banks = [BgBank::new(2).unwrap(), BgBank::new(3).unwrap()];
        assert_eq!(census.banks(&banks), Uses { cells: 2, maps: 2 });
    }
}
