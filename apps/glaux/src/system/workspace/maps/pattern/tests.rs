use pretty_assertions::assert_eq;

use super::*;

fn entry(tile: u16) -> TileEntry {
    TileEntry::new(tile, false, false, false, BgBank::ZERO).unwrap()
}

fn layer_with(cells: &[(usize, usize, TileEntry)]) -> TileLayer {
    let mut layer = TileLayer::empty(8, 6).unwrap();
    for &(x, y, entry) in cells {
        layer.set(x, y, entry);
    }
    layer
}

#[test]
fn a_pattern_is_a_rectangle_of_the_sheet_with_the_anchors_attributes() {
    let anchor = TileEntry::new(20, true, false, true, BgBank::new(2).unwrap()).unwrap();
    let cells = stamp(anchor, (3, 2));
    let tiles: Vec<_> = cells.iter().map(|(at, e)| (*at, e.tile())).collect();
    assert_eq!(
        tiles,
        [((0, 0), 20), ((1, 0), 21), ((2, 0), 22), ((0, 1), 36), ((1, 1), 37), ((2, 1), 38)]
    );
    assert!(cells.iter().all(|(_, e)| e.flip_x() && e.priority() && e.bank().get() == 2));
}

#[test]
fn a_pattern_stays_inside_the_sheet_and_skips_tile_zero() {
    let cells = stamp(entry(15), (2, 1));
    assert_eq!(cells.len(), 1);
    assert_eq!(stamp(entry(1023), (1, 2)).len(), 1);
    let tiles: Vec<_> = stamp(entry(0), (2, 1)).iter().map(|(_, e)| e.tile()).collect();
    assert_eq!(tiles, [1]);
}

#[test]
fn a_clip_keeps_the_block_and_its_empty_cells() {
    let layer = layer_with(&[(1, 1, entry(5)), (2, 2, entry(6))]);
    let clip = Clip::copy(
        &layer,
        Selection {
            left: 1,
            top: 1,
            right: 2,
            bottom: 2,
        },
    );
    assert_eq!((clip.width, clip.height), (2, 2));
    let cells: Vec<_> = clip.cells().map(|(at, e)| (at, e.tile())).collect();
    assert_eq!(cells, [((0, 0), 5), ((1, 0), 0), ((0, 1), 0), ((1, 1), 6)]);
}

#[test]
fn an_attribute_changes_only_itself() {
    let base = TileEntry::new(9, true, false, false, BgBank::ZERO).unwrap();
    let banked = Attr::Bank(BgBank::new(3).unwrap()).apply(base);
    assert_eq!((banked.tile(), banked.bank().get(), banked.flip_x()), (9, 3, true));
    let flipped = Attr::FlipY(true).apply(banked);
    assert!(flipped.flip_x() && flipped.flip_y() && !flipped.priority());
    assert!(Attr::Priority(true).apply(flipped).priority());
}

#[test]
fn a_summary_tells_shared_values_from_mixed_ones_and_ignores_empty_cells() {
    let flipped = TileEntry::new(2, true, false, false, BgBank::new(1).unwrap()).unwrap();
    let layer = layer_with(&[(0, 0, entry(1)), (1, 0, flipped)]);
    let all = Selection {
        left: 0,
        top: 0,
        right: 3,
        bottom: 1,
    };
    let summary = summarize(&layer, all).unwrap();
    assert_eq!(summary.cells, 2);
    assert_eq!(summary.bank, Shared::Mixed);
    assert_eq!(summary.flip_x, Shared::Mixed);
    assert_eq!(summary.flip_y, Shared::Same(false));
    assert_eq!(summary.priority, Shared::Same(false));

    let empty = Selection {
        left: 5,
        top: 4,
        right: 6,
        bottom: 5,
    };
    assert_eq!(summarize(&layer, empty), None);
}

#[test]
fn uses_and_bounds_find_every_cell_of_a_tile() {
    let layer = layer_with(&[(1, 1, entry(7)), (4, 3, entry(7)), (2, 2, entry(8))]);
    assert_eq!(uses(&layer, 7), 2);
    assert_eq!(uses(&layer, 9), 0);
    assert_eq!(
        bounds_of(&layer, 7),
        Some(Selection {
            left: 1,
            top: 1,
            right: 4,
            bottom: 3
        })
    );
    assert_eq!(bounds_of(&layer, 9), None);
}
