use super::*;
use pretty_assertions::assert_eq;

const METRICS: Metrics = Metrics {
    cell_w: 6,
    cell_h: 10,
    top: 16,
};

fn layouts() -> Vec<SpriteLayout> {
    [false, true]
        .into_iter()
        .flat_map(|advanced| {
            [Zoom::X1, Zoom::X2].map(|zoom| SpriteLayout::new(METRICS, advanced, zoom))
        })
        .collect()
}

fn centre(rect: Rect) -> Point {
    Point::new(
        (rect.x + rect.width / 2) as u16,
        (rect.y + rect.height / 2) as u16,
    )
}

/// Every rectangle the editor hit-tests or fills, by name.
fn regions(layout: &SpriteLayout) -> Vec<(String, Rect)> {
    let mut out = vec![
        ("options".into(), layout.options),
        ("status".into(), layout.status),
        ("palette".into(), layout.palette),
        ("canvas".into(), layout.canvas_frame),
        ("sheet".into(), layout.sheet.inset(-1)),
        ("save".into(), layout.save_button),
        ("import".into(), layout.import_button),
        ("palette button".into(), layout.palette_button),
    ];
    out.extend(layout.tools.iter().map(|(t, r)| (format!("{t:?}"), *r)));
    out
}

#[test]
fn panels_fit_the_screen_without_overlapping() {
    for layout in layouts() {
        let regions = regions(&layout);
        for (name, rect) in &regions {
            assert!(
                rect.x >= 0
                    && rect.y >= METRICS.top
                    && rect.right() <= WIDTH as i32
                    && rect.bottom() <= HEIGHT as i32,
                "{name} {rect:?} advanced={}",
                layout.advanced
            );
        }
        for (i, (a_name, a)) in regions.iter().enumerate() {
            for (b_name, b) in &regions[i + 1..] {
                assert!(!a.overlaps(*b), "{a_name} {a:?} overlaps {b_name} {b:?}");
            }
        }
    }
}

#[test]
fn the_canvas_shows_the_tile_at_twelve_x() {
    for layout in layouts() {
        assert_eq!((layout.canvas.width, layout.canvas.height), (96, 96));
        assert!(layout.caption_y + METRICS.cell_h <= layout.palette.y);
    }
}

#[test]
fn one_x_shows_all_sixteen_columns_and_two_x_pans() {
    let normal = SpriteLayout::new(METRICS, false, Zoom::X1);
    assert_eq!(normal.visible, Visible::new(16, 22));
    let zoomed = SpriteLayout::new(METRICS, false, Zoom::X2);
    assert_eq!(zoomed.visible, Visible::new(11, 11));
    let advanced = SpriteLayout::new(METRICS, true, Zoom::X1);
    assert_eq!(advanced.visible, Visible::new(16, 18));
}

#[test]
fn canvas_pixels_continue_past_the_canvas_edges() {
    let layout = SpriteLayout::new(METRICS, false, Zoom::X1);
    let at = |dx: i32, dy: i32| {
        layout.canvas_pixel(Point::new(
            (layout.canvas.x + dx) as u16,
            (layout.canvas.y + dy) as u16,
        ))
    };
    assert_eq!(at(0, 0), (0, 0));
    assert_eq!(at(95, 95), (7, 7));
    assert_eq!(at(96, 0), (8, 0));
    assert_eq!(at(-1, -1), (-1, -1));
    assert_eq!(at(30, -12), (2, -1));
    assert_eq!(at(30, -13), (2, -2));
}

#[test]
fn options_buttons_fit_beside_the_tool_tag() {
    for layout in layouts() {
        let buttons = [
            layout.grid_button,
            layout.advanced_button,
            layout.zoom_out,
            layout.zoom_label,
            layout.zoom_in,
        ];
        for pair in buttons.windows(2) {
            assert!(pair[0].right() <= pair[1].x, "{pair:?}");
        }
        // "Tile 1023", the longest tool name and "MODIFIED", with their
        // padding.
        let cells = 9 + 1 + 10 + 1 + 8;
        assert!(layout.grid_button.x >= MARGIN + cells * METRICS.cell_w + 4);
    }
}

#[test]
fn every_region_hits_what_it_shows() {
    let view = SheetView::default();
    for layout in layouts() {
        assert_eq!(
            layout.hit(centre(layout.grid_button), &view),
            Hit::GridButton
        );
        assert_eq!(
            layout.hit(centre(layout.advanced_button), &view),
            Hit::AdvancedButton
        );
        assert_eq!(layout.hit(centre(layout.zoom_out), &view), Hit::ZoomOut);
        assert_eq!(layout.hit(centre(layout.zoom_in), &view), Hit::ZoomIn);
        assert_eq!(layout.hit(centre(layout.zoom_label), &view), Hit::None);
        assert_eq!(
            layout.hit(centre(layout.palette_button), &view),
            Hit::PaletteButton
        );
        assert_eq!(
            layout.hit(centre(layout.save_button), &view),
            Hit::SaveButton
        );
        assert_eq!(
            layout.hit(centre(layout.import_button), &view),
            Hit::ImportButton
        );
        for (tool, rect) in layout.tools {
            assert_eq!(layout.hit(centre(rect), &view), Hit::Tool(tool));
        }
        for (local, rect) in layout.swatches.iter().enumerate() {
            assert_eq!(layout.hit(centre(*rect), &view), Hit::Swatch(local as u8));
        }
        for (bank, rect) in layout.banks.iter().enumerate() {
            let expected = if layout.advanced {
                Hit::Bank(bank as u8)
            } else {
                Hit::Swatch(bank as u8)
            };
            assert_eq!(layout.hit(centre(*rect), &view), expected);
        }
        assert_eq!(layout.hit(Point::new(0, 239), &view), Hit::None);
    }
}

#[test]
fn canvas_points_map_to_tile_pixels() {
    let layout = SpriteLayout::new(METRICS, false, Zoom::X1);
    let view = SheetView::default();
    let at = |dx: i32, dy: i32| {
        let point = Point::new((layout.canvas.x + dx) as u16, (layout.canvas.y + dy) as u16);
        layout.hit(point, &view)
    };
    assert_eq!(at(0, 0), Hit::Canvas { x: 0, y: 0 });
    assert_eq!(at(11, 11), Hit::Canvas { x: 0, y: 0 });
    assert_eq!(at(12, 0), Hit::Canvas { x: 1, y: 0 });
    assert_eq!(at(3 * 12 + 5, 5 * 12), Hit::Canvas { x: 3, y: 5 });
    assert_eq!(at(95, 95), Hit::Canvas { x: 7, y: 7 });
    assert_eq!(at(96, 0), Hit::None, "the frame");
}

#[test]
fn sheet_points_map_to_scrolled_tiles() {
    for zoom in [Zoom::X1, Zoom::X2] {
        let layout = SpriteLayout::new(METRICS, false, zoom);
        let mut view = SheetView::default();
        view.set_zoom(zoom, layout.visible, 0);
        let cell = |column, row| centre(layout.sheet_cell(column, row, zoom));
        assert_eq!(layout.hit(cell(0, 0), &view), Hit::Sheet(0));
        assert_eq!(layout.hit(cell(3, 2), &view), Hit::Sheet(35));

        view.scroll_rows(5, layout.visible);
        view.scroll_columns(2, layout.visible);
        let left = if zoom == Zoom::X2 { 2 } else { 0 };
        assert_eq!(
            layout.hit(cell(3, 2), &view),
            Hit::Sheet(7 * 16 + 3 + left),
            "{zoom:?}"
        );
        let last = cell(layout.visible.columns - 1, layout.visible.rows - 1);
        assert!(matches!(layout.hit(last, &view), Hit::Sheet(_)));
    }
}

#[test]
fn advanced_mode_keeps_the_main_layout() {
    let normal = SpriteLayout::new(METRICS, false, Zoom::X1);
    let advanced = SpriteLayout::new(METRICS, true, Zoom::X1);
    assert_eq!(advanced.canvas, normal.canvas);
    assert_eq!(advanced.sheet.x, normal.sheet.x);
    assert_eq!(advanced.tools, normal.tools);
    assert_eq!(advanced.options, normal.options);
    assert_eq!(advanced.palette_button, normal.palette_button);
    assert!(advanced.palette.height > normal.palette.height);
}

#[test]
fn save_and_import_split_the_row_under_the_caption() {
    for layout in layouts() {
        let (save, import) = (layout.save_button, layout.import_button);
        assert_eq!(save.y, import.y);
        assert_eq!(save.height, import.height);
        assert_eq!(save.width, import.width);
        assert_eq!(save.x, layout.canvas_frame.x);
        assert_eq!(import.right(), layout.canvas_frame.right());
        assert!(save.right() < import.x, "a gap separates them");
        assert_eq!(layout.palette_button.width, layout.canvas_frame.width);
        assert!(layout.palette_button.y > save.bottom());
        for label in [SAVE_LABEL, IMPORT_LABEL] {
            assert!(
                text_cells(label) * METRICS.cell_w + 2 * BUTTON_PAD <= save.width,
                "{label} fits"
            );
        }
    }
}
