use super::*;
use pretty_assertions::assert_eq;

fn lines(breakpoints: &Breakpoints) -> Vec<usize> {
    breakpoints.lines().collect()
}

fn edit(first: usize, last: usize, at_line_start: bool, inserted: usize) -> LineEdit {
    LineEdit {
        first,
        last,
        at_line_start,
        inserted,
    }
}

fn after(set: &[usize], edit: LineEdit) -> Vec<usize> {
    let mut breakpoints = Breakpoints::from_lines(set.iter().copied());
    breakpoints.follow(edit);
    lines(&breakpoints)
}

#[test]
fn toggling_sets_and_clears_a_line() {
    let mut breakpoints = Breakpoints::default();
    assert!(breakpoints.toggle(4));
    assert!(breakpoints.contains(4));
    assert!(!breakpoints.toggle(4));
    assert!(breakpoints.is_empty());
}

#[test]
fn truncating_drops_the_lines_past_the_end() {
    let mut breakpoints = Breakpoints::from_lines([1, 4, 5, 9]);
    breakpoints.truncate(5);
    assert_eq!(lines(&breakpoints), [1, 4]);
}

#[test]
fn an_edit_inside_a_line_changes_nothing() {
    assert_eq!(after(&[0, 3, 7], edit(3, 3, false, 0)), [0, 3, 7]);
    assert_eq!(after(&[0, 3, 7], edit(3, 3, true, 0)), [0, 3, 7]);
}

#[test]
fn a_line_break_in_the_middle_of_a_line_keeps_the_upper_half() {
    assert_eq!(after(&[2, 5], edit(2, 2, false, 1)), [2, 6]);
}

#[test]
fn a_line_break_at_the_start_of_a_line_pushes_it_down() {
    assert_eq!(after(&[2, 5], edit(2, 2, true, 1)), [3, 6]);
    assert_eq!(after(&[2], edit(2, 2, true, 3)), [5]);
}

#[test]
fn deleting_a_whole_line_deletes_its_breakpoint_and_pulls_the_next_ones_up() {
    // Removing "line 3\n": from the start of line 3 to the start of line 4.
    assert_eq!(after(&[2, 3, 4, 8], edit(3, 4, true, 0)), [2, 3, 7]);
}

#[test]
fn deleting_an_empty_line_works_the_same() {
    assert_eq!(after(&[5], edit(3, 4, true, 0)), [4]);
    assert_eq!(after(&[3], edit(3, 4, true, 0)), Vec::<usize>::new());
}

#[test]
fn joining_a_line_into_the_one_above_loses_only_the_joined_line() {
    // Backspace at the start of line 4: the break at the end of line 3.
    assert_eq!(after(&[3, 4, 6], edit(3, 4, false, 0)), [3, 5]);
}

#[test]
fn replacing_a_range_across_lines_drops_the_lines_it_swallowed() {
    // From the middle of line 2 to the middle of line 5, pasting two lines.
    assert_eq!(after(&[1, 2, 3, 4, 5, 6], edit(2, 5, false, 2)), [1, 2, 5]);
}

#[test]
fn replacing_from_a_line_start_moves_the_last_swallowed_line_to_the_new_text() {
    assert_eq!(after(&[2, 3, 4], edit(2, 4, true, 0)), [2]);
    assert_eq!(after(&[2, 3, 4, 5], edit(2, 4, true, 1)), [3, 4]);
}

#[test]
fn lines_before_the_edit_never_move() {
    assert_eq!(after(&[0, 1], edit(5, 7, false, 4)), [0, 1]);
}
