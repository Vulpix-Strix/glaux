use pretty_assertions::assert_eq;
use tree_sitter::Point;

use super::between;

fn point(row: usize, column: usize) -> Point {
    Point { row, column }
}

#[test]
fn insertion_in_the_middle() {
    let edit = between("ab\ncd", "ab\nXcd");
    assert_eq!(
        (edit.start_byte, edit.old_end_byte, edit.new_end_byte),
        (3, 3, 4)
    );
    assert_eq!(edit.start_position, point(1, 0));
    assert_eq!(edit.old_end_position, point(1, 0));
    assert_eq!(edit.new_end_position, point(1, 1));
}

#[test]
fn deletion_across_lines() {
    let edit = between("one\ntwo\nthree", "onee");
    assert_eq!(edit.start_byte, 3);
    assert_eq!(edit.old_end_byte, 12);
    assert_eq!(edit.new_end_byte, 3);
    assert_eq!(edit.start_position, point(0, 3));
    assert_eq!(edit.old_end_position, point(2, 4));
    assert_eq!(edit.new_end_position, point(0, 3));
}

#[test]
fn repeated_characters_do_not_overlap_prefix_and_suffix() {
    let edit = between("aaa", "aaaa");
    assert!(edit.start_byte <= edit.old_end_byte);
    assert!(edit.start_byte <= edit.new_end_byte);
    assert_eq!(edit.new_end_byte - edit.start_byte, 1);
    assert_eq!(edit.old_end_byte - edit.start_byte, 0);
}

#[test]
fn edits_stay_on_character_boundaries() {
    // "é" and "è" share their first byte (0xC3), as do "✓" and "✔" up to two bytes.
    for (old, new) in [("aéb", "aèb"), ("a✓b", "a✔b"), ("é", "è"), ("✓", "✔")] {
        let edit = between(old, new);
        assert!(old.is_char_boundary(edit.start_byte), "{old} -> {new}");
        assert!(old.is_char_boundary(edit.old_end_byte), "{old} -> {new}");
        assert!(new.is_char_boundary(edit.new_end_byte), "{old} -> {new}");
    }
}

#[test]
fn columns_count_bytes() {
    let edit = between("é\nxy", "é\nxzy");
    assert_eq!(edit.start_position, point(1, 1));
    assert_eq!(edit.new_end_position, point(1, 2));
}
