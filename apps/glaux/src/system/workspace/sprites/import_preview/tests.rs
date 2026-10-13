use super::*;
use crate::sprite_import::RgbaImage;
use crate::system::video::VideoFiles;
use crate::system::workspace::Rect;
use pretty_assertions::assert_eq;

const METRICS: Metrics = Metrics {
    cell_w: 6,
    cell_h: 10,
    top: 16,
};

fn area(metrics: Metrics) -> Rect {
    Rect {
        x: 0,
        y: metrics.top,
        width: 320,
        height: 240 - metrics.top,
    }
}

fn defaults() -> CartridgeVideoDefaults {
    CartridgeVideoDefaults::from_rom(&crate::rom::load().unwrap())
}

fn document() -> SpriteDocument {
    SpriteDocument::from_files(&VideoFiles::default(), &defaults()).unwrap()
}

/// An 8x8 sheet, solid red but for a transparent top-left pixel.
fn preview(document: &SpriteDocument) -> ImportPreview {
    let mut rgba: Vec<u8> = [200, 10, 10, 255].repeat(64);
    rgba[3] = 0;
    let source = SheetSource {
        image: RgbaImage {
            width: 8,
            height: 8,
            rgba,
        },
        groups: Vec::new(),
        groups_path: None,
    };
    ImportPreview::new(
        "sheet.png".into(),
        source,
        document,
        defaults(),
        Palette::default(),
    )
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
    }
}

fn release(at: Point) -> InputEvent {
    InputEvent::PointerUp {
        button: PointerButton::Primary,
        at,
    }
}

#[test]
fn the_parts_fit_the_window_without_overlapping() {
    let small = Metrics {
        cell_w: 4,
        cell_h: 5,
        top: 16,
    };
    for metrics in [METRICS, small] {
        let layout = ImportLayout::new(area(metrics), metrics);
        let notes = Rect {
            x: layout.text_x,
            y: layout.notes_y,
            width: layout.text_right - layout.text_x,
            height: NOTE_LINES as i32 * metrics.cell_h,
        };
        let parts = [
            layout.preview.inset(-1),
            layout.transparency,
            notes,
            layout.cancel,
            layout.import,
        ];
        let inside = layout.window.inset(1);
        for (i, a) in parts.iter().enumerate() {
            assert!(
                a.x >= inside.x
                    && a.y >= layout.title_bar.bottom()
                    && a.right() <= inside.right()
                    && a.bottom() <= inside.bottom(),
                "{a:?} inside {inside:?}"
            );
            for b in &parts[i + 1..] {
                assert!(!a.overlaps(*b), "{a:?} overlaps {b:?}");
            }
        }
        assert_eq!(layout.preview.width, 128, "a whole grid row at 1x");
    }
}

#[test]
fn the_largest_impact_fits_its_lines() {
    let layout = ImportLayout::new(area(METRICS), METRICS);
    let notes = impact_notes(&ReplaceImpact {
        tiles_changed: 1024,
        drawn_tiles_erased: 1024,
        attributes_changed: 1024,
        banks_changed: 8,
        masters_changed: 224,
        master_range: Some((32, 255)),
    });
    let lines: usize = notes
        .iter()
        .map(|note| {
            wrap(
                &crate::system::test_font::LAYOUT_FONT,
                note,
                layout.note_columns(),
            )
            .len()
        })
        .sum();
    assert!(lines <= NOTE_LINES, "{lines} lines: {notes:?}");
}

#[test]
fn the_notes_name_tile_and_color_numbers_code_may_draw() {
    let mut impact = ReplaceImpact {
        tiles_changed: 3,
        ..ReplaceImpact::default()
    };
    let notes = impact_notes(&impact).join(" ");
    assert!(notes.contains("draws tiles by number"), "{notes}");
    assert!(!notes.contains("colors"), "{notes}");

    impact.masters_changed = 2;
    impact.master_range = Some((40, 41));
    let notes = impact_notes(&impact).join(" ");
    assert!(notes.contains("master colors 40-41"), "{notes}");
    assert!(notes.contains("colors 40-41, by number"), "{notes}");
}

#[test]
fn a_button_acts_only_when_pressed_and_released_on_it() {
    let document = document();
    let mut preview = preview(&document);
    let layout = ImportLayout::new(area(METRICS), METRICS);
    let (cancel, import) = (centre(layout.cancel), centre(layout.import));
    assert_eq!(
        preview.handle_input(press(import), &layout, &document),
        None
    );
    assert_eq!(
        preview.handle_input(release(cancel), &layout, &document),
        None
    );
    preview.handle_input(press(cancel), &layout, &document);
    assert_eq!(
        preview.handle_input(release(cancel), &layout, &document),
        Some(ImportOutcome::Cancel)
    );
    preview.handle_input(press(import), &layout, &document);
    assert!(matches!(
        preview.handle_input(release(import), &layout, &document),
        Some(ImportOutcome::Import(_))
    ));
}

#[test]
fn a_transparent_top_left_pixel_leaves_no_color_to_key_out() {
    let document = document();
    let mut preview = preview(&document);
    let layout = ImportLayout::new(area(METRICS), METRICS);
    preview.handle_input(InputEvent::Text('t'), &layout, &document);
    assert!(!preview.is_keyed());
    assert_eq!(preview.summary().unwrap().colors, 1);
}

#[test]
fn enter_imports_the_conversion_and_escape_cancels() {
    let document = document();
    let mut preview = preview(&document);
    let layout = ImportLayout::new(area(METRICS), METRICS);
    let Some(ImportOutcome::Import(video)) =
        preview.handle_input(InputEvent::Enter, &layout, &document)
    else {
        panic!("Enter imports");
    };
    assert_eq!(video.obj.tiles().pixel(0, 0, 0), Some(0));
    assert_eq!(
        video.obj.tiles().pixel(0, 1, 0).map(|local| {
            let master = video
                .obj
                .palettes()
                .resolve(video.obj.attributes().bank(0), local);
            master.map(|m| video.palette.get(m))
        }),
        Some(Some(glaux_machine::palette::Rgb::from_hex(0xc80a0a)))
    );
    assert_eq!(
        preview.handle_input(InputEvent::Break, &layout, &document),
        Some(ImportOutcome::Cancel)
    );
}
