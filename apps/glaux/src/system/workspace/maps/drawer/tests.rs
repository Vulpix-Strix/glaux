use super::super::fixture::{Fixture, document, editor, entry, render};
use super::super::frame::Frame;
use super::*;
use glaux_machine::framebuffer::Framebuffer;
use glaux_machine::tilemap::BgBank;
use pretty_assertions::assert_eq;

fn pixels(fb: &Framebuffer, rect: Rect) -> Vec<u8> {
    (rect.y..rect.bottom())
        .flat_map(|y| (rect.x..rect.right()).map(move |x| (x, y)))
        .map(|(x, y)| fb.pixel(x, y).unwrap())
        .collect()
}

fn full(tile: u16, flip_x: bool, flip_y: bool, priority: bool, bank: u8) -> TileEntry {
    TileEntry::new(tile, flip_x, flip_y, priority, BgBank::new(bank).unwrap()).unwrap()
}

fn strip_cell(frame: &Frame, tile: usize) -> Rect {
    let strip = frame.strip.unwrap();
    Rect {
        x: strip.x + (tile % STRIP_COLUMNS) as i32 * 8,
        y: strip.y + (tile / STRIP_COLUMNS) as i32 * 8,
        width: 8,
        height: 8,
    }
}

#[test]
fn the_flags_name_the_flips_and_the_priority_bit_in_a_fixed_order() {
    assert_eq!(flags(entry(1)), "");
    assert_eq!(flags(full(1, false, true, false, 0)), "Y");
    assert_eq!(flags(full(1, true, false, true, 3)), "XP");
    assert_eq!(flags(full(1, true, true, true, 0)), "XYP");
}

#[test]
fn the_strip_shows_each_tile_in_the_brushs_bank() {
    let fixture = Fixture::new();
    let mut editor = editor();
    let document = document(10, 10);
    let frame = editor.frame();
    let fb = render(&editor, &fixture, &document);
    // Tile 1 is solid local 3, which BGP0 maps to master 100.
    let one = strip_cell(&frame, 1);
    assert_eq!(fb.pixel(one.x + 4, one.y + 4), Some(fixture.slot(100)));
    editor.edit.brush = full(1, false, false, false, 1);
    let bank_one = render(&editor, &fixture, &document);
    assert_eq!(
        bank_one.pixel(one.x + 4, one.y + 4),
        Some(fixture.slot(103))
    );
}

#[test]
fn tile_zero_and_empty_tiles_show_as_transparent_in_the_strip() {
    let fixture = Fixture::new();
    let editor = editor();
    let document = document(10, 10);
    let frame = editor.frame();
    let fb = render(&editor, &fixture, &document);
    for tile in [0, 40] {
        let cell = strip_cell(&frame, tile);
        let inside = pixels(&fb, cell);
        assert!(
            inside.iter().all(|&p| p == 12 || p == 13 || p == ACCENT),
            "tile {tile}"
        );
    }
}

#[test]
fn the_brushs_tile_is_ringed_where_the_strip_shows_it() {
    let fixture = Fixture::new();
    let mut editor = editor();
    let document = document(10, 10);
    let frame = editor.frame();
    editor.edit.brush = entry(2);
    let fb = render(&editor, &fixture, &document);
    let cell = strip_cell(&frame, 2);
    assert_eq!(fb.pixel(cell.x, cell.y), Some(ACCENT));
    assert_ne!(
        fb.pixel(strip_cell(&frame, 1).x, strip_cell(&frame, 1).y),
        Some(ACCENT)
    );
    editor.edit.strip_top = 20;
    let away = render(&editor, &fixture, &document);
    assert!(!pixels(&away, frame.strip.unwrap()).contains(&ACCENT));
}

#[test]
fn the_strip_shows_the_rows_it_is_scrolled_to() {
    let fixture = Fixture::new();
    let mut editor = editor();
    let document = document(10, 10);
    let frame = editor.frame();
    let top = render(&editor, &fixture, &document);
    editor.edit.strip_top = 1;
    let scrolled = render(&editor, &fixture, &document);
    let before = pixels(&top, strip_cell(&frame, 16));
    let after = pixels(&scrolled, strip_cell(&frame, 0));
    assert_eq!(before.len(), after.len());
    assert!(before.iter().all(|&p| p == 12 || p == 13));
    assert!(after.iter().all(|&p| p == 12 || p == 13));
}

#[test]
fn the_edited_layer_is_the_inverted_row_and_the_others_are_not() {
    let fixture = Fixture::new();
    let mut editor = editor();
    let document = document(10, 10);
    for layer in BgLayer::ALL {
        editor.edit.layer = layer;
        let fb = render(&editor, &fixture, &document);
        let rows = editor.frame().layer_rows.unwrap();
        for (slot, row) in rows.iter().enumerate() {
            let corner = fb.pixel(row.name.right() - 1, row.row.y);
            let expected = if Frame::layer_in_slot(slot) == layer {
                PALE
            } else {
                BACKGROUND
            };
            assert_eq!(corner, Some(expected), "{layer}, row {slot}");
        }
    }
}

#[test]
fn eyes_and_locks_look_different_for_each_state() {
    let fixture = Fixture::new();
    let mut editor = editor();
    let document = document(10, 10);
    let rows = editor.frame().layer_rows.unwrap();
    let base = render(&editor, &fixture, &document);
    editor.edit.visible[BgLayer::Bg2.index()] = false;
    let hidden = render(&editor, &fixture, &document);
    assert_ne!(pixels(&base, rows[0].eye), pixels(&hidden, rows[0].eye));
    assert_eq!(pixels(&base, rows[1].eye), pixels(&hidden, rows[1].eye));
    editor.edit.locked[BgLayer::Bg2.index()] = true;
    let locked = render(&editor, &fixture, &document);
    assert_ne!(pixels(&hidden, rows[0].lock), pixels(&locked, rows[0].lock));
}

#[test]
fn a_hidden_layer_is_dimmed_in_the_list() {
    let fixture = Fixture::new();
    let mut editor = editor();
    let document = document(10, 10);
    let rows = editor.frame().layer_rows.unwrap();
    editor.edit.layer = BgLayer::Bg0;
    let shown = render(&editor, &fixture, &document);
    editor.edit.visible[BgLayer::Bg2.index()] = false;
    let hidden = render(&editor, &fixture, &document);
    let name = rows[0].name;
    assert!(pixels(&shown, name).contains(&TEXT));
    assert!(!pixels(&shown, name).contains(&IDLE_SHADE));
    assert!(!pixels(&hidden, name).contains(&TEXT));
    assert!(pixels(&hidden, name).contains(&IDLE_SHADE));
}

#[test]
fn the_collapsed_bar_shows_the_layers_with_the_edited_one_in_the_accent() {
    let fixture = Fixture::new();
    let mut editor = editor();
    let document = document(10, 10);
    editor.edit.drawer = super::super::frame::DrawerSize::Collapsed;
    editor.edit.layer = BgLayer::Bg2;
    let frame = editor.frame();
    assert!(frame.strip.is_none());
    let fb = render(&editor, &fixture, &document);
    for layer in BgLayer::ALL {
        let chip = frame.chips[layer.index()];
        let face = fb.pixel(chip.x, chip.y);
        let expected = if layer == BgLayer::Bg2 {
            ACCENT
        } else {
            BACKGROUND
        };
        assert_eq!(face, Some(expected), "{layer}");
    }
}

#[test]
fn the_brush_is_drawn_in_the_bar_as_it_will_paint() {
    let fixture = Fixture::new();
    let mut editor = editor();
    let document = document(10, 10);
    let frame = editor.frame();
    // Tile 3 has one pixel at its top left, which a flip moves.
    editor.edit.brush = entry(3);
    let plain = render(&editor, &fixture, &document);
    assert_eq!(
        plain.pixel(frame.brush.x, frame.brush.y),
        Some(fixture.slot(102))
    );
    editor.edit.brush = full(3, true, true, false, 0);
    let flipped = render(&editor, &fixture, &document);
    assert_eq!(
        flipped.pixel(frame.brush.x + 7, frame.brush.y + 7),
        Some(fixture.slot(102))
    );
    assert_ne!(
        flipped.pixel(frame.brush.x, frame.brush.y),
        Some(fixture.slot(102))
    );
}

#[test]
fn the_size_button_shows_how_much_of_the_drawer_is_open() {
    let fixture = Fixture::new();
    let mut editor = editor();
    let document = document(10, 10);
    let mut faces = Vec::new();
    for size in [
        super::super::frame::DrawerSize::Collapsed,
        super::super::frame::DrawerSize::Compact,
        super::super::frame::DrawerSize::Expanded,
    ] {
        editor.edit.drawer = size;
        let fb = render(&editor, &fixture, &document);
        faces.push(pixels(&fb, editor.frame().size_button));
    }
    assert_ne!(faces[0], faces[1]);
    assert_ne!(faces[1], faces[2]);
    assert_ne!(faces[0], faces[2]);
}

#[test]
fn the_strip_scroll_thumb_stays_on_its_bar_at_both_ends() {
    let fixture = Fixture::new();
    let mut editor = editor();
    let document = document(10, 10);
    let frame = editor.frame();
    let framed = frame.strip_frame().unwrap();
    let track = Rect {
        x: framed.right() + 1,
        y: frame.strip.unwrap().y,
        width: 2,
        height: frame.strip.unwrap().height,
    };
    let top = render(&editor, &fixture, &document);
    assert_eq!(top.pixel(track.x, track.y), Some(DIM));
    editor.edit.strip_top = 64 - frame.strip_rows;
    let bottom = render(&editor, &fixture, &document);
    assert_eq!(bottom.pixel(track.x, track.bottom() - 1), Some(DIM));
    assert_ne!(bottom.pixel(track.x, track.y), Some(DIM));
}
