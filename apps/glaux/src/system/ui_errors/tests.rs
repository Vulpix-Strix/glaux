use glaux_machine::bg::{BgPalettesError, BgTileBankError};
use glaux_machine::sprite::{ObjTileAttributesError, ObjTileBankError};
use pretty_assertions::assert_eq;

use super::{cartridge, fs, map::map, map::map_save, video::video_asset};
use crate::cartridge::CartridgeError;
use crate::cartridge::map::{MapError, MapFile, MapId};
use crate::cartridge_fs::FsError;
use crate::system::map_document::{Conflict, DiskFile, SaveFailure};
use crate::system::sprite_document::ConflictKind;
use crate::system::video::VideoAssetError;

#[test]
fn visible_errors_keep_their_english_and_dynamic_values() {
    assert_eq!(
        fs(&FsError::NotFound("/my game".into())),
        "No such file or directory: /my game"
    );
    assert_eq!(
        cartridge(&CartridgeError::RequiresGlaux {
            required: semver::Version::new(1, 2, 0),
            running: semver::Version::new(0, 1, 0),
        }),
        "Cartridge requires Glaux 1.2.0 (this is 0.1.0)"
    );
    assert_eq!(
        video_asset(&VideoAssetError::ObjTiles(ObjTileBankError::Misaligned {
            length: 3
        })),
        "Invalid sprites.bin: object tile data length must be a multiple of 32 bytes, got 3"
    );
    assert_eq!(
        video_asset(&VideoAssetError::ObjAttributes(
            ObjTileAttributesError::ReservedBits {
                tile: 12,
                value: 0xaf
            }
        )),
        "Invalid objattr.bin: object tile 12 attribute 0xaf sets reserved bits; only bits 0-2 are defined"
    );
    assert_eq!(
        video_asset(&VideoAssetError::BgTiles(BgTileBankError::TooLarge {
            length: 40000
        })),
        "Invalid bgtiles.bin: background tile data is too large: got 40000 bytes, maximum is 32768"
    );
    assert_eq!(
        video_asset(&VideoAssetError::BgPalettes(BgPalettesError::Length {
            length: 10
        })),
        "Invalid bgp.bin: background palette data must be exactly 64 bytes, got 10"
    );
}

#[test]
fn map_errors_keep_their_english_and_dynamic_values() {
    let id = MapId::parse("overworld").unwrap();
    assert_eq!(
        map(&MapError::InvalidSize {
            width: 300,
            height: 2
        }),
        "map.ron: size 300x2 is outside 1 to 256 in each direction"
    );
    assert_eq!(
        map(&MapError::MissingMeta(id.clone())),
        "Map overworld has no map.ron"
    );
    assert_eq!(
        map(&MapError::NotAFile {
            id,
            file: MapFile::Layer(glaux_machine::tilemap::BgLayer::Bg1)
        }),
        "bg1.bin of map overworld is not a file"
    );
    assert_eq!(
        map(&MapError::Fs(FsError::ReadOnly("/maps/a/map.ron".into()))),
        fs(&FsError::ReadOnly("/maps/a/map.ron".into()))
    );
}

#[test]
fn a_map_save_conflict_names_the_files() {
    let conflict = |file| Conflict {
        kind: ConflictKind::Modified,
        disk: DiskFile { file, bytes: None },
    };
    assert_eq!(
        map_save(&SaveFailure::Conflict(vec![
            conflict(MapFile::Layer(glaux_machine::tilemap::BgLayer::Bg0)),
            conflict(MapFile::Meta),
        ])),
        "bg0.bin, map.ron changed outside Glaux."
    );
}
