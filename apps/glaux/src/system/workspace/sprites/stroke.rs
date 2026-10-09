//! Pointer drags as unbroken lines of tile pixels.
//!
//! The host reports at most one pointer move per frame, so a fast drag
//! jumps several pixels between moves; joining consecutive positions with
//! a line leaves no gaps.

/// The pixels of the line from `from` to `to`, both included, in order.
/// Each pixel is a step of at most one on each axis from the one before.
pub fn line(from: (i32, i32), to: (i32, i32)) -> impl Iterator<Item = (i32, i32)> {
    let (dx, dy) = ((to.0 - from.0).abs(), -(to.1 - from.1).abs());
    let (sx, sy) = ((to.0 - from.0).signum(), (to.1 - from.1).signum());
    let mut at = from;
    let mut error = dx + dy;
    let mut done = false;
    std::iter::from_fn(move || {
        if done {
            return None;
        }
        let current = at;
        if at == to {
            done = true;
            return Some(current);
        }
        let twice = 2 * error;
        if twice >= dy {
            error += dy;
            at.0 += sx;
        }
        if twice <= dx {
            error += dx;
            at.1 += sy;
        }
        Some(current)
    })
}

#[cfg(test)]
mod tests {
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
}
