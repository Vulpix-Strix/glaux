use super::super::fixture::{Fixture, document, editor, entry, render, render_with};
use super::*;
use glaux_machine::tilemap::{BgBank, TileEntry};
use pretty_assertions::assert_eq;

use super::super::frame::DrawerSize;

const CHECK_DARK: u8 = 12;
const CHECK_LIGHT: u8 = 13;

fn inside_cell(editor: &MapEditor, document: &MapDocument, cell: (i32, i32)) -> (i32, i32) {
    let frame = editor.frame();
    let rect = editor
        .viewport(&frame, document)
        .cell_rect(frame.canvas, cell);
    (rect.x + 5, rect.y + 5)
}

fn pixel_at(fb: &Framebuffer, at: (i32, i32)) -> u8 {
    fb.pixel(at.0, at.1).unwrap()
}

fn pixels(fb: &Framebuffer, rect: Rect) -> Vec<u8> {
    (rect.y..rect.bottom())
        .flat_map(|y| (rect.x..rect.right()).map(move |x| (x, y)))
        .map(|(x, y)| fb.pixel(x, y).unwrap())
        .collect()
}

#[test]
fn clipping_keeps_only_the_part_inside() {
    let clip = Rect {
        x: 10,
        y: 10,
        width: 20,
        height: 20,
    };
    let rect = |x, y, width, height| Rect {
        x,
        y,
        width,
        height,
    };
    assert_eq!(clipped(rect(0, 0, 15, 15), clip), Some(rect(10, 10, 5, 5)));
    assert_eq!(clipped(rect(12, 12, 4, 4), clip), Some(rect(12, 12, 4, 4)));
    assert_eq!(
        clipped(rect(25, 25, 50, 50), clip),
        Some(rect(25, 25, 5, 5))
    );
    assert_eq!(clipped(rect(0, 0, 10, 10), clip), None, "only touching");
    assert_eq!(clipped(rect(30, 10, 5, 5), clip), None);
    assert_eq!(clipped(rect(-50, -50, 10, 10), clip), None);
}

#[test]
fn filling_and_outlining_never_touch_what_is_outside_the_clip() {
    let clip = Rect {
        x: 100,
        y: 100,
        width: 10,
        height: 10,
    };
    let mut fb = Framebuffer::new();
    fill_clipped(
        &mut fb,
        Rect {
            x: 90,
            y: 90,
            width: 40,
            height: 40,
        },
        clip,
        7,
    );
    assert_eq!(fb.as_bytes().iter().filter(|&&p| p == 7).count(), 100);
    let mut fb = Framebuffer::new();
    outline_clipped(
        &mut fb,
        Rect {
            x: 95,
            y: 95,
            width: 30,
            height: 30,
        },
        clip,
        7,
    );
    assert!(
        fb.as_bytes().iter().all(|&p| p == 0),
        "the sides are outside"
    );
    outline_clipped(
        &mut fb,
        Rect {
            x: 100,
            y: 95,
            width: 5,
            height: 30,
        },
        clip,
        7,
    );
    assert_eq!(
        fb.pixel(100, 100),
        Some(7),
        "the left edge crosses the clip"
    );
    assert_eq!(fb.pixel(104, 100), Some(7), "and the right one");
    assert_eq!(fb.pixel(100, 99), Some(0));
}

#[test]
fn ruler_numbers_are_spaced_by_the_room_they_need() {
    // Tiles of 8, 16 and 32 pixels, labels 24 and 12 pixels apart at least.
    assert_eq!([8, 16, 32].map(|tile| ruler_step(tile, 24)), [4, 2, 1]);
    assert_eq!([8, 16, 32].map(|tile| ruler_step(tile, 12)), [2, 1, 1]);
    assert_eq!(ruler_step(1, 1000), 16, "the largest step is the limit");
}

#[test]
fn cells_are_drawn_in_the_exact_colors_of_their_tiles_and_banks() {
    let fixture = Fixture::new();
    let mut editor = editor();
    let mut document = document(30, 20);
    document.set_cell(BgLayer::Bg1, 1, 1, entry(1));
    document.set_cell(
        BgLayer::Bg1,
        2,
        1,
        TileEntry::new(1, false, false, false, BgBank::new(1).unwrap()).unwrap(),
    );
    editor.edit.grid = false;
    let fb = render(&editor, &fixture, &document);
    assert_eq!(
        pixel_at(&fb, inside_cell(&editor, &document, (1, 1))),
        fixture.slot(100)
    );
    assert_eq!(
        pixel_at(&fb, inside_cell(&editor, &document, (2, 1))),
        fixture.slot(103)
    );
    assert_eq!(
        fixture.preview.palette().get(fixture.slot(100)),
        fixture.master.get(100)
    );
}

#[test]
fn empty_cells_and_transparent_pixels_show_the_checkerboard() {
    let fixture = Fixture::new();
    let mut editor = editor();
    let mut document = document(30, 20);
    // Tile 3 has one opaque pixel, so the rest of its cell is transparent.
    document.set_cell(BgLayer::Bg1, 1, 1, entry(3));
    editor.edit.grid = false;
    let fb = render(&editor, &fixture, &document);
    let at = inside_cell(&editor, &document, (1, 1));
    assert!([CHECK_DARK, CHECK_LIGHT].contains(&pixel_at(&fb, at)));
    let rect = editor
        .viewport(&editor.frame(), &document)
        .cell_rect(editor.frame().canvas, (1, 1));
    assert_eq!(
        fb.pixel(rect.x, rect.y),
        Some(fixture.slot(102)),
        "the one opaque pixel, drawn 2x2"
    );
    assert_eq!(fb.pixel(rect.x + 1, rect.y + 1), Some(fixture.slot(102)));
    let empty = inside_cell(&editor, &document, (5, 5));
    assert!([CHECK_DARK, CHECK_LIGHT].contains(&pixel_at(&fb, empty)));
}

#[test]
fn a_hidden_layer_is_not_drawn_and_a_shown_one_is() {
    let fixture = Fixture::new();
    let mut editor = editor();
    let mut document = document(30, 20);
    document.set_cell(BgLayer::Bg0, 1, 1, entry(1));
    editor.edit.grid = false;
    let at = inside_cell(&editor, &document, (1, 1));
    assert_eq!(
        pixel_at(&render(&editor, &fixture, &document), at),
        fixture.slot(100)
    );
    editor.edit.visible[BgLayer::Bg0.index()] = false;
    let hidden = render(&editor, &fixture, &document);
    assert!([CHECK_DARK, CHECK_LIGHT].contains(&pixel_at(&hidden, at)));
}

#[test]
fn layers_stack_by_the_compositors_depths_and_the_priority_bit() {
    let fixture = Fixture::new();
    let mut editor = editor();
    let mut document = document(30, 20);
    editor.edit.grid = false;
    // Tile 1 on BG0 and tile 2 on BG1: BG1 is in front.
    document.set_cell(BgLayer::Bg0, 1, 1, entry(1));
    document.set_cell(BgLayer::Bg1, 1, 1, entry(2));
    // The same, with the priority bit on the BG0 cell.
    document.set_cell(
        BgLayer::Bg0,
        3,
        1,
        TileEntry::new(1, false, false, true, BgBank::ZERO).unwrap(),
    );
    document.set_cell(BgLayer::Bg1, 3, 1, entry(2));
    let fb = render(&editor, &fixture, &document);
    assert_eq!(
        pixel_at(&fb, inside_cell(&editor, &document, (1, 1))),
        fixture.slot(101)
    );
    assert_eq!(
        pixel_at(&fb, inside_cell(&editor, &document, (3, 1))),
        fixture.slot(100)
    );
}

#[test]
fn flips_mirror_the_tile() {
    let fixture = Fixture::new();
    let mut editor = editor();
    let mut document = document(30, 20);
    editor.edit.grid = false;
    document.set_cell(
        BgLayer::Bg1,
        1,
        1,
        TileEntry::new(3, true, false, false, BgBank::ZERO).unwrap(),
    );
    document.set_cell(
        BgLayer::Bg1,
        3,
        1,
        TileEntry::new(3, false, true, false, BgBank::ZERO).unwrap(),
    );
    let fb = render(&editor, &fixture, &document);
    let frame = editor.frame();
    let view = editor.viewport(&frame, &document);
    let flipped_x = view.cell_rect(frame.canvas, (1, 1));
    let flipped_y = view.cell_rect(frame.canvas, (3, 1));
    // The pixel moved from the top left to the top right, and to the
    // bottom left.
    assert_eq!(
        fb.pixel(flipped_x.x + 14, flipped_x.y),
        Some(fixture.slot(102))
    );
    assert_ne!(fb.pixel(flipped_x.x, flipped_x.y), Some(fixture.slot(102)));
    assert_eq!(
        fb.pixel(flipped_y.x, flipped_y.y + 14),
        Some(fixture.slot(102))
    );
}

#[test]
fn nothing_is_drawn_outside_the_canvas_whatever_the_zoom_and_scroll() {
    let fixture = Fixture::new();
    for rulers in [false, true] {
        for drawer in [
            DrawerSize::Collapsed,
            DrawerSize::Compact,
            DrawerSize::Expanded,
        ] {
            let mut editor = editor();
            editor.edit.rulers = rulers;
            editor.edit.drawer = drawer;
            editor
                .edit
                .view
                .set_zoom(super::super::view::Zoom::X4, (256, 256), (200, 100));
            editor.edit.view.scroll_by(777, 555, (256, 256), (200, 100));
            editor.edit.selection =
                super::super::tools::Selection::between((0, 0), (255, 255), (256, 256));
            let empty = document(256, 256);
            let mut busy = document(256, 256);
            for y in 0..256 {
                for x in 0..256 {
                    busy.set_cell(BgLayer::Bg1, x, y, entry(1 + ((x + y) % 3) as u16));
                }
            }
            let frame = editor.frame();
            let used = frame.tile_info.map(|(x, y)| Rect {
                x,
                y,
                width: frame.drawer.right() - x,
                height: 12,
            });
            let a = render(&editor, &fixture, &empty);
            let b = render(&editor, &fixture, &busy);
            for y in 0..240 {
                for x in 0..320 {
                    if [frame.canvas, frame.options, frame.undo, frame.redo]
                        .iter()
                        .chain(&used)
                        .any(|part| part.contains_xy(x, y))
                    {
                        continue;
                    }
                    // The marker of unsaved edits and the undo plates show what was edited.
                    // The thumbs of the scroll bars depend on the map's
                    // size only, which is the same.
                    assert_eq!(
                        a.pixel(x, y),
                        b.pixel(x, y),
                        "({x}, {y}), rulers {rulers}, {drawer:?}"
                    );
                }
            }
        }
    }
}

#[test]
fn the_grid_only_draws_lines_inside_the_map() {
    let fixture = Fixture::new();
    let mut editor = editor();
    let document = document(4, 3);
    let on = render(&editor, &fixture, &document);
    editor.edit.grid = false;
    let off = render(&editor, &fixture, &document);
    let frame = editor.frame();
    let view = editor.viewport(&frame, &document);
    let map = Rect {
        x: frame.canvas.x,
        y: frame.canvas.y,
        width: 4 * 16 + 1,
        height: 3 * 16 + 1,
    };
    let mut changed = 0;
    for y in 0..240 {
        for x in 0..320 {
            if frame.options.contains_xy(x, y) {
                continue;
            }
            if on.pixel(x, y) != off.pixel(x, y) {
                changed += 1;
                assert!(map.contains_xy(x, y), "({x}, {y})");
                assert_eq!(on.pixel(x, y), Some(GRID_DOT));
                assert!((x - frame.canvas.x) % 16 == 0 || (y - frame.canvas.y) % 16 == 0);
            }
        }
    }
    assert!(changed > 0);
    assert_eq!(view.offset(), (0, 0));
}

#[test]
fn a_map_smaller_than_the_canvas_leaves_the_rest_blank() {
    let fixture = Fixture::new();
    let mut editor = editor();
    editor.edit.grid = false;
    let document = document(4, 3);
    let fb = render(&editor, &fixture, &document);
    let frame = editor.frame();
    let right_of_map = Rect {
        x: frame.canvas.x + 4 * 16,
        y: frame.canvas.y,
        width: frame.canvas.width - 4 * 16,
        height: frame.canvas.height,
    };
    assert!(pixels(&fb, right_of_map).iter().all(|&p| p == BACKGROUND));
    let inside = pixels(
        &fb,
        Rect {
            x: frame.canvas.x,
            y: frame.canvas.y,
            width: 4 * 16,
            height: 3 * 16,
        },
    );
    assert!(inside.iter().all(|&p| p == CHECK_DARK || p == CHECK_LIGHT));
}

#[test]
fn the_selection_is_ringed_in_the_accent_color_and_cut_at_the_canvas() {
    let fixture = Fixture::new();
    let mut editor = editor();
    let document = document(64, 64);
    editor.edit.grid = false;
    editor.edit.selection = super::super::tools::Selection::between((2, 1), (4, 2), (64, 64));
    let fb = render(&editor, &fixture, &document);
    let frame = editor.frame();
    let view = editor.viewport(&frame, &document);
    let top_left = view.cell_rect(frame.canvas, (2, 1));
    assert_eq!(fb.pixel(top_left.x, top_left.y), Some(ACCENT));
    let bottom_right = view.cell_rect(frame.canvas, (4, 2));
    assert_eq!(
        fb.pixel(bottom_right.right() - 1, bottom_right.bottom() - 1),
        Some(ACCENT)
    );
    // Scrolled so that only the last rows of the selection show, the top
    // edge is off the canvas and not drawn.
    editor
        .edit
        .view
        .scroll_tiles(0, 2, (64, 64), (frame.canvas.width, frame.canvas.height));
    let scrolled = render(&editor, &fixture, &document);
    let view = editor.viewport(&frame, &document);
    let top = view.cell_rect(frame.canvas, (2, 1));
    assert!(top.y < frame.canvas.y);
    assert_ne!(scrolled.pixel(top.x + 3, frame.canvas.y), Some(ACCENT));
}

#[test]
fn the_cell_under_the_pointer_is_ringed_unless_the_hand_is_in_use() {
    let fixture = Fixture::new();
    let mut editor = editor();
    let small = document(2, 2);
    let document = document(30, 20);
    editor.edit.grid = false;
    let frame = editor.frame();
    let view = editor.viewport(&frame, &document);
    let cell = view.cell_rect(frame.canvas, (3, 2));
    let pointer = Point::new((cell.x + 8) as u16, (cell.y + 8) as u16);
    let ringed = render_with(&editor, Ok(fixture.source()), &document, Some(pointer));
    assert_eq!(ringed.pixel(cell.x - 1, cell.y - 1), Some(ACCENT));
    assert_eq!(ringed.pixel(cell.x + 16, cell.y + 16), Some(ACCENT));
    editor.edit.tool = Tool::Hand;
    let hand = render_with(&editor, Ok(fixture.source()), &document, Some(pointer));
    assert_ne!(hand.pixel(cell.x - 1, cell.y - 1), Some(ACCENT));
    editor.edit.tool = Tool::Pencil;
    let outside = view.cell_rect(frame.canvas, (5, 5));
    let at = Point::new((outside.x + 8) as u16, (outside.y + 8) as u16);
    let fb = render_with(&editor, Ok(fixture.source()), &small, Some(at));
    assert_ne!(fb.pixel(outside.x - 1, outside.y - 1), Some(ACCENT));
}

#[test]
fn the_scroll_bars_show_only_where_the_map_is_larger_than_the_canvas() {
    let fixture = Fixture::new();
    let editor = editor();
    let frame = editor.frame();
    let small = render(&editor, &fixture, &document(4, 3));
    assert!(pixels(&small, frame.scroll_v).iter().all(|&p| p == 0));
    assert!(pixels(&small, frame.scroll_h).iter().all(|&p| p == 0));
    let wide = render(&editor, &fixture, &document(256, 4));
    assert!(pixels(&wide, frame.scroll_h).contains(&DIM));
    assert!(pixels(&wide, frame.scroll_v).iter().all(|&p| p == 0));
    let tall = render(&editor, &fixture, &document(4, 256));
    assert!(pixels(&tall, frame.scroll_v).contains(&DIM));
}

#[test]
fn the_scroll_thumb_moves_with_the_view_and_stays_on_its_bar() {
    let fixture = Fixture::new();
    let mut editor = editor();
    let document = document(256, 256);
    let frame = editor.frame();
    let thumb = |fb: &Framebuffer| {
        let column = pixels(
            fb,
            Rect {
                width: 1,
                ..frame.scroll_v
            },
        );
        let first = column.iter().position(|&p| p == DIM).unwrap();
        let last = column.iter().rposition(|&p| p == DIM).unwrap();
        (first, last)
    };
    let top = thumb(&render(&editor, &fixture, &document));
    assert_eq!(top.0, 0);
    editor.edit.view.scroll_by(
        0,
        1_000_000,
        (256, 256),
        (frame.canvas.width, frame.canvas.height),
    );
    let bottom = thumb(&render(&editor, &fixture, &document));
    assert_eq!(bottom.1 as i32, frame.scroll_v.height - 1);
    assert!(bottom.0 > top.0);
}

#[test]
fn the_rulers_draw_in_their_bands_only_while_on() {
    let fixture = Fixture::new();
    let mut editor = editor();
    let document = document(64, 64);
    editor.edit.rulers = true;
    let frame = editor.frame();
    let on = render(&editor, &fixture, &document);
    assert!(pixels(&on, frame.ruler_top).iter().any(|&p| p != 0));
    assert!(pixels(&on, frame.ruler_left).iter().any(|&p| p != 0));
    let beside = Rect {
        x: frame.ruler_top.right(),
        width: 4,
        ..frame.ruler_top
    };
    assert!(pixels(&on, beside).iter().all(|&p| p == 0 || p == BAR));
    editor.edit.rulers = false;
    let off = render(&editor, &fixture, &document);
    let frame = editor.frame();
    assert!(pixels(&off, frame.ruler_top).iter().all(|&p| p == 0));
}

#[test]
fn the_ruler_marks_the_column_and_row_of_the_pointer() {
    let fixture = Fixture::new();
    let mut editor = editor();
    let document = document(64, 64);
    editor.edit.rulers = true;
    let frame = editor.frame();
    let view = editor.viewport(&frame, &document);
    let cell = view.cell_rect(frame.canvas, (4, 2));
    let at = Point::new((cell.x + 3) as u16, (cell.y + 3) as u16);
    let marked = render_with(&editor, Ok(fixture.source()), &document, Some(at));
    let plain = render_with(&editor, Ok(fixture.source()), &document, None);
    assert!(pixels(&marked, frame.ruler_top).contains(&ACCENT));
    assert!(!pixels(&plain, frame.ruler_top).contains(&ACCENT));
    assert!(pixels(&marked, frame.ruler_left).contains(&ACCENT));
}

#[test]
fn the_active_tool_plate_takes_the_body_color_and_the_accent_ring() {
    let fixture = Fixture::new();
    let mut editor = editor();
    let document = document(10, 10);
    let frame = editor.frame();
    for tool in Tool::ALL {
        editor.edit.tool = tool;
        let fb = render(&editor, &fixture, &document);
        for (other, rect) in frame.tools {
            let corner = fb.pixel(rect.x, rect.y);
            if other == tool {
                assert_eq!(corner, Some(ACCENT), "{tool:?} ring");
                assert_eq!(fb.pixel(rect.x + 1, rect.y + 1), Some(BACKGROUND));
            } else {
                assert_eq!(corner, Some(BAR), "{other:?} while {tool:?}");
            }
        }
    }
}

#[test]
fn the_select_and_hand_icons_are_drawn_on_their_plates() {
    let fixture = Fixture::new();
    let editor = editor();
    let document = document(10, 10);
    let fb = render(&editor, &fixture, &document);
    for (tool, rect) in editor.frame().tools {
        if matches!(tool, Tool::Select | Tool::Hand) {
            let inside = pixels(&fb, rect.inset(1));
            assert!(inside.contains(&OUTLINE), "{tool:?} outline");
            assert!(inside.contains(&DIM), "{tool:?} fill");
        }
    }
}

#[test]
fn a_cartridge_whose_tiles_cannot_be_shown_says_why_in_place_of_the_canvas() {
    let editor = editor();
    let document = document(10, 10);
    let fb = render_with(&editor, Err("bgtiles.bin is broken"), &document, None);
    let frame = editor.frame();
    assert!(pixels(&fb, frame.canvas_frame).contains(&ERROR));
    assert_eq!(
        fb.pixel(frame.tools[0].1.x, frame.tools[0].1.y),
        Some(ACCENT)
    );
}

#[test]
fn the_status_text_names_the_cell_the_selection_or_the_map() {
    let mut editor = editor();
    let mut document = document(30, 20);
    document.set_cell(BgLayer::Bg1, 3, 2, entry(7));
    let frame = editor.frame();
    let view = editor.viewport(&frame, &document);
    let cell = view.cell_rect(frame.canvas, (3, 2));
    let over = Point::new((cell.x + 3) as u16, (cell.y + 3) as u16);
    assert_eq!(
        editor.status_text(&document, Some(over)),
        "X 3 Y 2  BG1  Tile 0007"
    );
    assert_eq!(editor.status_text(&document, None), "30 x 20 tiles");
    editor.edit.selection = super::super::tools::Selection::between((1, 4), (3, 5), (30, 20));
    assert_eq!(
        editor.status_text(&document, None),
        "Selection 3x2  X 1 Y 4"
    );
    assert_eq!(
        editor.status_text(&document, Some(over)),
        "X 3 Y 2  BG1  Tile 0007",
        "the cell under the pointer comes first"
    );
    let past = view.cell_rect(frame.canvas, (31, 2));
    let at = Point::new((past.x + 3) as u16, (past.y + 3) as u16);
    editor.edit.selection = None;
    assert_eq!(editor.status_text(&document, Some(at)), "30 x 20 tiles");
}
