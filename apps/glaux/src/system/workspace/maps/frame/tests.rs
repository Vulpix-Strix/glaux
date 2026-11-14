use super::*;
use glaux_machine::font::FontMetrics;
use glaux_machine::framebuffer::HEIGHT;
use pretty_assertions::assert_eq;

fn metrics() -> Metrics {
    super::super::super::tab::Layout::new(FontMetrics::SYSTEM).map_metrics()
}

fn header() -> Layout {
    let metrics = metrics();
    let widths = ["NEW", "RENAME", "DELETE", "SAVE"].map(|label| (label.len() as i32 + 2) * 6);
    Layout::new(metrics, widths)
}

fn frame_for(size: DrawerSize, rulers: bool) -> Frame {
    Frame::new(metrics(), &header(), size, rulers, 2)
}

const SIZES: [DrawerSize; 3] = [
    DrawerSize::Collapsed,
    DrawerSize::Compact,
    DrawerSize::Expanded,
];

fn inside(inner: Rect, outer: Rect) -> bool {
    inner.x >= outer.x
        && inner.y >= outer.y
        && inner.right() <= outer.right()
        && inner.bottom() <= outer.bottom()
}

fn point(x: i32, y: i32) -> Point {
    Point::new(x as u16, y as u16)
}

fn middle(rect: Rect) -> Point {
    point(rect.x + rect.width / 2, rect.y + rect.height / 2)
}

#[test]
fn the_drawer_cycles_through_its_three_sizes_in_both_directions() {
    let mut size = DrawerSize::default();
    assert_eq!(size, DrawerSize::Compact);
    let forward: Vec<DrawerSize> = (0..4)
        .map(|_| {
            size = size.cycled(false);
            size
        })
        .collect();
    assert_eq!(
        forward,
        [
            DrawerSize::Expanded,
            DrawerSize::Collapsed,
            DrawerSize::Compact,
            DrawerSize::Expanded
        ]
    );
    for size in SIZES {
        assert_eq!(size.cycled(false).cycled(true), size);
    }
    assert_eq!(DrawerSize::Collapsed.strip_rows(), 0);
    assert!(DrawerSize::Compact.strip_rows() < DrawerSize::Expanded.strip_rows());
}

#[test]
fn every_part_fits_the_screen_and_the_body_at_the_system_font() {
    let body = header().body();
    let screen = Rect {
        x: 0,
        y: 0,
        width: 320,
        height: HEIGHT as i32,
    };
    for size in SIZES {
        for rulers in [false, true] {
            let frame = frame_for(size, rulers);
            let what = format!("{size:?}, rulers {rulers}");
            for (name, rect) in [
                ("options", frame.options),
                ("canvas frame", frame.canvas_frame),
                ("scroll v", frame.scroll_v),
                ("scroll h", frame.scroll_h),
                ("drawer", frame.drawer),
            ] {
                assert!(inside(rect, body), "{name}: {rect:?}, {what}");
            }
            for (tool, rect) in frame.tools {
                assert!(inside(rect, screen), "{tool:?}: {rect:?}, {what}");
            }
            assert!(
                frame.canvas.width >= 160 && frame.canvas.height >= 40,
                "{what}"
            );
        }
    }
}

#[test]
fn the_parts_do_not_overlap() {
    for size in SIZES {
        for rulers in [false, true] {
            let frame = frame_for(size, rulers);
            let mut parts = vec![
                ("options", frame.options),
                ("canvas", frame.canvas_frame),
                ("scroll v", frame.scroll_v),
                ("scroll h", frame.scroll_h),
                ("drawer", frame.drawer),
            ];
            if rulers {
                parts.push(("ruler top", frame.ruler_top));
                parts.push(("ruler left", frame.ruler_left));
            }
            for (tool, rect) in frame.tools {
                parts.push((Box::leak(format!("{tool:?}").into_boxed_str()), rect));
            }
            for (i, (a, ra)) in parts.iter().enumerate() {
                for (b, rb) in &parts[i + 1..] {
                    assert!(!ra.overlaps(*rb), "{a} {ra:?} and {b} {rb:?}, {size:?}");
                }
            }
        }
    }
}

#[test]
fn the_options_buttons_sit_in_the_bar_without_overlapping() {
    let frame = frame_for(DrawerSize::Compact, false);
    let buttons = [
        frame.grid_button,
        frame.rulers_button,
        frame.zoom_out,
        frame.zoom_label,
        frame.zoom_in,
    ];
    for (i, a) in buttons.iter().enumerate() {
        assert!(inside(*a, frame.options), "{a:?}");
        for b in &buttons[i + 1..] {
            assert!(!a.overlaps(*b), "{a:?} {b:?}");
        }
    }
    assert!(frame.grid_button.x >= 4 + 30 * 6 / 2);
}

#[test]
fn the_rulers_take_room_from_the_canvas_only_while_they_are_on() {
    let off = frame_for(DrawerSize::Compact, false);
    let on = frame_for(DrawerSize::Compact, true);
    assert_eq!(off.ruler_top.height * off.ruler_left.width, 0);
    assert_eq!(on.ruler_top.height, 12);
    assert_eq!(on.ruler_left.width, 18);
    assert_eq!(on.canvas.width, off.canvas.width - 18);
    assert_eq!(on.canvas.height, off.canvas.height - 12);
    assert_eq!(on.ruler_top.x, on.canvas.x);
    assert_eq!(on.ruler_left.y, on.canvas.y);
}

#[test]
fn a_taller_drawer_leaves_the_canvas_less_room_and_never_less_than_a_few_tiles() {
    let heights: Vec<i32> = SIZES
        .map(|size| frame_for(size, true).canvas.height)
        .to_vec();
    assert!(
        heights[0] > heights[1] && heights[1] > heights[2],
        "{heights:?}"
    );
    assert!(heights[2] >= 40, "{heights:?}");
}

#[test]
fn the_strip_holds_sixteen_tiles_a_row_and_the_layers_sit_beside_it() {
    for size in [DrawerSize::Compact, DrawerSize::Expanded] {
        let frame = frame_for(size, false);
        let strip = frame.strip.unwrap();
        assert_eq!(strip.width, 16 * 8);
        assert_eq!(strip.height, size.strip_rows() as i32 * 8);
        assert!(inside(frame.strip_frame().unwrap(), frame.drawer));
        let rows = frame.layer_rows.unwrap();
        for (slot, row) in rows.iter().enumerate() {
            assert!(inside(row.row, frame.drawer), "{row:?}");
            assert!(row.row.x > strip.right(), "beside the strip");
            assert!(!row.eye.overlaps(row.lock) && !row.lock.overlaps(row.name));
            assert!(inside(row.eye, row.row) && inside(row.name, row.row));
            if slot > 0 {
                assert_eq!(row.row.y, rows[slot - 1].row.bottom(), "rows touch");
            }
        }
        assert_eq!(Frame::layer_in_slot(0), BgLayer::Bg2, "BG2 first");
        assert!(rows[2].row.bottom() <= frame.drawer.bottom());
    }
    let collapsed = frame_for(DrawerSize::Collapsed, false);
    assert!(collapsed.strip.is_none() && collapsed.layer_rows.is_none());
    assert_eq!(collapsed.drawer.height, collapsed.drawer_bar.height);
}

#[test]
fn the_collapsed_bar_has_room_for_the_brush_and_three_chips() {
    let frame = frame_for(DrawerSize::Collapsed, false);
    for (i, chip) in frame.chips.iter().enumerate() {
        assert!(inside(*chip, frame.drawer_bar), "{chip:?}");
        if i > 0 {
            assert!(frame.chips[i - 1].right() <= chip.x, "BG0 left of BG1");
        }
    }
    assert_eq!(frame.chips[2].right(), frame.drawer_bar.right());
    assert!(
        frame.brush_text.0 + 4 * 6 + 9 * 6 <= frame.chips[0].x,
        "text fits"
    );
    assert!(inside(frame.size_button, frame.drawer_bar));
    assert!(frame.size_button.right() <= frame.brush.x);
}

#[test]
fn a_point_resolves_to_the_part_under_it() {
    let frame = frame_for(DrawerSize::Compact, false);
    assert_eq!(frame.hit(middle(frame.canvas), 0), Hit::Canvas);
    for (tool, rect) in frame.tools {
        assert_eq!(frame.hit(middle(rect), 0), Hit::Tool(tool));
    }
    assert_eq!(frame.hit(middle(frame.grid_button), 0), Hit::Grid);
    assert_eq!(frame.hit(middle(frame.rulers_button), 0), Hit::Rulers);
    assert_eq!(frame.hit(middle(frame.zoom_out), 0), Hit::ZoomOut);
    assert_eq!(frame.hit(middle(frame.zoom_in), 0), Hit::ZoomIn);
    assert_eq!(frame.hit(middle(frame.size_button), 0), Hit::DrawerSize);
    assert_eq!(frame.hit(middle(frame.brush), 0), Hit::Brush);
    let rows = frame.layer_rows.unwrap();
    assert_eq!(frame.hit(middle(rows[0].eye), 0), Hit::Eye(BgLayer::Bg2));
    assert_eq!(frame.hit(middle(rows[1].lock), 0), Hit::Lock(BgLayer::Bg1));
    assert_eq!(
        frame.hit(middle(rows[2].name), 0),
        Hit::LayerName(BgLayer::Bg0)
    );
    assert_eq!(frame.hit(point(0, 239), 0), Hit::Nothing);
    assert_ne!(
        frame.hit(middle(frame.chips[1]), 0),
        Hit::Chip(BgLayer::Bg1)
    );
    let collapsed = frame_for(DrawerSize::Collapsed, false);
    assert_eq!(
        collapsed.hit(middle(collapsed.chips[1]), 0),
        Hit::Chip(BgLayer::Bg1)
    );
}

#[test]
fn a_strip_position_names_the_tile_for_the_row_it_is_scrolled_to() {
    let frame = frame_for(DrawerSize::Compact, false);
    let strip = frame.strip.unwrap();
    let at = |column: i32, row: i32| point(strip.x + column * 8 + 3, strip.y + row * 8 + 3);
    assert_eq!(frame.hit(at(0, 0), 0), Hit::Tile(0));
    assert_eq!(frame.hit(at(15, 4), 0), Hit::Tile(4 * 16 + 15));
    assert_eq!(frame.hit(at(2, 1), 10), Hit::Tile(11 * 16 + 2));
    assert_eq!(frame.hit(at(15, 4), 59), Hit::Tile(63 * 16 + 15));
}
