use super::*;
use pretty_assertions::assert_eq;

fn doc(text: &str) -> CodeDocument {
    CodeDocument::from_text("code/main.luau", text)
}

#[test]
fn extending_past_the_anchor_flips_the_selection_to_its_other_side() {
    let document = doc("abcdef");
    let mut selection = Selection::collapsed(Caret::at(&document, 3));
    let right = |s: Selection| s.stepped(&document, true, Edge::End, |c| c.right(&document));
    let left = |s: Selection| s.stepped(&document, true, Edge::Start, |c| c.left(&document));
    selection = right(right(selection));
    assert_eq!(selection.range(), 3..5);
    selection = left(left(left(left(selection))));
    assert_eq!((selection.range(), selection.anchor()), (1..3, 3));
    selection = right(right(selection));
    assert!(selection.is_empty());
    assert_eq!(selection.caret().byte(), 3);
}

#[test]
fn a_plain_step_collapses_a_selection_to_the_side_it_heads_for() {
    let document = doc("abcdef");
    // Selected backwards, from 4 to 1.
    let selection = Selection::between(&document, 4, Caret::at(&document, 1));
    let left = selection.stepped(&document, false, Edge::Start, |c| c.left(&document));
    let right = selection.stepped(&document, false, Edge::End, |c| c.right(&document));
    assert_eq!((left.range(), left.caret().byte()), (1..1, 1));
    assert_eq!((right.range(), right.caret().byte()), (4..4, 4));
    // With nothing selected, the step moves.
    let empty = Selection::collapsed(Caret::at(&document, 2));
    let moved = empty.stepped(&document, false, Edge::End, |c| c.right(&document));
    assert_eq!(moved.caret().byte(), 3);
}

#[test]
fn moving_without_select_drops_the_selection() {
    let document = doc("one\ntwo");
    let selection = Selection::between(&document, 0, Caret::at(&document, 3));
    let moved = selection.moved(Caret::document_end(&document), false);
    assert_eq!((moved.range(), moved.caret().byte()), (7..7, 7));
}

#[test]
fn selecting_everything_skips_a_byte_order_mark() {
    let document = doc("\u{feff}ab\r\ncd");
    assert_eq!(Selection::all(&document).range(), 3..9);
}

#[test]
fn reset_moves_both_ends_to_valid_positions() {
    let document = doc("é\r\nx");
    // Inside é, and between the CR and the LF.
    let selection = Selection::between(&document, 1, Caret::at(&document, 0));
    assert_eq!(selection.range(), 0..0);
    let selection = Selection {
        anchor: 3,
        caret: Caret::at(&document, 5),
    };
    assert_eq!(selection.reset(&document).range(), 2..5);
}
