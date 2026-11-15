use glaux_machine::tilemap::BgBank;
use pretty_assertions::assert_eq;

use super::*;

fn defaults() -> CartridgeVideoDefaults {
    CartridgeVideoDefaults::from_rom(&crate::rom::load().unwrap())
}

#[test]
fn missing_background_files_fall_back_to_blank_tiles_and_linear_palettes() {
    let video = CartridgeVideo::decode(&VideoFiles::default(), &defaults()).unwrap();
    assert_eq!(video.bg, BgVram::default());
    assert_eq!(video.bg.palettes().resolve(BgBank::ZERO, 3), Some(3));
}

#[test]
fn background_files_decode_as_their_own_assets() {
    let mut palettes = [0; 64];
    palettes[16 + 1] = 99;
    let files = VideoFiles {
        bg_tiles: Some(vec![0x12; 32]),
        bg_palettes: Some(palettes.to_vec()),
        ..VideoFiles::default()
    };
    let video = CartridgeVideo::decode(&files, &defaults()).unwrap();
    assert_eq!(video.bg.tiles().pixel(0, 0, 0), Some(1));
    assert_eq!(
        video.bg.palettes().resolve(BgBank::new(1).unwrap(), 1),
        Some(99)
    );
    let plain = CartridgeVideo::decode(&VideoFiles::default(), &defaults()).unwrap();
    assert_eq!(video.obj, plain.obj);
}

#[test]
fn the_hardware_takes_over_every_part_of_the_video_state() {
    let files = VideoFiles {
        bg_tiles: Some(vec![0x34; 64]),
        ..VideoFiles::default()
    };
    let video = CartridgeVideo::decode(&files, &defaults()).unwrap();
    let (palette, obj, bg) = (video.palette.clone(), video.obj.clone(), video.bg.clone());
    let hardware = video.into_hardware();
    assert_eq!(hardware.palette, palette);
    assert_eq!(hardware.obj, obj);
    assert_eq!(hardware.bg, bg);
}

#[test]
fn invalid_background_files_name_their_asset() {
    let files = VideoFiles {
        bg_tiles: Some(vec![0; 5]),
        ..VideoFiles::default()
    };
    assert!(matches!(
        CartridgeVideo::decode(&files, &defaults()),
        Err(VideoAssetError::BgTiles(_))
    ));
    let files = VideoFiles {
        bg_palettes: Some(vec![0; 10]),
        ..VideoFiles::default()
    };
    assert!(matches!(
        CartridgeVideo::decode(&files, &defaults()),
        Err(VideoAssetError::BgPalettes(_))
    ));
}

#[test]
fn background_assets_copy_and_encode_natively() {
    let files = VideoFiles {
        bg_tiles: Some(vec![0x34; 64]),
        ..VideoFiles::default()
    };
    let source = CartridgeVideo::decode(&files, &defaults()).unwrap();
    let mut target = CartridgeVideo::decode(&VideoFiles::default(), &defaults()).unwrap();
    target.copy_asset(&source, VideoAsset::BgTiles);
    assert_eq!(target.bg, source.bg);
    assert_eq!(target.native_bytes(VideoAsset::BgTiles).len(), 32 * 1024);
    assert_eq!(
        &target.native_bytes(VideoAsset::BgTiles)[..64],
        &[0x34; 64][..]
    );
    assert_eq!(target.native_bytes(VideoAsset::BgPalettes).len(), 64);
}

/// `VideoFiles` with only `asset` present, holding `len` zero bytes.
fn only(asset: VideoAsset, len: usize) -> VideoFiles {
    let mut files = VideoFiles::default();
    let bytes = Some(vec![0; len]);
    match asset {
        VideoAsset::ObjTiles => files.obj_tiles = bytes,
        VideoAsset::ObjAttributes => files.obj_attributes = bytes,
        VideoAsset::ObjPalettes => files.obj_palettes = bytes,
        VideoAsset::MasterPalette => files.master_palette = bytes,
        VideoAsset::BgTiles => files.bg_tiles = bytes,
        VideoAsset::BgPalettes => files.bg_palettes = bytes,
    }
    files
}

#[test]
fn the_sizes_a_video_file_may_have() {
    // (asset, length, whether the file is accepted)
    let cases = [
        (VideoAsset::ObjTiles, 0, true),
        (VideoAsset::ObjTiles, 32, true),
        (VideoAsset::ObjTiles, 8704, true),
        (VideoAsset::ObjTiles, 32768, true),
        (VideoAsset::ObjTiles, 33, false),
        (VideoAsset::ObjTiles, 32800, false),
        (VideoAsset::ObjAttributes, 0, true),
        (VideoAsset::ObjAttributes, 272, true),
        (VideoAsset::ObjAttributes, 1024, true),
        (VideoAsset::ObjAttributes, 1025, false),
        (VideoAsset::ObjPalettes, 128, true),
        (VideoAsset::ObjPalettes, 0, false),
        (VideoAsset::ObjPalettes, 127, false),
        (VideoAsset::ObjPalettes, 129, false),
        (VideoAsset::MasterPalette, 768, true),
        (VideoAsset::MasterPalette, 0, false),
        (VideoAsset::MasterPalette, 767, false),
        (VideoAsset::MasterPalette, 769, false),
    ];
    for (asset, len, accepted) in cases {
        let result = CartridgeVideo::decode(&only(asset, len), &defaults());
        match (accepted, result) {
            (true, Ok(_)) => {}
            (false, Err(err)) => assert!(
                err.to_string().starts_with(&format!("Invalid {asset}")),
                "{asset} with {len} bytes: {err}"
            ),
            (true, Err(err)) => panic!("{asset} with {len} bytes was rejected: {err}"),
            (false, Ok(_)) => panic!("{asset} with {len} bytes was accepted"),
        }
    }
}

#[test]
fn reserved_bits_and_entries_in_video_files_are_rejected() {
    let defaults = defaults();
    let mut attributes = only(VideoAsset::ObjAttributes, 4);
    attributes.obj_attributes.as_mut().unwrap()[3] = 0b1000;
    assert!(CartridgeVideo::decode(&attributes, &defaults).is_err());

    let mut palettes = only(VideoAsset::ObjPalettes, 128);
    palettes.obj_palettes.as_mut().unwrap()[16] = 1;
    assert!(CartridgeVideo::decode(&palettes, &defaults).is_err());
}
