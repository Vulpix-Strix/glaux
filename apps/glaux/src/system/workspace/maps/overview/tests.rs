use pretty_assertions::assert_eq;

use super::super::fixture::{Fixture, at, document, editor, entry, input, key, press, render};
use super::super::tools::Tool;
use super::*;
use crate::cartridge::map::MapId;
use glaux_machine::tilemap::{BgBank, TileEntry};

fn open(editor: &mut MapEditor, document: &mut MapDocument) -> OverviewLayout {
    key(editor, document, 'm');
    layout(editor, document)
}

fn layout(editor: &MapEditor, document: &MapDocument) -> OverviewLayout {
    let map = map_pixels(document);
    let overview = editor.edit.overview.expect("the overview is open");
    OverviewLayout::new(
        &editor.frame(),
        map,
        overview.zoom(map),
        editor.metrics.cell_w,
    )
}

fn offset(editor: &MapEditor, document: &MapDocument) -> (i32, i32) {
    editor.viewport(&editor.frame(), document).offset()
}

#[test]
fn the_overview_shows_the_whole_map_at_the_closest_zoom_that_fits() {
    let tiles = |width: i32, height: i32| (width * 8, height * 8);
    assert_eq!(OverviewZoom::fit(tiles(16, 16)), OverviewZoom::X1);
    assert_eq!(OverviewZoom::fit(tiles(40, 30)), OverviewZoom::X2);
    assert_eq!(OverviewZoom::fit(tiles(64, 32)), OverviewZoom::X2);
    assert_eq!(OverviewZoom::fit(tiles(256, 256)), OverviewZoom::X16);
    for width in [1, 33, 64, 65, 129, 200, 256] {
        for height in [1, 16, 17, 64, 255, 256] {
            let map = tiles(width, height);
            let shown = OverviewZoom::fit(map).shown(map);
            assert!(shown.0 <= 256 && shown.1 <= 128, "{width}x{height}");
        }
    }
}

#[test]
fn zooming_in_stops_where_the_whole_map_still_fits() {
    let fit = OverviewZoom::X4;
    assert_eq!(OverviewZoom::X16.zoomed_in(fit), Some(OverviewZoom::X8));
    assert_eq!(OverviewZoom::X8.zoomed_in(fit), Some(OverviewZoom::X4));
    assert_eq!(OverviewZoom::X4.zoomed_in(fit), None);
    assert_eq!(OverviewZoom::X8.zoomed_out(), Some(OverviewZoom::X16));
    assert_eq!(OverviewZoom::X16.zoomed_out(), None);

    let mut editor = editor();
    let mut doc = document(64, 32);
    open(&mut editor, &mut doc);
    key(&mut editor, &mut doc, '+');
    assert_eq!(layout(&editor, &doc).map.width, 256);
    key(&mut editor, &mut doc, '-');
    assert_eq!(layout(&editor, &doc).map.width, 128);
}

#[test]
fn m_opens_and_closes_the_overview_keeping_the_editor_as_it_was() {
    let mut editor = editor();
    let mut doc = document(64, 32);
    editor.edit.tool = Tool::Fill;
    editor.edit.layer = BgLayer::Bg0;
    editor.edit.drawer = DrawerSize::Expanded;
    editor.edit.grid = false;
    editor.edit.visible = [true, true, false];
    key(&mut editor, &mut doc, '+');
    let before = editor.edit.clone();

    let layout = open(&mut editor, &mut doc);
    assert_eq!(editor.edit.drawer, DrawerSize::Collapsed);
    assert!(editor.claims_break());
    let overview = editor.edit.overview.unwrap();
    assert_eq!(overview.shown, [true, true, false], "starts as the eyes");
    press(&mut editor, &mut doc, middle_of(layout.layers[0]));
    assert_eq!(editor.edit.overview.unwrap().shown, [false, true, false]);
    assert_eq!(editor.edit.visible, [true, true, false], "the eyes stay");

    key(&mut editor, &mut doc, 'm');
    assert_eq!(editor.edit, before);

    open(&mut editor, &mut doc);
    input(&mut editor, &mut doc, InputEvent::Break);
    assert_eq!(editor.edit, before);
}

fn middle_of(rect: Rect) -> Point {
    at(rect.x + rect.width / 2, rect.y + rect.height / 2)
}

#[test]
fn the_viewport_is_measured_against_the_canvas_the_editor_returns_to() {
    let fixture = Fixture::new();
    for drawer in [DrawerSize::Compact, DrawerSize::Expanded] {
        let mut editor = editor();
        let mut doc = document(64, 32);
        editor.edit.drawer = drawer;
        let canvas = editor.frame().canvas;
        let layout = open(&mut editor, &mut doc);
        assert_ne!(
            editor.frame().canvas,
            canvas,
            "the overview collapses the drawer"
        );

        // At 2x a screen pixel of the canvas is half a map pixel, and the
        // overview shows one pixel for two.
        let (width, height) = (canvas.width / 4, canvas.height / 4);
        let fb = render(&editor, &fixture, &doc);
        let (x, y) = (layout.map.x, layout.map.y);
        assert_eq!(fb.pixel(x + width - 1, y), Some(ACCENT), "{drawer:?}");
        assert_eq!(fb.pixel(x + width, y), Some(BACKGROUND), "{drawer:?}");
        assert_eq!(fb.pixel(x, y + height - 1), Some(ACCENT), "{drawer:?}");
        assert_eq!(fb.pixel(x, y + height), Some(BACKGROUND), "{drawer:?}");

        let corner = at(layout.map.right() - 1, layout.map.bottom() - 1);
        press(&mut editor, &mut doc, corner);
        let last = (64 * 16 - canvas.width, 32 * 16 - canvas.height);
        key(&mut editor, &mut doc, 'm');
        assert_eq!(offset(&editor, &doc), last, "{drawer:?}");
    }
}

#[test]
fn a_click_centres_the_viewport_and_a_drag_keeps_where_it_was_grabbed() {
    let mut editor = editor();
    let mut doc = document(256, 256);
    let canvas = editor.frame().canvas;
    let layout = open(&mut editor, &mut doc);
    assert_eq!(layout.map.width, 128);

    // Block (64, 64) at 1/16 is centred on map pixel (1032, 1032).
    press(
        &mut editor,
        &mut doc,
        at(layout.map.x + 64, layout.map.y + 64),
    );
    let centred = (1032 * 2 - canvas.width / 2, 1032 * 2 - canvas.height / 2);
    assert_eq!(editor.edit.view.offset(), centred);

    input(
        &mut editor,
        &mut doc,
        InputEvent::PointerMove(Some(at(layout.map.x + 74, layout.map.y + 69))),
    );
    assert_eq!(
        editor.edit.view.offset(),
        (centred.0 + 10 * 16 * 2, centred.1 + 5 * 16 * 2)
    );
    super::super::fixture::release(&mut editor, &mut doc, at(0, 0));
    assert_eq!(editor.edit.overview.unwrap().drag, None);
}

#[test]
fn opening_another_map_refits_the_overview_and_forgets_the_drag() {
    let mut editor = editor();
    let mut doc = document(64, 32);
    editor.opened(doc.id());
    let layout = open(&mut editor, &mut doc);
    key(&mut editor, &mut doc, '-');
    press(
        &mut editor,
        &mut doc,
        at(layout.map.x + 40, layout.map.y + 20),
    );
    assert!(editor.edit.overview.unwrap().drag.is_some());

    let mut big = document(256, 256);
    editor.opened(&MapId::parse("forest").unwrap());
    let overview = editor.edit.overview.unwrap();
    assert_eq!(overview.zoom(map_pixels(&big)), OverviewZoom::X16);
    assert_eq!((overview.drag, overview.hover), (None, None));
    key(&mut editor, &mut big, 'm');
    assert_eq!(offset(&editor, &big), (0, 0));
}

fn composer_for(document: &MapDocument) -> Composer<'_> {
    Composer::new(
        BgLayer::ALL.map(|layer| document.layer(layer)),
        [true; BG_LAYERS],
    )
}

#[test]
fn a_tile_with_a_single_pixel_still_shows_when_the_overview_shrinks_it() {
    let fixture = Fixture::new();
    let mut doc = document(256, 256);
    // Tile 3 has one opaque pixel, local 5 in its top-left corner.
    doc.set_cell(BgLayer::Bg1, 5, 7, entry(3));
    let composer = composer_for(&doc);
    let mut colors = TileColors::new(fixture.bg.tiles());
    let tiles = map_size(&doc);
    let mut shown = |zoom, block| sample(&composer, &fixture.bg, &mut colors, zoom, tiles, block);
    assert_eq!(shown(OverviewZoom::X16, (2, 3)), Some(102));
    assert_eq!(shown(OverviewZoom::X8, (5, 7)), Some(102));
    assert_eq!(shown(OverviewZoom::X2, (20, 28)), Some(102));
    assert_eq!(shown(OverviewZoom::X2, (21, 28)), None);
    assert_eq!(shown(OverviewZoom::X16, (3, 3)), None);
}

#[test]
fn shrunk_blocks_show_the_layer_in_front_and_the_bank_of_its_cell() {
    let fixture = Fixture::new();
    let mut doc = document(16, 16);
    let bank1 = TileEntry::new(1, false, false, false, BgBank::new(1).unwrap()).unwrap();
    doc.set_cell(BgLayer::Bg2, 0, 0, bank1);
    doc.set_cell(BgLayer::Bg0, 0, 0, entry(2));
    doc.set_cell(BgLayer::Bg0, 1, 0, entry(2));
    let composer = composer_for(&doc);
    let front = composer.cell(0, 0).pixel(&fixture.bg, 4, 4);
    let mut colors = TileColors::new(fixture.bg.tiles());
    let tiles = map_size(&doc);
    for zoom in [OverviewZoom::X8, OverviewZoom::X16] {
        let block = sample(&composer, &fixture.bg, &mut colors, zoom, tiles, (0, 0));
        assert_eq!(block, front, "{zoom:?}");
    }
    assert_eq!(front, Some(103), "BG2 is in front at power-on, in BGP1");

    for y in 0..16 * 8 {
        for x in 0..16 * 8 {
            let exact = sample(
                &composer,
                &fixture.bg,
                &mut colors,
                OverviewZoom::X1,
                tiles,
                (x, y),
            );
            assert_eq!(
                exact,
                composer.cell(x / 8, y / 8).pixel(&fixture.bg, x % 8, y % 8)
            );
        }
    }
}
