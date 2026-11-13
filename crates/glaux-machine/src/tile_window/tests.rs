use super::*;
use crate::tilemap::BgBank;
use pretty_assertions::assert_eq;

fn entry(tile: u16, flip_x: bool, bank: u8) -> TileEntry {
    TileEntry::new(tile, flip_x, false, false, BgBank::new(bank).unwrap()).unwrap()
}

#[test]
fn a_window_starts_all_zero() {
    let window = TileWindow::new();
    assert!(window.as_bytes().iter().all(|&byte| byte == 0));
    assert_eq!(window.entry(63, 63), Some(TileEntry::EMPTY));
}

#[test]
fn entries_sit_at_twice_their_row_major_index_in_little_endian() {
    let mut window = TileWindow::new();
    let placed = entry(0x2A5, true, 2);
    assert!(window.set_entry(3, 5, placed));
    let offset = 2 * (5 * 64 + 3);
    assert_eq!(
        window.as_bytes()[offset..offset + 2],
        placed.raw().to_le_bytes()
    );
    assert_eq!(window.entry(3, 5), Some(placed));
    assert_eq!(
        window.as_bytes().iter().filter(|&&byte| byte != 0).count(),
        2
    );
}

#[test]
fn raw_writes_drop_only_the_reserved_bit_and_read_back_as_an_entry() {
    let mut window = TileWindow::new();
    assert!(window.set_byte(0, 0xFF));
    assert!(window.set_byte(1, 0xFF));
    assert_eq!(window.as_bytes()[..2], [0xFF, 0x7F]);

    let read = window.entry(0, 0).unwrap();
    assert_eq!(read.raw(), 0x7FFF);
    assert_eq!(read.tile(), 0x3FF);
    assert!(read.flip_x() && read.flip_y() && read.priority());
    assert_eq!(read.bank(), BgBank::new(3).unwrap());
}

#[test]
fn tile_zero_is_stored_like_any_other_tile() {
    let mut window = TileWindow::new();
    let flagged = TileEntry::new(0, true, true, true, BgBank::new(1).unwrap()).unwrap();
    assert!(window.set_entry(0, 0, flagged));
    assert_eq!(window.entry(0, 0), Some(flagged));
}

#[test]
fn positions_outside_the_window_are_refused() {
    let mut window = TileWindow::new();
    assert_eq!(window.entry(64, 0), None);
    assert_eq!(window.entry(0, 64), None);
    assert!(!window.set_entry(64, 0, TileEntry::EMPTY));
    assert!(!window.set_byte(TILE_WINDOW_BYTES, 1));
}

mod transfer {
    use super::*;
    use pretty_assertions::assert_eq;
    use proptest::prelude::*;

    /// A map whose cell `(x, y)` holds a distinct tile, so any misplaced
    /// cell shows up.
    fn numbered(width: usize, height: usize) -> TileLayer {
        let mut layer = TileLayer::empty(width, height).unwrap();
        for y in 0..height {
            for x in 0..width {
                let tile = 1 + ((y * width + x) % 1023) as u16;
                layer.set(x, y, entry(tile, x % 2 == 1, (y % 4) as u8));
            }
        }
        layer
    }

    fn filled(window: &TileWindow) -> usize {
        (0..64 * 64)
            .filter(|i| window.entry(i % 64, i / 64) != Some(TileEntry::EMPTY))
            .count()
    }

    #[test]
    fn a_full_reload_of_a_window_sized_map_matches_its_file_bytes() {
        let map = numbered(64, 64);
        let mut window = TileWindow::new();
        window.transfer(&map, 0, 0, 64, 64);
        assert_eq!(window.as_bytes().as_slice(), map.to_bytes().as_slice());
    }

    #[test]
    fn cells_past_the_map_edge_are_written_as_empty_entries() {
        let map = numbered(70, 10);
        let mut window = TileWindow::new();
        window.set_entry(5, 0, entry(9, false, 1));
        window.transfer(&map, 66, 0, 10, 1);
        assert_eq!(window.entry(2, 0), map.get(66, 0));
        assert_eq!(window.entry(3, 0), map.get(67, 0));
        assert_eq!(window.entry(4, 0), map.get(68, 0));
        assert_eq!(window.entry(5, 0), map.get(69, 0));
        for x in [6, 7, 8, 9, 10, 11] {
            assert_eq!(window.entry(x, 0), Some(TileEntry::EMPTY), "column {x}");
        }
    }

    #[test]
    fn the_map_never_repeats_to_fill_the_window() {
        let map = numbered(4, 4);
        let mut window = TileWindow::new();
        window.transfer(&map, 0, 0, 64, 64);
        assert_eq!(window.entry(4, 0), Some(TileEntry::EMPTY));
        assert_eq!(window.entry(0, 4), Some(TileEntry::EMPTY));
        assert_eq!(window.entry(3, 3), map.get(3, 3));
    }

    #[test]
    fn negative_world_cells_use_a_euclidean_modulo() {
        let map = numbered(8, 8);
        let mut window = TileWindow::new();
        window.transfer(&map, -2, -1, 4, 2);
        // World x -2 and -1 are outside the map and land on columns 62, 63.
        assert_eq!(window.entry(62, 63), Some(TileEntry::EMPTY));
        assert_eq!(window.entry(63, 63), Some(TileEntry::EMPTY));
        assert_eq!(window.entry(0, 0), map.get(0, 0));
        assert_eq!(window.entry(1, 0), map.get(1, 0));
        assert_eq!(window.entry(0, 63), Some(TileEntry::EMPTY));
    }

    #[test]
    fn an_origin_past_the_window_wraps_the_physical_cells() {
        let map = numbered(256, 256);
        let mut window = TileWindow::new();
        window.transfer(&map, 100, 200, 1, 1);
        assert_eq!(window.entry(100 % 64, 200 % 64), map.get(100, 200));
    }

    #[test]
    fn a_column_writes_64_entries_and_a_row_writes_64() {
        let map = numbered(256, 256);
        let mut column = TileWindow::new();
        column.transfer(&map, 130, 17, 1, 64);
        let written = (0..64)
            .filter(|&y| column.entry(130 % 64, (17 + y) % 64) == map.get(130, 17 + y))
            .count();
        assert_eq!(written, 64);
        assert_eq!(filled(&column), 64);

        let mut row = TileWindow::new();
        row.transfer(&map, 3, 200, 64, 1);
        assert_eq!(filled(&row), 64);
    }

    #[test]
    fn a_region_longer_than_the_window_is_cut_to_64_cells() {
        let map = numbered(256, 256);
        let mut window = TileWindow::new();
        window.transfer(&map, 0, 0, 200, 200);
        let mut expected = TileWindow::new();
        expected.transfer(&map, 0, 0, 64, 64);
        assert_eq!(window, expected);
    }

    #[test]
    fn extreme_coordinates_do_not_overflow() {
        let map = numbered(4, 4);
        let mut window = TileWindow::new();
        window.transfer(&map, i64::from(i32::MAX), i64::from(i32::MIN), 64, 64);
        window.transfer(&map, i64::from(i32::MIN), i64::from(i32::MAX), 64, 64);
        // Both far corners are outside the 4 x 4 map, so every cell is empty.
        assert!(window.as_bytes().iter().all(|&byte| byte == 0));
    }

    #[test]
    fn the_source_is_not_changed_by_later_window_writes() {
        let map = numbered(64, 64);
        let before = map.to_bytes();
        let mut window = TileWindow::new();
        window.transfer(&map, 0, 0, 64, 64);
        window.set_entry(0, 0, entry(1023, true, 3));
        assert_eq!(map.to_bytes(), before);
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(64))]

        #[test]
        fn a_transfer_matches_a_per_cell_reference(
            width in 1usize..80,
            height in 1usize..80,
            x in -300i64..300,
            y in -300i64..300,
            w in 1usize..=64,
            h in 1usize..=64,
        ) {
            let map = numbered(width, height);
            let mut window = TileWindow::new();
            window.transfer(&map, x, y, w, h);

            let mut expected = [[TileEntry::EMPTY; 64]; 64];
            for row in 0..h as i64 {
                for column in 0..w as i64 {
                    let (wx, wy) = (x + column, y + row);
                    let inside = wx >= 0 && wy >= 0 && (wx as usize) < width && (wy as usize) < height;
                    expected[wy.rem_euclid(64) as usize][wx.rem_euclid(64) as usize] =
                        if inside { map.get(wx as usize, wy as usize).unwrap() } else { TileEntry::EMPTY };
                }
            }
            for (wy, row) in expected.iter().enumerate() {
                for (wx, cell) in row.iter().enumerate() {
                    prop_assert_eq!(window.entry(wx, wy), Some(*cell));
                }
            }
        }
    }
}
