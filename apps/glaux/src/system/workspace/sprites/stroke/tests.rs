use super::*;
use pretty_assertions::assert_eq;

fn points(from: (i32, i32), to: (i32, i32)) -> Vec<(i32, i32)> {
    line(from, to).collect()
}

fn assert_continuous(from: (i32, i32), to: (i32, i32)) {
    let points = points(from, to);
    assert_eq!(points.first(), Some(&from));
    assert_eq!(points.last(), Some(&to));
    for pair in points.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        assert!(
            (a.0 - b.0).abs() <= 1 && (a.1 - b.1).abs() <= 1 && a != b,
            "{from:?} -> {to:?}: {a:?} then {b:?}"
        );
    }
    let longest_axis = (to.0 - from.0).abs().max((to.1 - from.1).abs());
    assert_eq!(points.len() as i32, longest_axis + 1, "{from:?} -> {to:?}");
}

#[test]
fn a_single_point_is_its_own_line() {
    assert_eq!(points((3, 4), (3, 4)), [(3, 4)]);
}

#[test]
fn straight_and_diagonal_lines_cover_every_pixel() {
    assert_eq!(
        points((0, 2), (4, 2)),
        [(0, 2), (1, 2), (2, 2), (3, 2), (4, 2)]
    );
    assert_eq!(points((1, 3), (1, 0)), [(1, 3), (1, 2), (1, 1), (1, 0)]);
    assert_eq!(
        points((0, 0), (7, 7)),
        (0..8).map(|n| (n, n)).collect::<Vec<_>>()
    );
}

#[test]
fn lines_in_every_direction_are_unbroken() {
    for to in [
        (7, 2),
        (2, 7),
        (-5, 3),
        (-3, -9),
        (6, -1),
        (0, -7),
        (-12, 0),
        (13, 5),
    ] {
        assert_continuous((0, 0), to);
        assert_continuous(to, (0, 0));
    }
    assert_continuous((-4, 3), (11, 6));
}
