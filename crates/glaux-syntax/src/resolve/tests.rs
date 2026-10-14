use pretty_assertions::assert_eq;

use super::*;
use SyntaxKind::*;

fn capture(range: Range<usize>, kind: SyntaxKind, pattern: usize) -> Capture {
    Capture {
        range,
        kind,
        pattern,
        index: kind as usize,
    }
}

fn span(range: Range<usize>, kind: SyntaxKind) -> Span {
    Span { range, kind }
}

#[test]
fn nested_capture_splits_the_outer_one() {
    let captures = [capture(0..10, String, 0), capture(3..5, Constant, 1)];
    assert_eq!(
        resolve(&captures, &(0..10)),
        vec![
            span(0..3, String),
            span(3..5, Constant),
            span(5..10, String)
        ]
    );
}

#[test]
fn inner_capture_wins_even_from_an_earlier_pattern() {
    let captures = [capture(0..10, String, 5), capture(3..5, Constant, 0)];
    assert_eq!(
        resolve(&captures, &(0..10)),
        vec![
            span(0..3, String),
            span(3..5, Constant),
            span(5..10, String)
        ]
    );
}

#[test]
fn later_pattern_wins_on_the_same_range() {
    let captures = [capture(2..6, Variable, 0), capture(2..6, Function, 3)];
    assert_eq!(resolve(&captures, &(0..8)), vec![span(2..6, Function)]);
    let swapped = [capture(2..6, Function, 3), capture(2..6, Variable, 0)];
    assert_eq!(resolve(&swapped, &(0..8)), vec![span(2..6, Function)]);
}

#[test]
fn capture_index_breaks_a_pattern_tie() {
    let low = Capture {
        index: 1,
        ..capture(0..4, Keyword, 2)
    };
    let high = Capture {
        index: 4,
        ..capture(0..4, Operator, 2)
    };
    assert_eq!(
        resolve(&[low.clone(), high.clone()], &(0..4)),
        vec![span(0..4, Operator)]
    );
    assert_eq!(resolve(&[high, low], &(0..4)), vec![span(0..4, Operator)]);
}

#[test]
fn partially_overlapping_captures_cut_at_the_boundary() {
    let captures = [capture(0..6, Keyword, 0), capture(4..9, Comment, 1)];
    // The shorter node (4..9 is longer: 5 vs 6) wins the overlap.
    assert_eq!(
        resolve(&captures, &(0..9)),
        vec![span(0..4, Keyword), span(4..9, Comment)]
    );
}

#[test]
fn adjacent_spans_of_one_kind_merge() {
    let captures = [capture(0..3, Number, 0), capture(3..6, Number, 1)];
    assert_eq!(resolve(&captures, &(0..6)), vec![span(0..6, Number)]);
}

#[test]
fn adjacent_spans_of_different_kinds_stay_apart() {
    let captures = [capture(0..3, Number, 0), capture(3..6, String, 0)];
    assert_eq!(
        resolve(&captures, &(0..6)),
        vec![span(0..3, Number), span(3..6, String)]
    );
}

#[test]
fn spans_are_clipped_to_the_window() {
    let captures = [capture(0..10, Comment, 0)];
    assert_eq!(resolve(&captures, &(4..7)), vec![span(4..7, Comment)]);
}

#[test]
fn precedence_does_not_depend_on_the_window() {
    // Clipped lengths would be equal here; the node lengths are not.
    let captures = [capture(0..20, String, 1), capture(5..8, Constant, 0)];
    assert_eq!(resolve(&captures, &(5..8)), vec![span(5..8, Constant)]);
}

#[test]
fn gaps_stay_unstyled_and_empty_captures_are_ignored() {
    let captures = [
        capture(0..2, Keyword, 0),
        capture(5..5, Number, 0),
        capture(6..8, String, 0),
    ];
    assert_eq!(
        resolve(&captures, &(0..8)),
        vec![span(0..2, Keyword), span(6..8, String)]
    );
}
