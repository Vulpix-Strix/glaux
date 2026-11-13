use super::*;
use crate::oam::{SIZE_8X8, SIZE_16X16, SIZE_32X32, SIZE_RESERVED};
use crate::sprite::obj_grid_tile;
use pretty_assertions::assert_eq;

fn object(tile: u16, size: u8) -> OamEntry {
    OamEntry {
        tile,
        size,
        enabled: true,
        ..OamEntry::default()
    }
}

fn fault(column: usize, row: usize, cause: GridFault) -> TileFault {
    TileFault { column, row, cause }
}

#[test]
fn a_disabled_object_is_not_a_mistake_whatever_it_holds() {
    let reserved = OamEntry {
        enabled: false,
        ..object(1008, SIZE_RESERVED)
    };
    assert_eq!(reserved.diagnose(), Diagnosis::Disabled);
    let crossing = OamEntry {
        enabled: false,
        ..object(15, SIZE_32X32)
    };
    assert_eq!(crossing.diagnose(), Diagnosis::Disabled);
    assert!(!Diagnosis::Disabled.is_problem());
}

#[test]
fn the_reserved_size_makes_an_enabled_object_invisible() {
    let diagnosis = object(0, SIZE_RESERVED).diagnose();
    assert_eq!(diagnosis, Diagnosis::ReservedSize);
    assert!(diagnosis.is_problem());
}

#[test]
fn the_spec_example_resolves_four_tiles_of_the_grid() {
    let tiles: Vec<_> = [(0, 0), (1, 0), (0, 1), (1, 1)]
        .into_iter()
        .map(|(column, row)| obj_grid_tile(20, column, row))
        .collect();
    assert_eq!(tiles, [Some(20), Some(21), Some(36), Some(37)]);
    assert_eq!(object(20, SIZE_16X16).diagnose(), Diagnosis::Valid);
}

#[test]
fn a_single_tile_on_the_last_grid_row_is_valid() {
    for tile in 1008..1024 {
        assert_eq!(
            object(tile, SIZE_8X8).diagnose(),
            Diagnosis::Valid,
            "{tile}"
        );
    }
}

#[test]
fn rows_below_the_bank_fault_on_the_last_grid_row() {
    let diagnosis = object(1008, SIZE_32X32).diagnose();
    let expected: Vec<_> = (1..4)
        .flat_map(|row| (0..4).map(move |column| fault(column, row, GridFault::ExceedsBank)))
        .collect();
    assert_eq!(expected.len(), 12);
    assert_eq!(diagnosis, Diagnosis::PartiallyInvalid(expected));
    assert!(diagnosis.is_problem());
}

#[test]
fn the_edge_fault_wins_where_both_apply() {
    let diagnosis = object(1022, SIZE_32X32).diagnose();
    let mut expected = Vec::new();
    for row in 0..4 {
        for column in 0..4 {
            let cause = if column >= 2 {
                Some(GridFault::CrossesGridEdge)
            } else if row >= 1 {
                Some(GridFault::ExceedsBank)
            } else {
                None
            };
            if let Some(cause) = cause {
                expected.push(fault(column, row, cause));
            }
        }
    }
    assert_eq!(expected.len(), 2 + 3 * 4);
    assert_eq!(diagnosis, Diagnosis::PartiallyInvalid(expected));
}

#[test]
fn an_object_crossing_the_grid_edge_loses_the_columns_past_it() {
    let expected: Vec<_> = (0..4)
        .flat_map(|row| [2, 3].map(move |column| fault(column, row, GridFault::CrossesGridEdge)))
        .collect();
    assert_eq!(
        object(14, SIZE_32X32).diagnose(),
        Diagnosis::PartiallyInvalid(expected)
    );
}

#[test]
fn position_and_flips_do_not_make_an_object_invalid() {
    for (x, y) in [
        (i16::MIN, i16::MIN),
        (i16::MAX, i16::MAX),
        (-4, 100),
        (319, 239),
        (400, 0),
    ] {
        let placed = OamEntry {
            x,
            y,
            ..object(20, SIZE_32X32)
        };
        assert_eq!(placed.diagnose(), Diagnosis::Valid, "{x} {y}");
    }
    let plain = object(14, SIZE_32X32);
    for (flip_x, flip_y) in [(true, false), (false, true), (true, true)] {
        let flipped = OamEntry {
            flip_x,
            flip_y,
            ..plain
        };
        assert_eq!(flipped.diagnose(), plain.diagnose());
    }
}

#[test]
fn fields_outside_their_range_are_diagnosed_as_stored() {
    // Tile 1024 + 5 is stored as tile 5, and size 9 as size 1.
    assert_eq!(
        object(1024 + 5, SIZE_8X8).diagnose(),
        object(5, SIZE_8X8).diagnose()
    );
    assert_eq!(object(15, 9).diagnose(), object(15, SIZE_16X16).diagnose());
    assert_eq!(
        object(15, 9).diagnose(),
        Diagnosis::PartiallyInvalid(vec![
            fault(1, 0, GridFault::CrossesGridEdge),
            fault(1, 1, GridFault::CrossesGridEdge),
        ])
    );
    assert_eq!(
        object(0, SIZE_RESERVED + 8).diagnose(),
        Diagnosis::ReservedSize
    );
    assert_eq!(
        object(u16::MAX, u8::MAX).diagnose(),
        Diagnosis::ReservedSize
    );
}

#[test]
fn the_table_is_diagnosed_one_entry_or_all_in_index_order() {
    let mut oam = Oam::new();
    assert_eq!(oam.diagnose(OAM_ENTRIES), None);
    assert_eq!(oam.diagnose(0), Some(Diagnosis::Disabled));
    assert!(
        oam.diagnoses()
            .all(|(_, diagnosis)| diagnosis == Diagnosis::Disabled)
    );

    assert!(oam.set_entry(5, object(0, SIZE_RESERVED)));
    assert!(oam.set_entry(9, object(15, SIZE_16X16)));
    assert!(oam.set_entry(10, object(20, SIZE_16X16)));
    let all: Vec<_> = oam.diagnoses().collect();
    assert_eq!(all.len(), OAM_ENTRIES);
    assert!(all.iter().map(|(index, _)| *index).eq(0..OAM_ENTRIES));
    let problems: Vec<_> = all
        .into_iter()
        .filter(|(_, diagnosis)| diagnosis.is_problem())
        .collect();
    assert_eq!(
        problems,
        [
            (5, Diagnosis::ReservedSize),
            (
                9,
                Diagnosis::PartiallyInvalid(vec![
                    fault(1, 0, GridFault::CrossesGridEdge),
                    fault(1, 1, GridFault::CrossesGridEdge),
                ])
            ),
        ]
    );
    assert_eq!(oam.diagnose(10), Some(Diagnosis::Valid));
}
