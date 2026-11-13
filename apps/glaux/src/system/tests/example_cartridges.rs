//! Regression baseline for the cartridges in `examples/`.
//!
//! Two kinds of test protect what existing cartridges rely on. The first
//! pins the video asset files they ship and how Glaux reads them. The
//! second plays four of them with scripted buttons and fingerprints the
//! frames they show. Both have to keep passing, unchanged, while the
//! hardware model underneath is replaced.
//!
//! The examples are copied to a scratch directory and never written. Frames
//! are drawn with the fake ROM of these tests, so a change to the System
//! ROM font cannot move a fingerprint.

use std::fs;
use std::path::{Path, PathBuf};

use pretty_assertions::assert_eq;

use super::profiling_workload::copy_dir;
use super::*;
use crate::cartridge::VideoAsset;
use crate::system::video::{CartridgeVideo, VideoFiles};

fn examples_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples")
}

/// Sizes in bytes of the video asset files of an example, in the order of
/// [`VideoAsset::ALL`]: `sprites.bin`, `objattr.bin`, `obp.bin`,
/// `palette.bin`, `bgtiles.bin` and `bgp.bin`. `None` means the example
/// does not ship the file.
type Sizes = [Option<usize>; VideoAsset::ALL.len()];

const SHEET_GAME: Sizes = [Some(8704), Some(272), Some(128), Some(768), None, None];

/// Every example, with the asset files it ships. None ships a background
/// asset or a map.
const EXAMPLES: [(&str, Sizes); 6] = [
    ("log-stress", [None; 6]),
    ("loot-rain", SHEET_GAME),
    (
        "palettes",
        [Some(1024), Some(32), Some(128), Some(768), None, None],
    ),
    ("signal-lost", SHEET_GAME),
    ("sprites", [Some(4096), None, None, None, None, None]),
    ("tile-viewer", [Some(4096), None, None, None, None, None]),
];

fn read_files(name: &str) -> VideoFiles {
    let dir = examples_dir().join(name);
    let [
        obj_tiles,
        obj_attributes,
        obj_palettes,
        master_palette,
        bg_tiles,
        bg_palettes,
    ] = VideoAsset::ALL.map(|asset| fs::read(dir.join(asset.file_name())).ok());
    VideoFiles {
        obj_tiles,
        obj_attributes,
        obj_palettes,
        master_palette,
        bg_tiles,
        bg_palettes,
    }
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
fn the_examples_ship_the_video_files_these_tests_were_written_for() {
    let mut on_disk: Vec<_> = fs::read_dir(examples_dir())
        .unwrap()
        .map(|entry| entry.unwrap().file_name().into_string().unwrap())
        .filter(|name| examples_dir().join(name).join("cartridge.ron").exists())
        .collect();
    on_disk.sort();
    let known: Vec<_> = EXAMPLES.iter().map(|(name, _)| *name).collect();
    assert_eq!(on_disk, known, "an example was added or removed");

    for (name, sizes) in EXAMPLES {
        let files = read_files(name);
        let actual = VideoAsset::ALL.map(|asset| files.get(asset).map(<[u8]>::len));
        assert_eq!(actual, sizes, "{name}");
    }
}

#[test]
fn every_example_decodes_and_keeps_its_bytes_as_machine_state() {
    let defaults = video_defaults();
    let plain = CartridgeVideo::decode(&VideoFiles::default(), &defaults).unwrap();
    for (name, _) in EXAMPLES {
        let files = read_files(name);
        let video = CartridgeVideo::decode(&files, &defaults)
            .unwrap_or_else(|err| panic!("{name} does not decode: {err}"));
        for asset in VideoAsset::ALL {
            let native = video.native_bytes(asset);
            let Some(bytes) = files.get(asset) else {
                assert_eq!(
                    native,
                    plain.native_bytes(asset),
                    "{name}: {asset} is missing and must fall back to the default"
                );
                continue;
            };
            match asset {
                // These files are the whole state of the asset.
                VideoAsset::ObjPalettes | VideoAsset::MasterPalette => {
                    assert_eq!(&*native, bytes, "{name}: {asset}");
                }
                // These files are a prefix of the state: whatever they
                // leave out stays zero.
                VideoAsset::ObjTiles | VideoAsset::ObjAttributes => {
                    assert_eq!(&native[..bytes.len()], bytes, "{name}: {asset}");
                    assert!(
                        native[bytes.len()..].iter().all(|&byte| byte == 0),
                        "{name}: {asset} must leave the rest of the bank empty"
                    );
                }
                VideoAsset::BgTiles | VideoAsset::BgPalettes => {
                    unreachable!("no example ships {asset}")
                }
            }
        }
    }
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
        let result = CartridgeVideo::decode(&only(asset, len), &video_defaults());
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
    let defaults = video_defaults();
    let mut attributes = only(VideoAsset::ObjAttributes, 4);
    attributes.obj_attributes.as_mut().unwrap()[3] = 0b1000;
    assert!(CartridgeVideo::decode(&attributes, &defaults).is_err());

    let mut palettes = only(VideoAsset::ObjPalettes, 128);
    palettes.obj_palettes.as_mut().unwrap()[16] = 1;
    assert!(CartridgeVideo::decode(&palettes, &defaults).is_err());
}

/// A system at the shell whose cartridge root holds a copy of the example
/// `name`, drawing with the fake ROM.
fn shell_with_example(fixture: &Fixture, name: &str) -> System {
    copy_dir(&examples_dir().join(name), &fixture.root().join(name));
    let mut system = System::new(
        info(),
        sounds(),
        video_defaults(),
        fixture.storage(),
        fake_rom().font(),
        fake_rom().palette().clone(),
    );
    while matches!(system.state(), SystemState::Boot { .. }) {
        system = system.update(&[]);
    }
    system
}

/// The buttons pressed on play tick `tick`: walk right and left in turn,
/// and press A and B at fixed intervals.
fn scripted_input(tick: usize) -> Vec<InputEvent> {
    let mut input = Vec::new();
    match tick % 240 {
        0 => input.push(InputEvent::ButtonDown(Button::Right)),
        120 => {
            input.push(InputEvent::ButtonUp(Button::Right));
            input.push(InputEvent::ButtonDown(Button::Left));
        }
        _ => {}
    }
    for (button, period) in [(Button::A, 30), (Button::B, 45)] {
        match tick % period {
            0 => input.push(InputEvent::ButtonDown(button)),
            2 => input.push(InputEvent::ButtonUp(button)),
            _ => {}
        }
    }
    input
}

/// Play ticks after which a frame is fingerprinted.
const CHECKPOINTS: [usize; 4] = [1, 60, 240, 600];

/// FNV-1a over the frame's indices and then the palette it is presented
/// with, so that a change to either moves the fingerprint.
fn fingerprint(frame: &Framebuffer, palette: &Palette) -> u64 {
    const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01b3;
    frame
        .as_bytes()
        .iter()
        .chain(palette.to_rgb888().iter())
        .fold(OFFSET, |hash, &byte| {
            (hash ^ u64::from(byte)).wrapping_mul(PRIME)
        })
}

/// The frames the example `name` shows at each of [`CHECKPOINTS`], after
/// pressing A once to leave a title screen and then playing
/// [`scripted_input`]. Returns their fingerprints and the number of
/// distinct indices in the last frame.
fn play_example(name: &str, label: &str) -> (Vec<u64>, usize) {
    let fixture = Fixture::new(&format!("example-{name}-{label}"));
    let system = shell_with_example(&fixture, name);
    let mut system = enter(enter(system, &format!("load {name}")), "run");
    assert!(is_running(&system), "{name}: {:?}", transcript(&system));

    system = system.update(&[InputEvent::ButtonDown(Button::A)]);
    system = system.update(&[InputEvent::ButtonUp(Button::A)]);
    let rom = fake_rom();
    let mut prints = Vec::new();
    let mut colors = 0;
    for tick in 0..*CHECKPOINTS.last().unwrap() {
        system = system.update(&scripted_input(tick));
        if CHECKPOINTS.contains(&(tick + 1)) {
            let frame = render(&system);
            prints.push(fingerprint(&frame, system.frame_palette(&rom)));
            let mut seen = [false; 256];
            frame
                .as_bytes()
                .iter()
                .for_each(|&i| seen[usize::from(i)] = true);
            colors = seen.into_iter().filter(|&s| s).count();
        }
    }
    assert!(
        is_running(&system)
            && !transcript(&system)
                .iter()
                .any(|line| line.starts_with("! ")),
        "{name} stopped: {:?}",
        transcript(&system).iter().rev().take(3).collect::<Vec<_>>()
    );
    (prints, colors)
}

/// The cartridges played, and whether their frames change over time.
const PLAYED: [(&str, bool); 3] = [
    ("signal-lost", true),
    ("loot-rain", true),
    ("palettes", false),
];

#[test]
fn playing_an_example_twice_shows_the_same_frames() {
    for (name, _) in PLAYED {
        let first = play_example(name, "first");
        let second = play_example(name, "second");
        assert_eq!(first, second, "{name} is not deterministic");
    }
}

#[test]
fn the_played_frames_are_worth_comparing() {
    for (name, animated) in PLAYED {
        let (prints, colors) = play_example(name, "worth");
        assert!(
            colors >= 3,
            "{name}: the frame is nearly blank ({colors} indices)"
        );
        let moves = prints.windows(2).any(|pair| pair[0] != pair[1]);
        assert_eq!(moves, animated, "{name}: {prints:x?}");
    }
}

/// Fingerprints of the frames each example shows at [`CHECKPOINTS`].
///
/// A change here is only legitimate when the visible behavior of the
/// examples is meant to change. To record new values, run this test and
/// copy the table it prints.
const REFERENCES: &[(&str, [u64; 4])] = &[
    (
        "signal-lost",
        [
            0xf22aae5f61d98b62,
            0x3e448640e47656e8,
            0xbb03e3be0b0f5ea5,
            0xa82a8c7c3e848d02,
        ],
    ),
    (
        "loot-rain",
        [
            0xcf0ce8605ac16c96,
            0x9376ef80fb333388,
            0x030a4d67f664eb03,
            0xd462db201a0ef203,
        ],
    ),
    (
        "palettes",
        [
            0x62ff84f7555ea307,
            0x62ff84f7555ea307,
            0x62ff84f7555ea307,
            0x62ff84f7555ea307,
        ],
    ),
];

#[test]
fn the_examples_show_the_frames_recorded_as_reference() {
    let actual: Vec<_> = PLAYED
        .iter()
        .map(|(name, _)| (*name, play_example(name, "reference").0))
        .collect();
    let table: String = actual
        .iter()
        .map(|(name, prints)| {
            let prints: Vec<_> = prints.iter().map(|p| format!("{p:#018x}")).collect();
            format!("    ({name:?}, [{}]),\n", prints.join(", "))
        })
        .collect();
    for ((name, prints), (expected_name, expected)) in actual.iter().zip(REFERENCES.iter().copied())
    {
        assert_eq!(*name, expected_name);
        assert_eq!(
            prints.as_slice(),
            expected,
            "{name} no longer shows its reference frames. Current values:\n{table}"
        );
    }
    assert_eq!(actual.len(), REFERENCES.len(), "Current values:\n{table}");
}
