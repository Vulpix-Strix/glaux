//! The cartridge's maps, decoded for a running script to read.
//!
//! This is a read-only copy of the authoring data, not hardware state. Nothing
//! on the bus reaches it, and writing to VRAM never changes it. A script moves
//! regions of a map into a tilemap window with
//! [`TileWindow::transfer`](crate::tile_window::TileWindow::transfer).

use std::collections::HashMap;

use crate::tilemap::{BgLayer, TileLayer};

/// The three layers of a map, which always share one size.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MapData {
    layers: [TileLayer; 3],
}

impl MapData {
    /// Returns `None` when the layers differ in size.
    pub fn new(layers: [TileLayer; 3]) -> Option<Self> {
        let (width, height) = (layers[0].width(), layers[0].height());
        layers
            .iter()
            .all(|layer| layer.width() == width && layer.height() == height)
            .then_some(Self { layers })
    }

    pub fn width(&self) -> usize {
        self.layers[0].width()
    }

    pub fn height(&self) -> usize {
        self.layers[0].height()
    }

    pub fn layer(&self, layer: BgLayer) -> &TileLayer {
        &self.layers[layer.index()]
    }
}

/// The maps of a cartridge by id. The id is the map's directory name.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MapStore {
    maps: HashMap<String, MapData>,
}

impl MapStore {
    /// Replaces any earlier map with the same id.
    pub fn insert(&mut self, id: impl Into<String>, map: MapData) {
        self.maps.insert(id.into(), map);
    }

    pub fn get(&self, id: &str) -> Option<&MapData> {
        self.maps.get(id)
    }

    pub fn is_empty(&self) -> bool {
        self.maps.is_empty()
    }
}

#[cfg(test)]
mod tests;
