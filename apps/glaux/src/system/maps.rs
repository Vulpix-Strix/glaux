//! The maps a run can read.
//!
//! They come from the cartridge's files, except for the map open in the map
//! editor when it has unsaved edits and the run starts from the shell.

use std::sync::Arc;

use glaux_machine::map_store::{MapData, MapStore};
use glaux_machine::tilemap::BgLayer;

use super::map_document::MapDocument;
use super::running::StartError;
use super::video::VideoOverrides;
use crate::cartridge::{CartridgeError, LoadedCartridge};

/// The unsaved state of the map in the editor, to stand in for its files.
#[derive(Clone, Copy, Default)]
pub struct MapOverrides<'a> {
    document: Option<&'a MapDocument>,
}

impl<'a> MapOverrides<'a> {
    /// Overrides the map of `document` when it has unsaved edits.
    pub fn new(document: &'a MapDocument) -> Self {
        Self {
            document: document.is_modified().then_some(document),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.document.is_none()
    }
}

/// What a run uses in place of files that have unsaved edits.
#[derive(Default)]
pub struct RunOverrides<'a> {
    pub video: VideoOverrides<'a>,
    pub maps: MapOverrides<'a>,
}

impl<'a> From<VideoOverrides<'a>> for RunOverrides<'a> {
    fn from(video: VideoOverrides<'a>) -> Self {
        Self {
            video,
            maps: MapOverrides::default(),
        }
    }
}

/// Decodes every map of `cartridge`. A map with unsaved edits is taken from
/// its document instead of its files, which are then not read.
pub fn load(
    cartridge: &LoadedCartridge,
    overrides: MapOverrides<'_>,
) -> Result<Arc<MapStore>, StartError> {
    let mut store = MapStore::default();
    let ids = cartridge
        .list_maps()
        .map_err(|err| StartError::Cartridge(CartridgeError::Fs(err)))?;
    for id in ids {
        if overrides
            .document
            .is_some_and(|document| document.id() == &id)
        {
            continue;
        }
        let decoded = cartridge
            .read_map(&id)
            .and_then(|files| files.decode())
            .map_err(|error| StartError::Maps {
                id: id.clone(),
                error,
            })?;
        store.insert(id.to_string(), decoded);
    }
    if let Some(document) = overrides.document {
        let layers = BgLayer::ALL.map(|layer| document.layer(layer).clone());
        let data = MapData::new(layers).expect("the layers of a document share its size");
        store.insert(document.id().to_string(), data);
    }
    Ok(Arc::new(store))
}
