use super::*;
use glaux_machine::obj_palette::OBJ_PALETTES_BYTES;
use pretty_assertions::assert_eq;

/// Distinct, easily recognized test colors.
fn color(n: usize) -> Rgb {
    Rgb::from_hex(0x100000 + n as u32 * 0x010203)
}

/// A sheet of `tiles_x`x`tiles_y` tiles where tile `(tx, ty)` is painted
/// by `paint(tx, ty, x, y)` at its local pixel `(x, y)`.
fn sheet(
    tiles_x: usize,
    tiles_y: usize,
    paint: impl Fn(usize, usize, usize, usize) -> Option<Rgb>,
) -> Sheet {
    let (width, height) = (tiles_x * 8, tiles_y * 8);
    let pixels = (0..width * height)
        .map(|offset| {
            let (x, y) = (offset % width, offset / width);
            paint(x / 8, y / 8, x % 8, y % 8)
        })
        .collect();
    Sheet {
        width,
        height,
        pixels,
    }
}

/// Each tile uses the colors `palettes[tile_x]`, one per pixel column,
/// cycling; tile rows repeat the same pattern.
fn tiles_with(palettes: &[&[usize]]) -> Sheet {
    let palettes: Vec<Vec<usize>> = palettes.iter().map(|p| p.to_vec()).collect();
    sheet(palettes.len(), 1, move |tx, _, x, y| {
        let colors = &palettes[tx];
        (!colors.is_empty()).then(|| color(colors[(y * 8 + x) % colors.len()]))
    })
}

fn rom_palette() -> Palette {
    Palette::provisional()
}

fn fit_with(sheet: &Sheet, groups: &[SpriteGroup], current: &Palette) -> Result<Fitted, FitError> {
    fit(&FitInput {
        sheet,
        groups,
        current,
        rom_palette: &rom_palette(),
        rom_obj_palettes: &ObjPalettes::linear(),
    })
}

fn fit_sheet(sheet: &Sheet) -> Result<Fitted, FitError> {
    fit_with(sheet, &[], &rom_palette())
}

fn group(name: &str, tile: (usize, usize), size: (usize, usize)) -> SpriteGroup {
    SpriteGroup {
        name: name.into(),
        tile,
        size,
    }
}

fn range(from: usize, to: usize) -> Vec<usize> {
    (from..to).collect()
}

#[test]
fn classifies_alpha_and_the_key_color() {
    let rgba = [
        [1, 2, 3, 255],
        [9, 9, 9, 0],
        [0xff, 0, 0xff, 255],
        [4, 5, 6, 255],
    ]
    .concat();
    let sheet = Sheet::from_rgba(2, 2, &rgba, Some(Rgb::from_hex(0xff00ff))).unwrap();
    assert_eq!(
        sheet.pixels,
        [
            Some(Rgb::from_hex(0x010203)),
            None,
            None,
            Some(Rgb::from_hex(0x040506))
        ]
    );
}

#[test]
fn partly_transparent_pixels_are_reported_not_thresholded() {
    let rgba = [[1, 2, 3, 255], [1, 2, 3, 128], [1, 2, 3, 1], [0, 0, 0, 0]].concat();
    assert_eq!(
        Sheet::from_rgba(2, 2, &rgba, None),
        Err(FitError::SemiTransparent {
            x: 1,
            y: 0,
            alpha: 128,
            count: 2
        })
    );
}

#[test]
fn a_sheet_with_few_colors_uses_one_bank() {
    let ids = range(0, 15);
    let sheet = tiles_with(&[&ids, &ids[..4], &[]]);
    let fitted = fit_sheet(&sheet).unwrap();
    assert_eq!(fitted.summary.banks_used, 1);
    assert_eq!(fitted.summary.colors, 15);
    assert!(fitted.attributes.iter().all(|&bank| bank == 0));
    assert_eq!(fitted.attributes.len(), OBJ_GRID_COLUMNS);
    assert_eq!(fitted.tiles.len(), OBJ_GRID_COLUMNS * OBJ_TILE_BYTES);
}

#[test]
fn disjoint_color_sets_get_separate_banks_and_subsets_share() {
    let (a, b) = (range(0, 10), range(10, 20));
    let sheet = tiles_with(&[&a, &b, &a[..3], &b[5..]]);
    let fitted = fit_sheet(&sheet).unwrap();
    assert_eq!(fitted.summary.banks_used, 2);
    assert_eq!(fitted.attributes[..4], [0, 1, 0, 1]);
}

#[test]
fn units_join_the_bank_they_add_fewest_colors_to() {
    // Two full-ish banks, then a tile sharing most colors with the second.
    let (a, b) = (range(0, 12), range(20, 32));
    let mostly_b: Vec<usize> = range(22, 32).into_iter().chain([40]).collect();
    let sheet = tiles_with(&[&a, &b, &mostly_b]);
    let fitted = fit_sheet(&sheet).unwrap();
    assert_eq!(fitted.attributes[..3], [0, 1, 1]);
}

#[test]
fn exact_colors_reuse_kept_entries_and_new_colors_fill_the_region_in_order() {
    let zughy_3 = rom_palette().get(3);
    let sheet = sheet(1, 1, |_, _, x, _| match x {
        0 => Some(color(1)),
        1 => Some(zughy_3),
        2 => Some(color(0)),
        _ => None,
    });
    let fitted = fit_sheet(&sheet).unwrap();
    assert_eq!(fitted.summary.reused, 1);
    assert_eq!(fitted.summary.allocated, Some(32..=33));
    assert_eq!(fitted.palette.get(32), color(1));
    assert_eq!(fitted.palette.get(33), color(0));
    assert_eq!(
        fitted.palette.entries()[..32],
        rom_palette().entries()[..32]
    );
    assert_eq!(fitted.obj_palettes.resolve(ObjBank::ZERO, 1), Some(3));
}

#[test]
fn kept_entries_come_from_the_cartridge_and_the_region_from_the_rom() {
    let mut current = *rom_palette().entries();
    current[5] = color(7);
    current[40] = color(8);
    let sheet = sheet(1, 1, |_, _, _, _| Some(color(7)));
    let fitted = fit_with(&sheet, &[], &Palette::new(current)).unwrap();
    assert_eq!(fitted.summary.allocated, None);
    assert_eq!(fitted.palette.get(5), color(7));
    assert_eq!(fitted.palette.get(40), rom_palette().get(40));
}

#[test]
fn unused_banks_keep_the_rom_defaults_and_used_banks_hold_only_sheet_colors() {
    // Curated-style ROM banks: every bank points somewhere in 0..32, and
    // nothing matches `linear()`.
    let mut rom_bytes = [0; OBJ_PALETTES_BYTES];
    for (offset, byte) in rom_bytes.iter_mut().enumerate() {
        if offset % OBJ_PALETTE_COLORS != 0 {
            *byte = (31 - offset % 32) as u8;
        }
    }
    let rom_obj_palettes = ObjPalettes::from_bytes(&rom_bytes).unwrap();
    let zughy_3 = rom_palette().get(3);
    let sheet = sheet(1, 1, |_, _, x, _| match x {
        0 => Some(zughy_3),
        1 => Some(color(0)),
        _ => None,
    });
    let fitted = fit(&FitInput {
        sheet: &sheet,
        groups: &[],
        current: &rom_palette(),
        rom_palette: &rom_palette(),
        rom_obj_palettes: &rom_obj_palettes,
    })
    .unwrap();

    let bytes = fitted.obj_palettes.as_bytes();
    // The reused Zughy entry and the allocated entry, then nothing left over
    // from the ROM bank the sheet took.
    let mut bank0 = [0; OBJ_PALETTE_COLORS];
    bank0[1..3].copy_from_slice(&[3, 32]);
    assert_eq!(bytes[..OBJ_PALETTE_COLORS], bank0);
    assert_eq!(
        bytes[OBJ_PALETTE_COLORS..],
        rom_obj_palettes.as_bytes()[OBJ_PALETTE_COLORS..]
    );
}

#[test]
fn a_tile_with_sixteen_colors_names_its_position() {
    let sheet = tiles_with(&[&range(0, 3), &range(0, 16)]);
    assert_eq!(
        fit_sheet(&sheet).unwrap_err(),
        FitError::TileTooColorful {
            tile_x: 1,
            tile_y: 0,
            x: 8,
            y: 0,
            colors: 16
        }
    );
}

#[test]
fn needing_a_ninth_bank_is_an_error() {
    let sets: Vec<Vec<usize>> = (0..9).map(|n| range(n * 13, n * 13 + 13)).collect();
    let sets: Vec<&[usize]> = sets.iter().map(Vec::as_slice).collect();
    let err = fit_sheet(&tiles_with(&sets)).unwrap_err();
    assert_eq!(
        err,
        FitError::TooManyPalettes {
            unit: "tile (8, 0)".into(),
            colors: 13,
            bank_sizes: vec![13; 8],
        }
    );
    assert!(
        err.to_string()
            .starts_with("the sheet needs more than 8 sprite palettes")
    );
}

#[test]
fn more_colors_than_all_banks_hold_is_reported_up_front() {
    let sets: Vec<Vec<usize>> = (0..9).map(|n| range(n * 14, n * 14 + 14)).collect();
    let sets: Vec<&[usize]> = sets.iter().map(Vec::as_slice).collect();
    assert_eq!(
        fit_sheet(&tiles_with(&sets)).unwrap_err(),
        FitError::TooManyColors { colors: 126 }
    );
}

#[test]
fn group_tiles_share_a_bank_that_per_tile_packing_would_split() {
    // Alone, tile 1 would join tile 0's bank and tile 2 open another.
    let sheet = tiles_with(&[&range(0, 10), &range(0, 2), &range(20, 30)]);
    assert_eq!(fit_sheet(&sheet).unwrap().attributes[..3], [0, 0, 1]);

    let groups = [group("hero", (1, 0), (2, 1))];
    let fitted = fit_with(&sheet, &groups, &rom_palette()).unwrap();
    assert_eq!(fitted.attributes[..3], [1, 0, 0]);
    assert_eq!(
        fitted.summary.group_banks,
        [("hero".to_string(), ObjBank::ZERO)]
    );
}

#[test]
fn a_group_with_too_many_colors_is_named() {
    let sheet = tiles_with(&[&range(0, 10), &range(10, 20)]);
    assert_eq!(
        fit_with(&sheet, &[group("ogre", (0, 0), (2, 1))], &rom_palette()).unwrap_err(),
        FitError::GroupTooColorful {
            name: "ogre".into(),
            colors: 20
        }
    );
}

#[test]
fn invalid_groups_are_rejected() {
    let sheet = tiles_with(&[&[1], &[2], &[3]]);
    let cases = [
        (
            vec![group("a", (0, 0), (2, 1)), group("b", (1, 0), (2, 1))],
            FitError::GroupsOverlap("a".into(), "b".into()),
        ),
        (
            vec![group("a", (2, 0), (2, 1))],
            FitError::GroupOutsideSheet("a".into()),
        ),
        (
            vec![group("a", (0, 0), (usize::MAX, 1))],
            FitError::GroupOutsideSheet("a".into()),
        ),
        (
            vec![group("a", (0, 0), (0, 1))],
            FitError::EmptyGroup("a".into()),
        ),
        (vec![group(" ", (0, 0), (1, 1))], FitError::EmptyGroupName),
        (
            vec![group("a", (0, 0), (1, 1)), group("a", (1, 0), (1, 1))],
            FitError::DuplicateGroup("a".into()),
        ),
    ];
    for (groups, expected) in cases {
        assert_eq!(
            fit_with(&sheet, &groups, &rom_palette()).unwrap_err(),
            expected,
            "{groups:?}"
        );
    }
}

#[test]
fn fitting_is_deterministic_and_round_trips() {
    let sheet = sheet(4, 3, |tx, ty, x, y| {
        let n = (tx * 7 + ty * 3 + x / 2 + y / 3) % 23;
        (n % 5 != 0).then(|| color(n + 10 * (tx % 2)))
    });
    let first = fit_sheet(&sheet).unwrap();
    assert_eq!(fit_sheet(&sheet).unwrap(), first);
    // `fit` verifies the round trip itself; a decoded copy of its output
    // must still show the sheet.
    assert_eq!(verify(&sheet, &first), Ok(()));
}

#[test]
fn rejects_bad_dimensions() {
    let odd = Sheet {
        width: 9,
        height: 8,
        pixels: vec![None; 72],
    };
    assert_eq!(
        fit_sheet(&odd).unwrap_err(),
        FitError::InvalidDimensions {
            width: 9,
            height: 8
        }
    );
    let wide = Sheet {
        width: 136,
        height: 8,
        pixels: vec![None; 136 * 8],
    };
    assert_eq!(
        fit_sheet(&wide).unwrap_err(),
        FitError::TooLarge {
            width: 136,
            height: 8
        }
    );
}
