//! The system shell.

use super::{ACCENT, Line, TEXT};

/// Shell state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Shell;

/// Shell screen text.
pub(super) fn lines() -> Vec<Line> {
    vec![
        Line::new(0, 0, "READY. TYPE HELP FOR HELP", ACCENT),
        Line::new(0, 1, "> _", TEXT),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shows_prompt_below_banner() {
        let lines = lines();
        assert_eq!(lines[0].text, "READY. TYPE HELP FOR HELP");
        assert_eq!(lines[1].text, "> _");
        assert!(lines[1].row > lines[0].row);
    }
}
