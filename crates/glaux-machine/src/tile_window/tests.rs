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
