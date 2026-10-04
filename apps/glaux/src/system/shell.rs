//! The system shell.

use glaux_input::InputEvent;

use super::line_editor::LineEditor;
use super::{ACCENT, BACKGROUND, Line, TEXT};

const BANNER: &str = "READY. TYPE HELP FOR HELP";
const PROMPT: &str = "> ";
const CURSOR: char = '_';
const PROMPT_ROW: i32 = 1;

/// Shell state: the command line being edited.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Shell {
    editor: LineEditor,
}

impl Shell {
    pub fn new() -> Self {
        Self::default()
    }

    /// Applies `event` to the command line. Returns the submitted line when
    /// Enter is pressed.
    pub fn handle_input(&mut self, event: InputEvent) -> Option<String> {
        self.editor.apply(event)
    }

    /// Shell screen text, with the command line soft-wrapped to `columns`
    /// cells per row.
    pub(super) fn lines(&self, columns: usize) -> Vec<Line> {
        let columns = columns.max(1);
        let mut lines = vec![Line::new(0, 0, BANNER, ACCENT)];
        let prompt: Vec<char> = PROMPT.chars().collect();
        lines.extend(wrapped(0, &prompt, columns, TEXT, None));

        // `draw_text` advances one cell per char, so a char index is also a
        // cell offset from the end of the prompt.
        let text_start = prompt.len();
        let chars = self.editor.chars();
        let selection = self.editor.selection().unwrap_or(0..0);
        let segments = [
            (0..selection.start, TEXT, None),
            (selection.clone(), BACKGROUND, Some(TEXT)),
            (selection.end..chars.len(), TEXT, None),
        ];
        for (range, color, background) in segments {
            lines.extend(wrapped(
                text_start + range.start,
                &chars[range],
                columns,
                color,
                background,
            ));
        }

        // At the end of a full row the cursor wraps to the start of the
        // next one, where the next character will go.
        let cursor = [CURSOR];
        lines.extend(wrapped(
            text_start + self.editor.cursor(),
            &cursor,
            columns,
            TEXT,
            None,
        ));
        lines
    }
}

/// Lays out `text` starting `start` cells into the command line, splitting
/// it into one [`Line`] per row of `columns` cells.
fn wrapped(
    start: usize,
    text: &[char],
    columns: usize,
    color: u8,
    background: Option<u8>,
) -> Vec<Line> {
    let mut lines = Vec::new();
    let mut pos = start;
    let mut rest = text;
    while !rest.is_empty() {
        let col = pos % columns;
        let (row_text, tail) = rest.split_at(rest.len().min(columns - col));
        let row = PROMPT_ROW + (pos / columns) as i32;
        let mut line = Line::new(col as i32, row, row_text.iter().collect::<String>(), color);
        if let Some(background) = background {
            line = line.with_background(background);
        }
        lines.push(line);
        pos += row_text.len();
        rest = tail;
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    fn shell_with(events: &[InputEvent]) -> Shell {
        let mut shell = Shell::new();
        for &event in events {
            assert_eq!(shell.handle_input(event), None);
        }
        shell
    }

    fn typed(text: &str) -> Vec<InputEvent> {
        text.chars().map(InputEvent::Text).collect()
    }

    /// Wide enough that none of the short test lines wrap.
    const COLUMNS: usize = 60;

    #[test]
    fn empty_shell_shows_banner_prompt_and_cursor() {
        assert_eq!(
            Shell::new().lines(COLUMNS),
            [
                Line::new(0, 0, BANNER, ACCENT),
                Line::new(0, 1, "> ", TEXT),
                Line::new(2, 1, "_", TEXT),
            ]
        );
    }

    #[test]
    fn typed_text_follows_prompt_with_cursor_after_it() {
        let lines = shell_with(&typed("help")).lines(COLUMNS);
        assert_eq!(
            lines[2..],
            [Line::new(2, 1, "help", TEXT), Line::new(6, 1, "_", TEXT)]
        );
    }

    #[test]
    fn cursor_is_drawn_at_insertion_point() {
        let mut events = typed("abc");
        events.push(InputEvent::Left { select: false });
        let lines = shell_with(&events).lines(COLUMNS);
        assert_eq!(lines.last(), Some(&Line::new(4, 1, "_", TEXT)));
    }

    #[test]
    fn selection_is_drawn_inverse_between_plain_segments() {
        let mut events = typed("abcde");
        events.extend([
            InputEvent::Left { select: false },
            InputEvent::Left { select: true },
            InputEvent::Left { select: true },
        ]);
        let lines = shell_with(&events).lines(COLUMNS);
        assert_eq!(
            lines[2..],
            [
                Line::new(2, 1, "ab", TEXT),
                Line::new(4, 1, "cd", BACKGROUND).with_background(TEXT),
                Line::new(6, 1, "e", TEXT),
                Line::new(4, 1, "_", TEXT),
            ]
        );
    }

    #[test]
    fn non_ascii_text_takes_one_cell_per_char() {
        let lines = shell_with(&typed("日é")).lines(COLUMNS);
        assert_eq!(
            lines[2..],
            [Line::new(2, 1, "日é", TEXT), Line::new(4, 1, "_", TEXT)]
        );
    }

    #[test]
    fn enter_returns_line_and_clears_prompt() {
        let mut shell = shell_with(&typed("dir"));
        assert_eq!(shell.handle_input(InputEvent::Enter), Some("dir".into()));
        assert_eq!(shell, Shell::new());
    }

    #[test]
    fn long_line_wraps_onto_following_rows() {
        let lines = shell_with(&typed("abcdefghijklmn")).lines(8);
        assert_eq!(
            lines[1..],
            [
                Line::new(0, 1, "> ", TEXT),
                Line::new(2, 1, "abcdef", TEXT),
                Line::new(0, 2, "ghijklmn", TEXT),
                Line::new(0, 3, "_", TEXT),
            ]
        );
    }

    #[test]
    fn cursor_wraps_to_next_row_when_row_is_full() {
        let lines = shell_with(&typed("abcde")).lines(8);
        assert_eq!(lines.last(), Some(&Line::new(7, 1, "_", TEXT)));

        let lines = shell_with(&typed("abcdef")).lines(8);
        assert_eq!(lines.last(), Some(&Line::new(0, 2, "_", TEXT)));
    }

    #[test]
    fn selection_across_rows_is_split_per_row() {
        let mut events = typed("abcdefgh");
        events.extend([InputEvent::Home { select: false }]);
        events.extend([InputEvent::Right { select: false }; 4]);
        events.extend([InputEvent::Right { select: true }; 3]);
        let lines = shell_with(&events).lines(8);
        assert_eq!(
            lines[2..],
            [
                Line::new(2, 1, "abcd", TEXT),
                Line::new(6, 1, "ef", BACKGROUND).with_background(TEXT),
                Line::new(0, 2, "g", BACKGROUND).with_background(TEXT),
                Line::new(1, 2, "h", TEXT),
                Line::new(1, 2, "_", TEXT),
            ]
        );
    }

    #[test]
    fn narrow_width_still_lays_out_every_cell() {
        let lines = shell_with(&typed("ab")).lines(0);
        let cells: Vec<(i32, i32, &str)> = lines[1..]
            .iter()
            .map(|line| (line.col, line.row, line.text.as_str()))
            .collect();
        assert_eq!(
            cells,
            [
                (0, 1, ">"),
                (0, 2, " "),
                (0, 3, "a"),
                (0, 4, "b"),
                (0, 5, "_"),
            ]
        );
    }
}
