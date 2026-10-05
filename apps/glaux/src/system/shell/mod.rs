//! The system shell: a command line over a scrollback of earlier output.

mod commands;
mod completion;
mod history;
mod parser;
mod scrollback;

use glaux_input::InputEvent;
use semver::Version;

use super::line_editor::LineEditor;
use super::{ACCENT, BACKGROUND, ERROR, Line, TEXT};
use commands::{COMMANDS, Context};
use completion::{Completion, command_token, complete};
use history::History;
use scrollback::{Entry, Scrollback};

const BANNER: &str = "READY. TYPE HELP FOR HELP";
const PROMPT: &str = "> ";
const CURSOR: char = '_';

/// Shell state: the command line being edited, the command history and the
/// output shown above the prompt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Shell {
    editor: LineEditor,
    history: History,
    scrollback: Scrollback,
    rom_version: Version,
}

impl Shell {
    /// A shell showing only the banner, reporting `rom_version` in `info`.
    pub fn new(rom_version: Version) -> Self {
        let mut scrollback = Scrollback::default();
        scrollback.push(Entry::Banner(BANNER.into()));
        Self {
            editor: LineEditor::default(),
            history: History::default(),
            scrollback,
            rom_version,
        }
    }

    /// Applies `event`: edits the command line, navigates history,
    /// completes the command name, or runs the line on Enter.
    pub fn handle_input(&mut self, event: InputEvent) {
        match event {
            InputEvent::Up => {
                let current = self.editor.text();
                if let Some(line) = self.history.older(&current) {
                    self.editor.set_text(line);
                }
            }
            InputEvent::Down => {
                if let Some(line) = self.history.newer() {
                    self.editor.set_text(line);
                }
            }
            InputEvent::Tab => self.complete_command(),
            InputEvent::Enter => {
                if let Some(line) = self.editor.apply(event) {
                    self.submit(&line);
                }
            }
            // Changing the text ends history navigation, so the edited line
            // becomes the draft the next Up starts from.
            InputEvent::Text(_) | InputEvent::Backspace | InputEvent::Delete => {
                self.history.reset();
                self.editor.apply(event);
            }
            InputEvent::Left { .. }
            | InputEvent::Right { .. }
            | InputEvent::Home { .. }
            | InputEvent::End { .. } => {
                self.editor.apply(event);
            }
        }
    }

    /// Echoes and runs a submitted line. Blank lines leave no trace.
    fn submit(&mut self, line: &str) {
        self.history.reset();
        if line.trim().is_empty() {
            return;
        }
        self.scrollback.push(Entry::Command(line.to_owned()));
        self.history.push(line);
        match parser::parse(line) {
            Ok(Some(invocation)) => {
                let mut ctx = Context {
                    scrollback: &mut self.scrollback,
                    rom_version: &self.rom_version,
                };
                commands::execute(&invocation, &mut ctx);
            }
            Ok(None) => {}
            Err(err) => self.scrollback.push(Entry::Error(err.to_string())),
        }
    }

    /// Completes the command name under the cursor from the built-in
    /// commands, or lists the candidates when they share nothing more.
    fn complete_command(&mut self) {
        let chars = self.editor.chars();
        let cursor = self.editor.cursor();
        let Some(token) = command_token(chars, cursor) else {
            return;
        };
        let prefix: String = chars[token.start..cursor].iter().collect();
        let at_end = token.end == chars.len();
        let replacement = match complete(&prefix, COMMANDS.iter().map(|command| command.name)) {
            Completion::None => return,
            // A space after a complete name at the end of the line is ready
            // for the first argument.
            Completion::Unique(name) if at_end => format!("{name} "),
            Completion::Unique(name) => name.to_owned(),
            Completion::Extend(common) => common,
            Completion::Ambiguous(names) => {
                let names: Vec<String> = names.iter().map(|name| name.to_uppercase()).collect();
                self.scrollback.push(Entry::Command(self.editor.text()));
                self.scrollback.push(Entry::Output(names.join("  ")));
                return;
            }
        };
        self.history.reset();
        self.editor.replace(token, &replacement);
    }

    /// Shell screen text for a `columns` × `rows` cell area.
    ///
    /// Scrollback entries and the command line are soft-wrapped to
    /// `columns`. When they need more than `rows` rows, the oldest rows are
    /// dropped so the command line stays on the last rows.
    pub(super) fn lines(&self, columns: usize, rows: usize) -> Vec<Line> {
        let columns = columns.max(1);
        let rows = rows.max(1);
        let prompt: Vec<char> = PROMPT.chars().collect();
        let editor_rows = (prompt.len() + self.editor.chars().len()) / columns + 1;

        // Only the newest entries that can still reach the screen are laid
        // out; the scrollback may hold far more than fits.
        let mut used = editor_rows;
        let mut visible = Vec::new();
        for entry in self.scrollback.iter().rev() {
            if used >= rows {
                break;
            }
            used += entry_rows(entry, columns);
            visible.push(entry);
        }

        // Negative when the content overflows: those rows are cut below.
        let mut row = -(used.saturating_sub(rows) as i32);
        let mut lines = Vec::new();
        for entry in visible.into_iter().rev() {
            let (text, color, prompted) = match entry {
                Entry::Banner(text) => (text, ACCENT, false),
                Entry::Command(text) => (text, TEXT, true),
                Entry::Output(text) => (text, TEXT, false),
                Entry::Error(text) => (text, ERROR, false),
            };
            let text: Vec<char> = text.chars().collect();
            let start = if prompted {
                lines.extend(wrapped(0, &prompt, columns, row, TEXT, None));
                prompt.len()
            } else {
                0
            };
            lines.extend(wrapped(start, &text, columns, row, color, None));
            row += entry_rows(entry, columns) as i32;
        }
        lines.extend(self.editor_lines(&prompt, columns, row));
        lines.retain(|line| line.row >= 0);
        lines
    }

    /// The prompt and command line, starting at `row`.
    fn editor_lines(&self, prompt: &[char], columns: usize, row: i32) -> Vec<Line> {
        let mut lines = wrapped(0, prompt, columns, row, TEXT, None);

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
                row,
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
            row,
            TEXT,
            None,
        ));
        lines
    }
}

/// Rows `entry` takes when wrapped to `columns`; an empty entry still takes
/// one.
fn entry_rows(entry: &Entry, columns: usize) -> usize {
    let cells = match entry {
        Entry::Command(text) => PROMPT.chars().count() + text.chars().count(),
        Entry::Banner(text) | Entry::Output(text) | Entry::Error(text) => text.chars().count(),
    };
    cells.div_ceil(columns).max(1)
}

/// Lays out `text` starting `start` cells into a block whose first row is
/// `top`, splitting it into one [`Line`] per row of `columns` cells.
fn wrapped(
    start: usize,
    text: &[char],
    columns: usize,
    top: i32,
    color: u8,
    background: Option<u8>,
) -> Vec<Line> {
    let mut lines = Vec::new();
    let mut pos = start;
    let mut rest = text;
    while !rest.is_empty() {
        let col = pos % columns;
        let (row_text, tail) = rest.split_at(rest.len().min(columns - col));
        let row = top + (pos / columns) as i32;
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

    use InputEvent::{Backspace, Down, Enter, Tab, Up};

    /// Wide enough that none of the short test lines wrap.
    const COLUMNS: usize = 60;
    /// Tall enough that nothing scrolls off.
    const ROWS: usize = 40;

    fn new_shell() -> Shell {
        Shell::new(Version::new(0, 1, 0))
    }

    fn typed(text: &str) -> Vec<InputEvent> {
        text.chars().map(InputEvent::Text).collect()
    }

    fn shell_with(events: &[InputEvent]) -> Shell {
        let mut shell = new_shell();
        send(&mut shell, events);
        shell
    }

    /// A shell after `events` with the banner removed, so the command line
    /// starts on row 0 at any width.
    fn editor_only(events: &[InputEvent]) -> Shell {
        let mut shell = shell_with(events);
        shell.scrollback.clear();
        shell
    }

    fn send(shell: &mut Shell, events: &[InputEvent]) {
        for &event in events {
            shell.handle_input(event);
        }
    }

    fn run(shell: &mut Shell, line: &str) {
        send(shell, &typed(line));
        shell.handle_input(Enter);
    }

    fn entries(shell: &Shell) -> Vec<Entry> {
        shell.scrollback.iter().cloned().collect()
    }

    fn banner() -> Entry {
        Entry::Banner(BANNER.into())
    }

    fn command(text: &str) -> Entry {
        Entry::Command(text.into())
    }

    fn output(text: &str) -> Entry {
        Entry::Output(text.into())
    }

    fn error(text: &str) -> Entry {
        Entry::Error(text.into())
    }

    /// Editor lines, which follow everything laid out for the scrollback.
    fn editor_lines(shell: &Shell, columns: usize) -> Vec<Line> {
        let lines = shell.lines(columns, ROWS);
        let first = lines
            .iter()
            .rposition(|l| l.text == PROMPT && l.col == 0)
            .unwrap();
        lines[first..].to_vec()
    }

    // Editing and rendering of the command line.

    #[test]
    fn new_shell_shows_banner_prompt_and_cursor() {
        assert_eq!(
            new_shell().lines(COLUMNS, ROWS),
            [
                Line::new(0, 0, BANNER, ACCENT),
                Line::new(0, 1, "> ", TEXT),
                Line::new(2, 1, "_", TEXT),
            ]
        );
    }

    #[test]
    fn typed_text_follows_prompt_with_cursor_after_it() {
        let lines = shell_with(&typed("help")).lines(COLUMNS, ROWS);
        assert_eq!(
            lines[2..],
            [Line::new(2, 1, "help", TEXT), Line::new(6, 1, "_", TEXT)]
        );
    }

    #[test]
    fn cursor_is_drawn_at_insertion_point() {
        let mut events = typed("abc");
        events.push(InputEvent::Left { select: false });
        let lines = shell_with(&events).lines(COLUMNS, ROWS);
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
        let lines = shell_with(&events).lines(COLUMNS, ROWS);
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
        let lines = shell_with(&typed("日é")).lines(COLUMNS, ROWS);
        assert_eq!(
            lines[2..],
            [Line::new(2, 1, "日é", TEXT), Line::new(4, 1, "_", TEXT)]
        );
    }

    #[test]
    fn long_line_wraps_onto_following_rows() {
        let lines = editor_only(&typed("abcdefghijklmn")).lines(8, ROWS);
        assert_eq!(
            lines,
            [
                Line::new(0, 0, "> ", TEXT),
                Line::new(2, 0, "abcdef", TEXT),
                Line::new(0, 1, "ghijklmn", TEXT),
                Line::new(0, 2, "_", TEXT),
            ]
        );
    }

    #[test]
    fn cursor_wraps_to_next_row_when_row_is_full() {
        let lines = editor_only(&typed("abcde")).lines(8, ROWS);
        assert_eq!(lines.last(), Some(&Line::new(7, 0, "_", TEXT)));

        let lines = editor_only(&typed("abcdef")).lines(8, ROWS);
        assert_eq!(lines.last(), Some(&Line::new(0, 1, "_", TEXT)));
    }

    #[test]
    fn selection_across_rows_is_split_per_row() {
        let mut events = typed("abcdefgh");
        events.extend([InputEvent::Home { select: false }]);
        events.extend([InputEvent::Right { select: false }; 4]);
        events.extend([InputEvent::Right { select: true }; 3]);
        let lines = editor_only(&events).lines(8, ROWS);
        assert_eq!(
            lines[1..],
            [
                Line::new(2, 0, "abcd", TEXT),
                Line::new(6, 0, "ef", BACKGROUND).with_background(TEXT),
                Line::new(0, 1, "g", BACKGROUND).with_background(TEXT),
                Line::new(1, 1, "h", TEXT),
                Line::new(1, 1, "_", TEXT),
            ]
        );
    }

    #[test]
    fn narrow_width_still_lays_out_every_cell() {
        let cells: Vec<(i32, i32, String)> = editor_only(&typed("ab"))
            .lines(0, ROWS)
            .into_iter()
            .map(|line| (line.col, line.row, line.text))
            .collect();
        assert_eq!(
            cells,
            [
                (0, 0, ">".into()),
                (0, 1, " ".into()),
                (0, 2, "a".into()),
                (0, 3, "b".into()),
                (0, 4, "_".into()),
            ]
        );
    }

    // Submitting commands.

    #[test]
    fn enter_runs_the_command_and_opens_a_new_prompt() {
        let mut shell = new_shell();
        run(&mut shell, "help");
        assert_eq!(
            entries(&shell),
            [
                banner(),
                command("help"),
                output("HELP   SHOW AVAILABLE COMMANDS"),
                output("CLEAR  CLEAR THE CONSOLE"),
                output("INFO   SHOW SYSTEM INFORMATION"),
            ]
        );
        assert_eq!(shell.editor, LineEditor::default());
        assert_eq!(
            shell.lines(COLUMNS, ROWS)[1..3],
            [Line::new(0, 1, "> ", TEXT), Line::new(2, 1, "help", TEXT)]
        );
        assert_eq!(
            editor_lines(&shell, COLUMNS),
            [Line::new(0, 5, "> ", TEXT), Line::new(2, 5, "_", TEXT)]
        );
    }

    #[test]
    fn unknown_command_is_an_error_entry() {
        let mut shell = new_shell();
        run(&mut shell, "unknown");
        assert_eq!(
            entries(&shell),
            [
                banner(),
                command("unknown"),
                error("UNKNOWN COMMAND: UNKNOWN")
            ]
        );
        assert_eq!(
            shell.lines(COLUMNS, ROWS)[3],
            Line::new(0, 2, "UNKNOWN COMMAND: UNKNOWN", ERROR)
        );
    }

    #[test]
    fn parse_error_is_an_error_entry() {
        let mut shell = new_shell();
        run(&mut shell, "new \"Super Owl");
        assert_eq!(
            entries(&shell),
            [
                banner(),
                command("new \"Super Owl"),
                error("UNTERMINATED \" QUOTE")
            ]
        );
    }

    #[test]
    fn blank_enter_leaves_no_trace() {
        let mut shell = new_shell();
        send(&mut shell, &[Enter]);
        run(&mut shell, "   ");
        assert_eq!(entries(&shell), [banner()]);
        assert_eq!(shell.history, History::default());
        assert_eq!(shell, new_shell());
    }

    #[test]
    fn info_reports_the_rom_version() {
        let mut shell = Shell::new(Version::new(2, 0, 7));
        run(&mut shell, "INFO");
        assert!(entries(&shell).contains(&output("SYSTEM ROM 2.0.7")));
    }

    #[test]
    fn clear_empties_the_screen_but_keeps_history() {
        let mut shell = new_shell();
        run(&mut shell, "info");
        run(&mut shell, "clear");
        assert_eq!(entries(&shell), []);
        assert_eq!(
            shell.lines(COLUMNS, ROWS),
            [Line::new(0, 0, "> ", TEXT), Line::new(2, 0, "_", TEXT)]
        );

        send(&mut shell, &[Up]);
        assert_eq!(shell.editor.text(), "clear");
        send(&mut shell, &[Up]);
        assert_eq!(shell.editor.text(), "info");
    }

    #[test]
    fn long_submitted_command_wraps() {
        let mut shell = new_shell();
        shell.scrollback.clear();
        run(&mut shell, "abcdefghij");
        let lines = shell.lines(8, ROWS);
        assert_eq!(
            lines[..3],
            [
                Line::new(0, 0, "> ", TEXT),
                Line::new(2, 0, "abcdef", TEXT),
                Line::new(0, 1, "ghij", TEXT),
            ]
        );
        // The error starts on the row after the wrapped command.
        assert_eq!(lines[3].row, 2);
    }

    #[test]
    fn overflowing_output_keeps_the_newest_rows_and_the_prompt_at_the_bottom() {
        let mut shell = new_shell();
        for n in 0..10 {
            run(&mut shell, &format!("c{n}"));
        }
        send(&mut shell, &typed("x"));
        let lines = shell.lines(COLUMNS, 5);

        let rows: Vec<(i32, &str)> = lines.iter().map(|l| (l.row, l.text.as_str())).collect();
        assert_eq!(
            rows,
            [
                (0, "> "),
                (0, "c8"),
                (1, "UNKNOWN COMMAND: C8"),
                (2, "> "),
                (2, "c9"),
                (3, "UNKNOWN COMMAND: C9"),
                (4, "> "),
                (4, "x"),
                (4, "_"),
            ]
        );
    }

    #[test]
    fn partially_visible_entry_is_cut_at_the_top() {
        let mut shell = new_shell();
        shell.scrollback.clear();
        shell.scrollback.push(output("abcdefghijkl"));
        // 8 columns: the output takes 2 rows, the prompt 1, but only 2 fit.
        let lines = shell.lines(8, 2);
        assert_eq!(
            lines,
            [
                Line::new(0, 0, "ijkl", TEXT),
                Line::new(0, 1, "> ", TEXT),
                Line::new(2, 1, "_", TEXT),
            ]
        );
    }

    // History navigation.

    #[test]
    fn up_and_down_walk_history_and_restore_the_draft() {
        let mut shell = new_shell();
        run(&mut shell, "info");
        run(&mut shell, "help");
        send(&mut shell, &typed("dra"));

        send(&mut shell, &[Up]);
        assert_eq!(shell.editor.text(), "help");
        send(&mut shell, &[Up, Up]);
        assert_eq!(shell.editor.text(), "info");
        send(&mut shell, &[Down]);
        assert_eq!(shell.editor.text(), "help");
        send(&mut shell, &[Down]);
        assert_eq!(shell.editor.text(), "dra");
        assert_eq!(shell.editor.cursor(), 3);
        send(&mut shell, &[Down]);
        assert_eq!(shell.editor.text(), "dra");
    }

    #[test]
    fn editing_a_recalled_line_does_not_change_history() {
        let mut shell = new_shell();
        run(&mut shell, "info");
        send(&mut shell, &[Up, Backspace, Backspace]);
        send(&mut shell, &typed("XX"));
        assert_eq!(shell.editor.text(), "inXX");

        // The edit left navigation: the edited line is the new draft.
        send(&mut shell, &[Up]);
        assert_eq!(shell.editor.text(), "info");
        send(&mut shell, &[Down]);
        assert_eq!(shell.editor.text(), "inXX");
        assert_eq!(shell.history.entries().collect::<Vec<_>>(), ["info"]);
    }

    #[test]
    fn submitting_resets_history_navigation() {
        let mut shell = new_shell();
        run(&mut shell, "info");
        run(&mut shell, "help");
        send(&mut shell, &[Up, Up, Enter]);
        send(&mut shell, &[Up]);
        assert_eq!(shell.editor.text(), "info");
        assert_eq!(
            shell.history.entries().collect::<Vec<_>>(),
            ["info", "help", "info"]
        );
    }

    // Completion.

    #[test]
    fn tab_completes_a_unique_command() {
        let mut shell = shell_with(&typed("HE"));
        send(&mut shell, &[Tab]);
        assert_eq!(shell.editor.text(), "help ");
        assert_eq!(shell.editor.cursor(), 5);
    }

    #[test]
    fn tab_replaces_only_the_name_under_the_cursor() {
        let mut events = typed("hexx foo");
        events.extend([InputEvent::Home { select: false }]);
        events.extend([InputEvent::Right { select: false }; 2]);
        let mut shell = shell_with(&events);
        send(&mut shell, &[Tab]);
        assert_eq!(shell.editor.text(), "help foo");
        assert_eq!(shell.editor.cursor(), 4);
    }

    #[test]
    fn tab_on_arguments_or_without_match_does_nothing() {
        for line in ["help ", "xyz"] {
            let mut shell = shell_with(&typed(line));
            let before = shell.clone();
            send(&mut shell, &[Tab]);
            assert_eq!(shell, before, "{line:?}");
        }
    }

    #[test]
    fn tab_on_empty_line_lists_every_command() {
        let mut shell = new_shell();
        send(&mut shell, &[Tab]);
        assert_eq!(
            entries(&shell),
            [banner(), command(""), output("HELP  CLEAR  INFO")]
        );
        assert_eq!(shell.editor, LineEditor::default());
    }
}
