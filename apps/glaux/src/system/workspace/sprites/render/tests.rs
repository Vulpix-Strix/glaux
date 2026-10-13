use super::*;
use glaux_machine::font::FontMetrics;
use glaux_machine::obj_palette::{OBJ_PALETTES_BYTES, ObjPalettes};
use glaux_machine::palette::Palette;
use glaux_machine::sprite::{
    OBJ_TILE_BYTES, OBJ_TILE_CAPACITY, ObjTileAttributes, ObjTileBank, ObjVram,
};
use glaux_video::preview::ObjPreviewPalette;
use pretty_assertions::assert_eq;

use super::super::Metrics;
use crate::system::workspace::SYSTEM_COLORS;

const METRICS: Metrics = Metrics {
    cell_w: 6,
    cell_h: 10,
    top: 16,
};

/// Tile 0 is solid local 2 and tile 1 is empty. OBP0 maps local `n` to
/// master `200 + n`, so frame slots and master indices differ.
struct Fixture {
    obj: ObjVram,
    master: Palette,
    preview: ObjPreviewPalette,
}

impl Fixture {
    fn new() -> Self {
        let mut tiles = vec![0; OBJ_TILE_CAPACITY * OBJ_TILE_BYTES];
        tiles[..OBJ_TILE_BYTES].fill(0x22);
        let mut obp = [0; OBJ_PALETTES_BYTES];
        for (local, entry) in obp.iter_mut().enumerate().take(16).skip(1) {
            *entry = 200 + local as u8;
        }
        let obj = ObjVram::new(
            ObjTileBank::from_bytes(&tiles).unwrap(),
            ObjTileAttributes::new(),
            ObjPalettes::from_bytes(&obp).unwrap(),
        );
        let mut entries = *Palette::default().entries();
        entries[202] = Rgb::from_hex(0x62a7d5);
        let master = Palette::new(entries);
        let preview =
            ObjPreviewPalette::new(&Palette::default(), SYSTEM_COLORS, &master, obj.palettes())
                .unwrap();
        Self {
            obj,
            master,
            preview,
        }
    }

    fn source(&self) -> SpriteSource<'_> {
        SpriteSource {
            obj: &self.obj,
            master: &self.master,
            preview: &self.preview,
            modified: false,
        }
    }
}

fn render(editor: &SpriteEditor, fixture: &Fixture) -> Framebuffer {
    let mut fb = Framebuffer::new();
    let font = BitmapFont::new(FontMetrics::SYSTEM);
    editor.render(
        &mut fb,
        &font,
        &SystemUi::default(),
        Ok(fixture.source()),
        None,
        None,
    );
    fb
}

fn pixels(fb: &Framebuffer, rect: Rect) -> Vec<u8> {
    (rect.y..rect.bottom())
        .flat_map(|y| (rect.x..rect.right()).map(move |x| (x, y)))
        .map(|(x, y)| fb.pixel(x, y).unwrap())
        .collect()
}

#[test]
fn the_canvas_shows_the_tile_in_its_cartridge_color() {
    let fixture = Fixture::new();
    let editor = SpriteEditor::new(METRICS);
    let fb = render(&editor, &fixture);
    let canvas = editor.layout().canvas;
    let slot = fixture.preview.slot(202).unwrap();
    assert_eq!(fixture.preview.palette().get(slot), Rgb::from_hex(0x62a7d5));
    // Inside the first pixel, clear of the grid lines.
    assert_eq!(fb.pixel(canvas.x + 5, canvas.y + 5), Some(slot));
    assert_eq!(
        fb.pixel(canvas.right() - 1, canvas.bottom() - 1),
        Some(slot)
    );
}

#[test]
fn transparent_pixels_show_a_checkerboard() {
    let fixture = Fixture::new();
    let mut editor = SpriteEditor::new(METRICS);
    editor.selected = 1;
    editor.grid = false;
    let fb = render(&editor, &fixture);
    let canvas = pixels(&fb, editor.layout().canvas);
    assert!(canvas.contains(&CHECK_DARK) && canvas.contains(&CHECK_LIGHT));
    assert!(canvas.iter().all(|&p| p == CHECK_DARK || p == CHECK_LIGHT));
}

#[test]
fn empty_tiles_stay_visible_in_the_sheet() {
    let fixture = Fixture::new();
    let editor = SpriteEditor::new(METRICS);
    let fb = render(&editor, &fixture);
    let layout = editor.layout();
    let zoom = editor.sheet.zoom();
    let slot = fixture.preview.slot(202).unwrap();
    assert!(
        pixels(&fb, layout.sheet_cell(0, 0, zoom))
            .iter()
            .all(|&p| p == slot)
    );
    // The last visible tile is empty capacity, drawn as a checkerboard.
    let last = layout.sheet_cell(layout.visible.columns - 1, layout.visible.rows - 1, zoom);
    let empty = pixels(&fb, last);
    assert!(empty.contains(&CHECK_DARK) && empty.contains(&CHECK_LIGHT));
    assert!(!empty.contains(&slot));
}

#[test]
fn the_grid_toggle_only_changes_grid_lines() {
    let fixture = Fixture::new();
    let mut editor = SpriteEditor::new(METRICS);
    let with_grid = render(&editor, &fixture);
    editor.grid = false;
    let without = render(&editor, &fixture);
    let canvas = editor.layout().canvas;
    let mut changed = 0;
    for y in 0..240 {
        for x in 0..320 {
            if with_grid.pixel(x, y) == without.pixel(x, y) {
                continue;
            }
            changed += 1;
            let on_line = (x - canvas.x) % CANVAS_ZOOM == 0 || (y - canvas.y) % CANVAS_ZOOM == 0;
            assert!(canvas.contains_xy(x, y) && on_line, "({x}, {y})");
            assert_eq!(with_grid.pixel(x, y), Some(GRID_LINE));
        }
    }
    assert!(changed > 0);
}

#[test]
fn advanced_text_names_cartridge_master_indices_not_frame_slots() {
    let fixture = Fixture::new();
    let source = fixture.source();
    let slot = fixture.preview.slot(202).unwrap();
    assert_ne!(slot, 202);
    assert_eq!(
        selection_text(&source, ObjBank::ZERO, 2),
        "Selected: Local 2 -> Master 202 ($CA) -> #62A7D5"
    );
    assert_eq!(
        selection_text(&source, ObjBank::ZERO, 0),
        "Selected: Local 0 -> transparent"
    );
    assert_eq!(master_label(&source, ObjBank::ZERO, 2), "CA");
    assert_eq!(master_label(&source, ObjBank::ZERO, 0), "--");
}

#[test]
fn advanced_mode_adds_rows_without_moving_the_canvas() {
    let fixture = Fixture::new();
    let mut editor = SpriteEditor::new(METRICS);
    let normal = render(&editor, &fixture);
    editor.advanced = true;
    let advanced = render(&editor, &fixture);
    let canvas = editor.layout().canvas;
    let moved = pixels(&normal, canvas)
        .into_iter()
        .zip(pixels(&advanced, canvas))
        .filter(|(a, b)| a != b)
        .count();
    assert_eq!(moved, 0, "canvas pixels that changed");
    // The OBP row shows the tile's bank highlighted.
    let bank0 = editor.layout().banks[0];
    assert!(pixels(&advanced, bank0).contains(&PALE));
}

#[test]
fn status_reports_the_hovered_pixel_and_tile_coordinates() {
    let fixture = Fixture::new();
    let mut editor = SpriteEditor::new(METRICS);
    let source = fixture.source();
    let (left, _) = editor.status(Some(&source), Hit::Canvas { x: 3, y: 5 });
    assert_eq!(left, "Tile 0000 | X:3 Y:5 | Color:2");
    editor.selected = 12;
    editor.advanced = true;
    let (left, right) = editor.status(Some(&source), Hit::Sheet(1023));
    assert_eq!(left, "Tile 0012 (12,0) | OBP 0 | 4 bpp");
    assert_eq!(right, "Click: select tile 1023 (15,63)");
    let (_, right) = editor.status(Some(&source), Hit::Tool(ToolSlot::Eraser));
    assert_eq!(right, "E: Eraser");
    editor.tool = ToolSlot::Fill;
    let (_, right) = editor.status(Some(&source), Hit::Canvas { x: 0, y: 0 });
    assert_eq!(right, "Click: fill the area");
}

#[test]
fn a_written_pixel_shows_in_the_canvas_and_the_sheet_in_the_next_frame() {
    let mut fixture = Fixture::new();
    let editor = SpriteEditor::new(METRICS);
    fixture.obj.tiles_mut().set_pixel(0, 7, 7, 5);
    let fb = render(&editor, &fixture);
    let layout = editor.layout();
    let slot = fixture.preview.slot(205).unwrap();
    let canvas = layout.canvas;
    assert_eq!(
        fb.pixel(canvas.right() - 2, canvas.bottom() - 2),
        Some(slot)
    );
    let cell = layout.sheet_cell(0, 0, editor.sheet.zoom());
    assert_eq!(fb.pixel(cell.right() - 1, cell.bottom() - 1), Some(slot));
    assert_ne!(fb.pixel(cell.x, cell.y), Some(slot));
}

#[test]
fn only_the_active_tool_is_drawn_active() {
    let fixture = Fixture::new();
    let mut editor = SpriteEditor::new(METRICS);
    editor.tool = ToolSlot::Fill;
    let fb = render(&editor, &fixture);
    for (tool, rect) in editor.layout().tools {
        let active = pixels(&fb, rect).contains(&ACCENT);
        assert_eq!(active, tool == ToolSlot::Fill, "{tool:?}");
    }
}

/// A system-sized font whose glyphs spell out their character in their
/// five leftmost columns: a solid marker row, a blank row, then the
/// character code in two rows.
fn coded_font() -> BitmapFont {
    let metrics = FontMetrics::SYSTEM;
    let mut font = BitmapFont::new(metrics);
    let spacing = metrics.cell_width() - 5;
    for ch in ' '..='~' {
        let code = ch as u16;
        let mut rows = vec![0; usize::from(metrics.cell_height())];
        rows[0] = 0b11111 << spacing;
        rows[2] = (code & 0b11111) << spacing;
        rows[3] = (code >> 5) << spacing;
        font.insert(ch, 1, &rows).unwrap();
    }
    font
}

/// The text drawn in `ink` inside `rect` with [`coded_font`], one string
/// per run of adjacent cells, top to bottom and left to right.
fn texts(fb: &Framebuffer, rect: Rect, ink: u8) -> Vec<String> {
    let is_ink = |x: i32, y: i32| fb.pixel(x, y) == Some(ink);
    let row =
        |x: i32, y: i32| (0..5).fold(0u32, |bits, dx| bits << 1 | u32::from(is_ink(x + dx, y)));
    let mut runs: Vec<(i32, i32, String)> = Vec::new();
    for y in rect.y..rect.bottom() {
        for x in rect.x..rect.right() {
            let marker = row(x, y) == 0b11111 && !is_ink(x - 1, y) && !is_ink(x + 5, y);
            if !marker || row(x, y + 1) != 0 {
                continue;
            }
            let ch = char::from_u32(row(x, y + 2) | row(x, y + 3) << 5).unwrap();
            match runs.last_mut() {
                Some((run_y, end, text)) if *run_y == y && x - *end <= METRICS.cell_w => {
                    text.push(ch);
                    *end = x;
                }
                _ => runs.push((y, x, ch.to_string())),
            }
        }
    }
    runs.into_iter().map(|(.., text)| text).collect()
}

#[test]
fn the_options_bar_names_the_tool_and_flags_unsaved_edits() {
    let fixture = Fixture::new();
    let mut editor = SpriteEditor::new(METRICS);
    let bar = editor.layout().options;
    let draw = |editor: &SpriteEditor, modified| {
        let mut fb = Framebuffer::new();
        let source = SpriteSource {
            modified,
            ..fixture.source()
        };
        let ui = SystemUi::default();
        editor.render(&mut fb, &coded_font(), &ui, Ok(source), None, None);
        fb
    };

    let clean = draw(&editor, false);
    assert_eq!(texts(&clean, bar, TEXT), ["Tile 0000", "1x"]);
    assert_eq!(texts(&clean, bar, PALE), ["Pencil"]);
    // The grid shows by default, so its button is accented too.
    assert_eq!(texts(&clean, bar, ACCENT), ["Grid"]);

    let modified = draw(&editor, true);
    assert_eq!(texts(&modified, bar, ACCENT), ["MODIFIED", "Grid"]);
    assert_eq!(texts(&modified, bar, PALE), ["Pencil"]);

    editor.tool = ToolSlot::Eyedropper;
    assert_eq!(texts(&draw(&editor, false), bar, PALE), ["Eyedropper"]);
}

#[test]
fn long_messages_wrap_at_spaces() {
    assert_eq!(
        wrap(
            &crate::system::test_font::LAYOUT_FONT,
            "Invalid obp.bin: must be exactly 128 bytes",
            16
        ),
        ["Invalid obp.bin:", "must be exactly", "128 bytes"]
    );
    assert_eq!(
        wrap(&crate::system::test_font::LAYOUT_FONT, "abcdefghij", 4),
        ["abcd", "efgh", "ij"]
    );
}
