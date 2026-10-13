use super::*;
use pretty_assertions::assert_eq;

fn contents(text: &str) -> Vec<&str> {
    let lines = Lines::new(text);
    (0..lines.count())
        .map(|n| &text[lines.range(text, n).unwrap()])
        .collect()
}

#[test]
fn lines_leave_out_their_line_endings() {
    assert_eq!(contents("a\nbb\r\nccc"), ["a", "bb", "ccc"]);
    assert_eq!(contents("a\r\n\r\nb"), ["a", "", "b"]);
}

#[test]
fn the_text_after_the_last_newline_is_a_line() {
    assert_eq!(contents("a\nb"), ["a", "b"]);
    assert_eq!(contents("a\nb\n"), ["a", "b", ""]);
    assert_eq!(contents(""), [""]);
    assert_eq!(contents("\n"), ["", ""]);
}

#[test]
fn a_byte_order_mark_is_not_part_of_the_first_line() {
    let text = "\u{feff}local x\r\ny";
    assert_eq!(contents(text), ["local x", "y"]);
    let lines = Lines::new(text);
    assert_eq!(lines.range(text, 0), Some(3..10));
    assert_eq!(contents("\u{feff}"), [""]);
    assert_eq!(contents("\u{feff}\r\n"), ["", ""]);
}

#[test]
fn only_a_carriage_return_before_a_newline_ends_a_line() {
    assert_eq!(contents("a\rb\nc\r"), ["a\rb", "c\r"]);
}

#[test]
fn ranges_are_byte_offsets_into_the_text() {
    let text = "añ\né\n";
    let lines = Lines::new(text);
    assert_eq!(lines.range(text, 0), Some(0..3));
    assert_eq!(lines.range(text, 1), Some(4..6));
    assert_eq!(lines.range(text, 2), Some(7..7));
    assert_eq!(lines.range(text, 3), None);
}

#[test]
fn each_byte_belongs_to_the_line_its_ending_closes() {
    // BOM, "ab", CR LF, empty line's LF, "c".
    let text = "\u{feff}ab\r\n\nc";
    let lines = Lines::new(text);
    let owners: Vec<_> = (0..=text.len()).map(|byte| lines.line_of(byte)).collect();
    assert_eq!(owners, [0, 0, 0, 0, 0, 0, 0, 1, 2, 2]);
    assert_eq!(lines.line_of(usize::MAX), 2);
    assert_eq!(Lines::new("").line_of(0), 0);
    assert_eq!(Lines::new("a\n").line_of(2), 1);
}
