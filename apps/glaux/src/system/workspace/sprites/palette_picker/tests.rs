use super::*;
use glaux_machine::font::FontMetrics;
use glaux_machine::framebuffer::{HEIGHT, WIDTH};
use glaux_machine::obj_palette::ObjPalettes;
use glaux_machine::palette::Palette;
use glaux_machine::sprite::{OBJ_TILE_BYTES, ObjTileAttributes, ObjTileBank, ObjVram};
use glaux_video::preview::ObjPreviewPalette;
use pretty_assertions::assert_eq;

use super::super::render::{CHECK_DARK, CHECK_LIGHT};
use crate::system::workspace::SYSTEM_COLORS;

const METRICS: Metrics = crate::system::workspace::tests_support::SPRITE_METRICS;

/// The tile the fixture's picker is on.
const TILE: usize = 3;

fn area() -> Rect {
    Rect {
        x: 0,
        y: METRICS.top,
        width: WIDTH as i32,
        height: HEIGHT as i32 - METRICS.top,
    }
}

fn layout(advanced: bool) -> PickerLayout {
    PickerLayout::new(area(), METRICS, advanced)
}

fn bank(n: u8) -> ObjBank {
    ObjBank::new(n).unwrap()
}

fn centre(rect: Rect) -> Point {
    Point::new(
        (rect.x + rect.width / 2) as u16,
        (rect.y + rect.height / 2) as u16,
    )
}

fn press(at: Point) -> InputEvent {
    InputEvent::PointerDown {
        button: PointerButton::Primary,
        at,
        select: false,
    }
}

fn release(at: Point) -> InputEvent {
    InputEvent::PointerUp {
        button: PointerButton::Primary,
        at,
    }
}

// Input.

#[test]
fn arrows_wrap_and_digits_choose_directly() {
    let layout = layout(false);
    let mut picker = PalettePicker::new(TILE, bank(0));
    assert_eq!(
        picker.handle_input(InputEvent::Up { select: false }, &layout),
        None
    );
    assert_eq!(picker.pending(), bank(7));
    picker.handle_input(InputEvent::Down { select: false }, &layout);
    picker.handle_input(InputEvent::Down { select: false }, &layout);
    assert_eq!(picker.pending(), bank(1));
    picker.handle_input(InputEvent::Text('5'), &layout);
    assert_eq!(picker.pending(), bank(5));
    for ignored in [
        InputEvent::Text('8'),
        InputEvent::Text('e'),
        InputEvent::Left { select: false },
        InputEvent::Undo,
        InputEvent::Save,
        InputEvent::NextView,
        InputEvent::Wheel { delta: 1 },
    ] {
        assert_eq!(picker.handle_input(ignored, &layout), None, "{ignored:?}");
    }
    assert_eq!(picker.pending(), bank(5));
}

#[test]
fn enter_applies_the_pending_bank_and_escape_cancels() {
    let layout = layout(false);
    let mut picker = PalettePicker::new(TILE, bank(4));
    picker.handle_input(InputEvent::Down { select: false }, &layout);
    assert_eq!(
        picker.handle_input(InputEvent::Enter, &layout),
        Some(PickerOutcome::Apply(bank(5)))
    );
    assert_eq!(
        picker.handle_input(InputEvent::Break, &layout),
        Some(PickerOutcome::Cancel)
    );
}

#[test]
fn pressing_a_row_chooses_it_and_buttons_act_on_release() {
    let layout = layout(true);
    let mut picker = PalettePicker::new(TILE, bank(0));
    assert_eq!(
        picker.handle_input(press(centre(layout.rows[6])), &layout),
        None
    );
    assert_eq!(
        picker.handle_input(release(centre(layout.rows[6])), &layout),
        None
    );
    assert_eq!(picker.pending(), bank(6));

    // A press outside the window is taken but does nothing.
    assert_eq!(
        picker.handle_input(press(Point::new(0, 239)), &layout),
        None
    );
    assert_eq!(picker.pending(), bank(6));

    // Pressing a button and releasing elsewhere chooses nothing.
    picker.handle_input(press(centre(layout.apply)), &layout);
    assert_eq!(
        picker.handle_input(release(centre(layout.cancel)), &layout),
        None
    );
    picker.handle_input(press(centre(layout.apply)), &layout);
    assert_eq!(
        picker.handle_input(release(centre(layout.apply)), &layout),
        Some(PickerOutcome::Apply(bank(6)))
    );
    picker.handle_input(press(centre(layout.cancel)), &layout);
    assert_eq!(
        picker.handle_input(release(centre(layout.cancel)), &layout),
        Some(PickerOutcome::Cancel)
    );
}

// Layout.

#[test]
fn the_dialog_fits_the_screen_in_both_modes() {
    for advanced in [false, true] {
        let layout = layout(advanced);
        let window = layout.window;
        let screen = area();
        assert!(
            window.x >= screen.x + crate::system::workspace::dialog::AREA_MARGIN
                && window.y >= screen.y
                && window.right() + SHADOW_OFFSET <= screen.right()
                && window.bottom() + SHADOW_OFFSET <= screen.bottom(),
            "{window:?} advanced={advanced}"
        );
        let inside = |rect: Rect| {
            rect.x > window.x
                && rect.y > layout.title_bar.bottom()
                && rect.right() < window.right()
                && rect.bottom() < window.bottom()
        };
        for row in layout.rows {
            assert!(inside(row), "{row:?}");
            // Marker, label, swatches, preview and tag, left to right.
            assert!(layout.label_x > layout.marker_x);
            assert!(layout.swatch_x > layout.label_x);
            let strip = layout.swatch_strip(row);
            let preview = layout.preview(row).inset(-1);
            assert!(strip.right() < preview.x);
            assert!(preview.right() < layout.tag_x);
            assert!(layout.tag_x + 7 * METRICS.cell_w <= row.right());
            assert!(preview.y >= row.y && preview.bottom() <= row.bottom());
            assert!(METRICS.cell_h <= row.height, "a text line fits its row");
        }
        for pair in layout.rows.windows(2) {
            assert_eq!(pair[0].bottom(), pair[1].y);
        }
        assert!(inside(layout.cancel) && inside(layout.apply));
        assert!(layout.cancel.right() < layout.apply.x);
        assert!(layout.rows[7].bottom() < layout.apply.y);
    }
    assert!(layout(true).window.width < layout(false).window.width);
}

// Rendering.

/// Tile 3, on OBP2, holds local 0 at (0, 0), 1 at (1, 0), 15 at (2, 0)
/// and 7 everywhere else. The banks are linear, so bank `n` resolves local
/// `l` to master `16 * n + l` and every bank shows different colors.
struct Fixture {
    obj: ObjVram,
    master: Palette,
    preview: ObjPreviewPalette,
}

impl Fixture {
    fn new() -> Self {
        Self::with_palettes(ObjPalettes::linear())
    }

    fn with_palettes(palettes: ObjPalettes) -> Self {
        let mut tiles = vec![0; (TILE + 1) * OBJ_TILE_BYTES];
        let tile = &mut tiles[TILE * OBJ_TILE_BYTES..];
        tile.fill(0x77);
        tile[0] = 0x01;
        tile[1] = 0xf7;
        let mut attributes = ObjTileAttributes::new();
        attributes.set_bank(TILE, bank(2));
        let obj = ObjVram::new(
            ObjTileBank::from_bytes(&tiles).unwrap(),
            attributes,
            palettes,
        );
        let master = Palette::default();
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

    /// The frame index the picker shows master `m` with.
    fn slot(&self, m: u8) -> u8 {
        self.preview.slot(m).unwrap()
    }
}

fn render(picker: &PalettePicker, fixture: &Fixture, pointer: Option<Point>) -> Framebuffer {
    let mut fb = Framebuffer::new();
    let font = BitmapFont::new(FontMetrics::SYSTEM);
    picker.render(&mut fb, &font, &layout(false), &fixture.source(), pointer);
    fb
}

#[test]
fn every_row_shows_its_bank_and_the_tile_through_it() {
    let fixture = Fixture::new();
    let fb = render(&PalettePicker::new(TILE, bank(2)), &fixture, None);
    let layout = layout(false);
    let at = |x: i32, y: i32| fb.pixel(x, y).unwrap();
    for (n, &row) in layout.rows.iter().enumerate() {
        let base = 16 * n as u8;
        for local in 1..16 {
            let swatch = layout.swatch(row, local);
            assert_eq!(
                at(swatch.x + 3, swatch.y + 5),
                fixture.slot(base + local),
                "bank {n} local {local}"
            );
        }
        let preview = layout.preview(row);
        // Each tile pixel is a 2x2 block.
        let block = |x: i32, y: i32| {
            let corner = at(preview.x + 2 * x, preview.y + 2 * y);
            let opposite = at(preview.x + 2 * x + 1, preview.y + 2 * y + 1);
            assert_eq!(corner, opposite, "bank {n} pixel ({x}, {y})");
            corner
        };
        assert!(
            [CHECK_DARK, CHECK_LIGHT].contains(&block(0, 0)),
            "local 0 shows the checkerboard in bank {n}"
        );
        assert_eq!(block(1, 0), fixture.slot(base + 1), "bank {n}");
        assert_eq!(block(2, 0), fixture.slot(base + 15), "bank {n}");
        assert_eq!(block(7, 7), fixture.slot(base + 7), "bank {n}");
    }
    assert_ne!(
        fixture.slot(1),
        fixture.slot(16 + 1),
        "banks look different"
    );
}

/// The System ROM banks share master colors between banks; every swatch
/// still shows its own bank's color.
#[test]
fn rom_default_banks_show_their_colors_in_every_row() {
    let rom = crate::rom::load().unwrap();
    let fixture = Fixture::with_palettes(rom.obj_palettes().clone());
    let fb = render(&PalettePicker::new(TILE, bank(2)), &fixture, None);
    let layout = layout(false);
    let shown = |x: i32, y: i32| fixture.preview.palette().get(fb.pixel(x, y).unwrap());
    for (n, &row) in layout.rows.iter().enumerate() {
        for local in 1..16 {
            let master = rom.obj_palettes().resolve(bank(n as u8), local).unwrap();
            let swatch = layout.swatch(row, local);
            let color = shown(swatch.x + 3, swatch.y + 5);
            assert_eq!(color, rom.palette().get(master), "bank {n} local {local}");
        }
    }
}

#[test]
fn the_pending_row_is_inverted_and_the_hovered_one_lightened() {
    let fixture = Fixture::new();
    let layout = layout(false);
    let band = |fb: &Framebuffer, n: usize| {
        let row = layout.rows[n];
        fb.pixel(row.x + 1, row.y + 1).unwrap()
    };
    let picker = PalettePicker::new(TILE, bank(5));
    let fb = render(&picker, &fixture, Some(centre(layout.rows[6])));
    assert_eq!(band(&fb, 5), INK, "pending");
    assert_eq!(band(&fb, 6), PALE, "hovered");
    assert_eq!(
        band(&fb, 2),
        BODY,
        "the current bank alone is not highlighted"
    );

    // Hovering the pending row keeps it inverted.
    let fb = render(&picker, &fixture, Some(centre(layout.rows[5])));
    assert_eq!(band(&fb, 5), INK);
}
