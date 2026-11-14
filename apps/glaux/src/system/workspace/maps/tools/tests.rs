use super::*;
use glaux_machine::tilemap::BgBank;
use pretty_assertions::assert_eq;

fn entry(tile: u16) -> TileEntry {
    TileEntry::new(tile, false, false, false, BgBank::ZERO).unwrap()
}

#[test]
fn every_tool_has_its_own_key_and_message() {
    for tool in Tool::ALL {
        assert_eq!(Tool::from_key(tool.key()), Some(tool));
        assert_eq!(
            Tool::from_key(tool.key().to_ascii_uppercase()),
            Some(tool),
            "case is ignored"
        );
    }
    let mut keys: Vec<char> = Tool::ALL.iter().map(|tool| tool.key()).collect();
    keys.sort_unstable();
    keys.dedup();
    assert_eq!(keys.len(), Tool::ALL.len());
    assert_eq!(Tool::from_key('z'), None);
    assert_eq!(Tool::default(), Tool::Pencil);
}

#[test]
fn a_selection_is_the_rectangle_between_two_corners_in_any_order() {
    let a = Selection::between((5, 7), (2, 3), (10, 10)).unwrap();
    let b = Selection::between((2, 3), (5, 7), (10, 10)).unwrap();
    assert_eq!(a, b);
    assert_eq!((a.left, a.top, a.right, a.bottom), (2, 3, 5, 7));
    assert_eq!((a.width(), a.height()), (4, 5));
}

#[test]
fn a_selection_is_cut_at_the_edges_of_the_map_and_never_wraps() {
    let cut = Selection::between((-4, 2), (3, 20), (10, 8)).unwrap();
    assert_eq!((cut.left, cut.top, cut.right, cut.bottom), (0, 2, 3, 7));
    let one = Selection::between((9, 7), (40, 40), (10, 8)).unwrap();
    assert_eq!((one.width(), one.height()), (1, 1));
}

#[test]
fn a_rectangle_outside_the_map_selects_nothing() {
    assert_eq!(Selection::between((-5, -5), (-1, 3), (10, 10)), None);
    assert_eq!(Selection::between((10, 0), (12, 3), (10, 10)), None);
    assert_eq!(Selection::between((0, 10), (3, 12), (10, 10)), None);
    assert_eq!(Selection::between((0, 0), (1, 1), (0, 0)), None);
}

#[test]
fn the_connected_cells_are_those_joined_by_edges_that_hold_the_same_entry() {
    // . . # .
    // . # # .
    // # . . #   the lone # at the bottom left touches nothing above it
    let mut layer = TileLayer::empty(4, 3).unwrap();
    for (x, y) in [(2, 0), (1, 1), (2, 1), (0, 2), (3, 2)] {
        layer.set(x, y, entry(1));
    }
    let mut ink = connected(&layer, 2, 0);
    ink.sort_unstable();
    assert_eq!(ink, [(1, 1), (2, 0), (2, 1)]);
    assert_eq!(connected(&layer, 2, 0)[0], (2, 0), "the start comes first");

    let mut empty = connected(&layer, 0, 0);
    empty.sort_unstable();
    assert_eq!(empty, [(0, 0), (0, 1), (1, 0)]);
    assert_eq!(connected(&layer, 4, 0), Vec::new(), "off the layer");
}

#[test]
fn entries_that_differ_in_any_attribute_are_not_the_same_entry() {
    let mut layer = TileLayer::empty(5, 1).unwrap();
    let plain = entry(3);
    let flipped = TileEntry::new(3, true, false, false, BgBank::ZERO).unwrap();
    let boosted = TileEntry::new(3, false, false, true, BgBank::ZERO).unwrap();
    let other_bank = TileEntry::new(3, false, false, false, BgBank::new(2).unwrap()).unwrap();
    for (x, tile) in [plain, flipped, plain, boosted, other_bank]
        .into_iter()
        .enumerate()
    {
        layer.set(x, 0, tile);
    }
    assert_eq!(connected(&layer, 0, 0), [(0, 0)], "the flip breaks it");
    assert_eq!(connected(&layer, 2, 0), [(2, 0)], "the priority bit too");
    assert_eq!(connected(&layer, 4, 0), [(4, 0)], "and the bank");
}

#[test]
fn a_whole_map_of_the_largest_size_is_connected_without_trouble() {
    let layer = TileLayer::empty(256, 256).unwrap();
    let cells = connected(&layer, 100, 100);
    assert_eq!(cells.len(), 65_536);
    let mut seen = vec![false; 65_536];
    for (x, y) in cells {
        assert!(!seen[y * 256 + x], "({x}, {y}) listed twice");
        seen[y * 256 + x] = true;
    }
    assert!(seen.into_iter().all(|cell| cell));
}

#[test]
fn a_snake_the_width_of_a_cell_is_followed_to_its_end() {
    // A path winding through a 256 x 256 map one cell wide.
    let mut layer = TileLayer::empty(256, 256).unwrap();
    let mut length = 0;
    for y in (0..256).step_by(2) {
        for x in 0..256 {
            layer.set(x, y, entry(1));
            length += 1;
        }
        if y + 1 < 256 {
            let x = if (y / 2) % 2 == 0 { 255 } else { 0 };
            layer.set(x, y + 1, entry(1));
            length += 1;
        }
    }
    assert_eq!(connected(&layer, 0, 0).len(), length);
}

#[test]
fn painting_changes_a_cell_only_when_the_entry_differs() {
    let mut layer = TileLayer::empty(2, 2).unwrap();
    layer.set(0, 0, entry(4));
    assert!(!would_change(&layer, 0, 0, entry(4)));
    assert!(would_change(&layer, 0, 0, entry(5)));
    assert!(would_change(&layer, 1, 1, entry(4)));
    assert!(!would_change(&layer, 2, 2, entry(4)), "off the layer");
}

#[test]
fn a_line_of_cells_has_no_gaps() {
    let cells: Vec<(i32, i32)> = line((0, 0), (9, 3)).collect();
    assert_eq!(cells.first(), Some(&(0, 0)));
    assert_eq!(cells.last(), Some(&(9, 3)));
    for pair in cells.windows(2) {
        assert!((pair[1].0 - pair[0].0).abs() <= 1 && (pair[1].1 - pair[0].1).abs() <= 1);
    }
}
