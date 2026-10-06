//! The system shell: a command line over a scrollback of earlier output.

mod commands;
mod completion;
mod history;
mod parser;
mod scrollback;

use glaux_input::InputEvent;
use glaux_luau::{LogEntry, LogLevel};
use glaux_machine::psg::{Adsr, CHANNELS, Waveform};

use super::line_editor::LineEditor;
use super::{ACCENT, BACKGROUND, DIM, ERROR, Line, SystemInfo, TEXT, TRACE, WARN};
use crate::cartridge::LoadedCartridge;
use crate::cartridge_fs::CartridgeStorage;
use commands::{COMMANDS, Context};

pub(super) use commands::cartridge_identity;
use completion::{Completion, command_token, complete};
use history::History;
use scrollback::{Entry, Scrollback};

/// A diagnostic tone the shell holds on one PSG channel.
///
/// Set with the TONE command as system bring-up tooling; it is not how
/// cartridges produce sound.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tone {
    pub waveform: Waveform,
    /// Hz, between 1 and [`MAX_FREQUENCY`](glaux_machine::psg::MAX_FREQUENCY).
    pub frequency: u16,
    /// Up to [`MAX_VOLUME`](glaux_machine::psg::MAX_VOLUME).
    pub volume: u8,
}

/// The diagnostic sound state the shell holds for one PSG channel: the
/// oscillator set with TONE, the envelope set with ENV and the gate set with
/// GATE.
///
/// This is system bring-up tooling; it is not how cartridges produce sound.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Voice {
    /// `None` mutes the channel.
    pub tone: Option<Tone>,
    /// Envelope parameters in PSG units (samples).
    pub adsr: Adsr,
    pub gate: bool,
}

const BANNER: &str = "READY. TYPE HELP FOR HELP";
const PROMPT: &str = "> ";
/// Shell ticks the cursor spends in each blink phase: 600 ms at 60 Hz.
pub(super) const BLINK_TICKS: u32 = 36;

/// A cartridge lifecycle change asked for by a shell command and carried
/// out by the system.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShellRequest {
    /// Make this validated cartridge the loaded one, replacing any other.
    Load(LoadedCartridge),
    /// Start the loaded cartridge.
    Run,
    /// Remove the loaded cartridge from the shell.
    Unload,
    /// Continue the paused cartridge with the VM it had.
    Resume,
    /// Dispose of the paused cartridge's VM, keeping the cartridge loaded.
    Stop,
}

/// The cartridge the system holds, as the shell sees it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CartridgeSlot<'a> {
    Empty,
    /// Loaded, with no VM.
    Loaded(&'a LoadedCartridge),
    /// Its VM is alive but paused while the shell is active.
    Paused(&'a LoadedCartridge),
}

impl<'a> CartridgeSlot<'a> {
    /// A slot that is empty or holds a loaded cartridge.
    pub fn loaded(cartridge: Option<&'a LoadedCartridge>) -> Self {
        cartridge.map_or(Self::Empty, Self::Loaded)
    }
}

/// The command line being edited, the command history, the
/// output shown above the prompt and the current cartridge directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Shell {
    editor: LineEditor,
    history: History,
    scrollback: Scrollback,
    info: SystemInfo,
    cartridges: CartridgeStorage,
    /// Per-channel diagnostic voices set by TONE, ENV and GATE.
    voices: [Voice; CHANNELS],
    /// Ticks since the cursor blink last restarted in its visible phase.
    blink: u32,
}

impl Shell {
    /// A shell showing only the banner, reporting `info` in system commands
    /// and navigating `cartridges`.
    pub fn new(info: SystemInfo, cartridges: CartridgeStorage) -> Self {
        let mut scrollback = Scrollback::default();
        scrollback.push(Entry::Banner(BANNER.into()));
        Self {
            editor: LineEditor::default(),
            history: History::default(),
            scrollback,
            info,
            cartridges,
            voices: [Voice::default(); CHANNELS],
            blink: 0,
        }
    }

    /// Advances the cursor blink by one system tick.
    pub(super) fn tick(&mut self) {
        self.blink = self.blink.wrapping_add(1);
    }

    /// The diagnostic voice set with TONE, ENV and GATE for each PSG
    /// channel.
    pub fn voices(&self) -> [Voice; CHANNELS] {
        self.voices
    }

    /// Applies `event`: edits the command line, navigates history,
    /// completes the command name, or runs the line on Enter.
    ///
    /// `slot` is the cartridge the system currently holds. A command that
    /// changes the cartridge lifecycle returns the change for the system to
    /// carry out.
    pub fn handle_input(
        &mut self,
        event: InputEvent,
        slot: CartridgeSlot<'_>,
    ) -> Option<ShellRequest> {
        // Any input the shell acts on shows the cursor and restarts its
        // blink, so it stays solid while typing or moving.
        if !matches!(
            event,
            InputEvent::Break | InputEvent::ButtonDown(_) | InputEvent::ButtonUp(_)
        ) {
            self.blink = 0;
        }
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
                    return self.submit(&line, slot);
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
            // No cartridge runs while the shell is active, so there is
            // nothing to interrupt. A paused cartridge is left with the
            // RESUME and STOP commands.
            InputEvent::Break => {}
            // Game buttons belong to a running cartridge.
            InputEvent::ButtonDown(_) | InputEvent::ButtonUp(_) => {}
        }
        None
    }

    /// Adds entries logged by a cartridge to the scrollback, keeping their
    /// levels.
    pub(super) fn log(&mut self, entries: impl IntoIterator<Item = LogEntry>) {
        for entry in entries {
            for line in log_lines(&entry.text) {
                self.scrollback.push(Entry::Log(entry.level, line));
            }
        }
    }

    /// Adds a system status line to the scrollback.
    pub(super) fn report(&mut self, line: impl Into<String>) {
        self.scrollback.push(Entry::Output(line.into()));
    }

    /// Adds a system error line to the scrollback.
    pub(super) fn report_error(&mut self, line: impl Into<String>) {
        self.scrollback.push(Entry::Error(line.into()));
    }

    /// Scrollback text oldest first, with commands after `> ` and errors
    /// after `! `. Logged lines show only their text.
    #[cfg(test)]
    pub(super) fn transcript(&self) -> Vec<String> {
        self.scrollback
            .iter()
            .map(|entry| match entry {
                Entry::Banner(text) | Entry::Output(text) | Entry::Log(_, text) => text.clone(),
                Entry::Command(text) => format!("{PROMPT}{text}"),
                Entry::Error(text) => format!("! {text}"),
            })
            .collect()
    }

    /// Echoes and runs a submitted line. Blank lines leave no trace.
    fn submit(&mut self, line: &str, slot: CartridgeSlot<'_>) -> Option<ShellRequest> {
        self.history.reset();
        if line.trim().is_empty() {
            return None;
        }
        self.scrollback.push(Entry::Command(line.to_owned()));
        self.history.push(line);
        match parser::parse(line) {
            Ok(Some(invocation)) => {
                let mut request = None;
                let mut ctx = Context {
                    scrollback: &mut self.scrollback,
                    info: &self.info,
                    cartridges: &mut self.cartridges,
                    slot,
                    request: &mut request,
                    voices: &mut self.voices,
                };
                commands::execute(&invocation, &mut ctx);
                request
            }
            Ok(None) => None,
            Err(err) => {
                self.scrollback.push(Entry::Error(err.to_string()));
                None
            }
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
                Entry::Log(level, text) => (text, log_color(*level), false),
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

        // The cursor is a filled cell drawn last, so it covers whatever is
        // under it, selected or not, and shows that character inverse. A
        // space gives it a cell's width past the end of the line. At the end
        // of a full row it wraps to the start of the next one, where the next
        // character will go.
        if (self.blink / BLINK_TICKS).is_multiple_of(2) {
            let cursor = self.editor.cursor();
            let under = [chars.get(cursor).copied().unwrap_or(' ')];
            lines.extend(wrapped(
                text_start + cursor,
                &under,
                columns,
                row,
                BACKGROUND,
                Some(TEXT),
            ));
        }
        lines
    }
}

/// Columns between tab stops in logged text.
const TAB_WIDTH: usize = 4;

/// The color logged lines of `level` are shown in.
fn log_color(level: LogLevel) -> u8 {
    match level {
        LogLevel::Trace => TRACE,
        LogLevel::Debug => DIM,
        LogLevel::Info => TEXT,
        LogLevel::Warn => WARN,
    }
}

/// Scrollback lines for one log entry: split at newlines, with tabs expanded
/// to spaces because the shell wraps by character and has no tab glyph.
fn log_lines(text: &str) -> impl Iterator<Item = String> + '_ {
    text.split('\n').map(|line| {
        let mut expanded = String::with_capacity(line.len());
        let mut column = 0;
        for ch in line.chars() {
            if ch == '\t' {
                let spaces = TAB_WIDTH - column % TAB_WIDTH;
                expanded.extend(std::iter::repeat_n(' ', spaces));
                column += spaces;
            } else {
                expanded.push(ch);
                column += 1;
            }
        }
        expanded
    })
}

/// Rows `entry` takes when wrapped to `columns`; an empty entry still takes
/// one.
fn entry_rows(entry: &Entry, columns: usize) -> usize {
    let cells = match entry {
        Entry::Command(text) => PROMPT.chars().count() + text.chars().count(),
        Entry::Banner(text) | Entry::Output(text) | Entry::Error(text) | Entry::Log(_, text) => {
            text.chars().count()
        }
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
    use crate::cartridge::fixture::{write_cartridge, write_manifest};
    use crate::cartridge_fs::fixture::Fixture;
    use pretty_assertions::assert_eq;
    use semver::Version;

    use InputEvent::{Backspace, Down, Enter, Tab, Up};

    /// Wide enough that none of the short test lines wrap.
    const COLUMNS: usize = 60;
    /// Tall enough that nothing scrolls off.
    const ROWS: usize = 40;

    fn shell_with_rom(rom_version: Version) -> Shell {
        Shell::new(SystemInfo { rom_version }, CartridgeStorage::Unavailable)
    }

    fn new_shell() -> Shell {
        shell_with_rom(Version::new(0, 1, 0))
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
            shell.handle_input(event, CartridgeSlot::Empty);
        }
    }

    fn run(shell: &mut Shell, line: &str) {
        send(shell, &typed(line));
        shell.handle_input(Enter, CartridgeSlot::Empty);
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

    /// The block cursor over `under`, the character in its cell.
    fn cursor(col: i32, row: i32, under: &str) -> Line {
        Line::new(col, row, under, BACKGROUND).with_background(TEXT)
    }

    fn ticked(mut shell: Shell, ticks: u32) -> Shell {
        for _ in 0..ticks {
            shell.tick();
        }
        shell
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
                cursor(2, 1, " "),
            ]
        );
    }

    #[test]
    fn typed_text_follows_prompt_with_cursor_after_it() {
        let lines = shell_with(&typed("help")).lines(COLUMNS, ROWS);
        assert_eq!(
            lines[2..],
            [Line::new(2, 1, "help", TEXT), cursor(6, 1, " ")]
        );
    }

    #[test]
    fn cursor_is_drawn_at_insertion_point() {
        let mut events = typed("abc");
        events.push(InputEvent::Left { select: false });
        let lines = shell_with(&events).lines(COLUMNS, ROWS);
        assert_eq!(lines.last(), Some(&cursor(4, 1, "c")));
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
                cursor(4, 1, "c"),
            ]
        );
    }

    #[test]
    fn non_ascii_text_takes_one_cell_per_char() {
        let lines = shell_with(&typed("日é")).lines(COLUMNS, ROWS);
        assert_eq!(
            lines[2..],
            [Line::new(2, 1, "日é", TEXT), cursor(4, 1, " ")]
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
                cursor(0, 2, " "),
            ]
        );
    }

    #[test]
    fn cursor_wraps_to_next_row_when_row_is_full() {
        let lines = editor_only(&typed("abcde")).lines(8, ROWS);
        assert_eq!(lines.last(), Some(&cursor(7, 0, " ")));

        let lines = editor_only(&typed("abcdef")).lines(8, ROWS);
        assert_eq!(lines.last(), Some(&cursor(0, 1, " ")));
    }

    #[test]
    fn cursor_over_text_shows_the_character_inverse() {
        let mut events = typed("abc");
        events.push(InputEvent::Home { select: false });
        let lines = shell_with(&events).lines(COLUMNS, ROWS);
        assert_eq!(lines.last(), Some(&cursor(2, 1, "a")));
    }

    #[test]
    fn cursor_over_a_selection_is_drawn_after_it() {
        let mut events = typed("abc");
        events.push(InputEvent::Left { select: true });
        let lines = shell_with(&events).lines(COLUMNS, ROWS);
        assert_eq!(
            lines[2..],
            [
                Line::new(2, 1, "ab", TEXT),
                Line::new(4, 1, "c", BACKGROUND).with_background(TEXT),
                cursor(4, 1, "c"),
            ]
        );
    }

    #[test]
    fn cursor_blinks_without_moving_the_text() {
        let mut events = typed("abcd");
        events.extend([InputEvent::Left { select: true }; 2]);
        let shell = shell_with(&events);
        let on = shell.lines(COLUMNS, ROWS);
        assert_eq!(on.last(), Some(&cursor(4, 1, "c")));

        let off = ticked(shell.clone(), BLINK_TICKS).lines(COLUMNS, ROWS);
        assert_eq!(off, on[..on.len() - 1]);
        assert_eq!(
            ticked(shell.clone(), BLINK_TICKS - 1).lines(COLUMNS, ROWS),
            on
        );
        assert_eq!(ticked(shell, 2 * BLINK_TICKS).lines(COLUMNS, ROWS), on);
    }

    #[test]
    fn input_restarts_the_blink_visible() {
        let mut shell = ticked(shell_with(&typed("ab")), BLINK_TICKS);
        assert_eq!(shell.lines(COLUMNS, ROWS).last().unwrap().text, "ab");

        send(&mut shell, &[InputEvent::Left { select: false }]);
        assert_eq!(shell.lines(COLUMNS, ROWS).last(), Some(&cursor(3, 1, "b")));

        // Input the shell ignores leaves the blink running.
        let mut shell = ticked(shell, BLINK_TICKS);
        send(&mut shell, &[InputEvent::Break]);
        assert_eq!(shell.lines(COLUMNS, ROWS).last().unwrap().text, "ab");
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
                cursor(1, 1, "h"),
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
                (0, 4, " ".into()),
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
                output("HELP     SHOW AVAILABLE COMMANDS"),
                output("CLEAR    CLEAR THE CONSOLE"),
                output("INFO     SHOW SYSTEM INFORMATION"),
                output("VERSION  SHOW THE SYSTEM ROM VERSION"),
                output("PWD      SHOW THE CURRENT DIRECTORY"),
                output("LS       LIST A DIRECTORY"),
                output("CD       CHANGE THE CURRENT DIRECTORY"),
                output("LOAD     LOAD A CARTRIDGE"),
                output("UNLOAD   UNLOAD THE LOADED CARTRIDGE"),
                output("RUN      RUN THE LOADED CARTRIDGE"),
                output("RESUME   RESUME THE PAUSED CARTRIDGE"),
                output("STOP     STOP THE PAUSED CARTRIDGE"),
                output("TONE     PLAY A TEST TONE"),
                output("ENV      SET A CHANNEL ENVELOPE"),
                output("GATE     OPEN OR CLOSE A CHANNEL GATE"),
            ]
        );
        assert_eq!(shell.editor, LineEditor::default());
        assert_eq!(
            shell.lines(COLUMNS, ROWS)[1..3],
            [Line::new(0, 1, "> ", TEXT), Line::new(2, 1, "help", TEXT)]
        );
        assert_eq!(
            editor_lines(&shell, COLUMNS),
            [Line::new(0, 17, "> ", TEXT), cursor(2, 17, " ")]
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
        let mut shell = shell_with_rom(Version::new(2, 0, 7));
        run(&mut shell, "INFO");
        assert!(entries(&shell).contains(&output("SYSTEM ROM  2.0.7")));
    }

    #[test]
    fn clear_empties_the_screen_but_keeps_history() {
        let mut shell = new_shell();
        run(&mut shell, "info");
        run(&mut shell, "clear");
        assert_eq!(entries(&shell), []);
        assert_eq!(
            shell.lines(COLUMNS, ROWS),
            [Line::new(0, 0, "> ", TEXT), cursor(2, 0, " ")]
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
                (4, " "),
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
                cursor(2, 1, " "),
            ]
        );
    }

    #[test]
    fn navigation_output_never_shows_host_paths() {
        let fixture = Fixture::new("shell-host-paths");
        let root = fixture.root();
        write_cartridge(&root.join("games/pong"), "Pong");
        write_manifest(&root.join("bad/json"), "{");
        write_manifest(&root.join("bad/fields"), "{}");
        write_manifest(
            &root.join("bad/entry"),
            r#"{"name": "X", "version": "0.1.0", "entry": "../x.luau"}"#,
        );
        write_manifest(
            &root.join("bad/missing"),
            r#"{"name": "X", "version": "0.1.0", "entry": "main.luau"}"#,
        );
        #[cfg(unix)]
        {
            let escape = root.join("bad/escape");
            write_manifest(
                &escape,
                r#"{"name": "X", "version": "0.1.0", "entry": "main.luau"}"#,
            );
            std::fs::write(fixture.outside().join("main.luau"), "").unwrap();
            std::os::unix::fs::symlink(
                fixture.outside().join("main.luau"),
                escape.join("main.luau"),
            )
            .unwrap();
        }
        let mut shell = Shell::new(
            SystemInfo {
                rom_version: Version::new(0, 1, 0),
            },
            fixture.storage(),
        );
        for line in [
            "pwd",
            "ls",
            "cd games",
            "ls ..",
            "pwd",
            "cd nope",
            "cd /b.txt",
            "ls /b.txt",
            "cd ../../..",
            "pwd",
            "ls ../..",
            "ls nope",
            "cd ..\\..",
            "load /games/pong",
            "load /bad/json",
            "load /bad/fields",
            "load /bad/entry",
            "load /bad/missing",
            "load /bad/escape",
            "load /games/owl",
            "load /nope",
            "load /b.txt",
        ] {
            run(&mut shell, line);
        }
        let entries = entries(&shell);
        assert!(entries.contains(&output("/games")));
        assert!(entries.contains(&error("NO SUCH FILE OR DIRECTORY: /games/nope")));
        assert!(entries.contains(&output("LOADED Pong 0.1.0")));
        assert!(entries.contains(&error("NO CART.JSON IN /games/owl")));

        let host_paths = [
            fixture.root().display().to_string(),
            fixture.base().display().to_string(),
            std::env::temp_dir().display().to_string(),
        ];
        for entry in &entries {
            let text = match entry {
                Entry::Banner(text) | Entry::Output(text) | Entry::Error(text) => text,
                Entry::Command(_) | Entry::Log(..) => continue,
            };
            for host in &host_paths {
                assert!(!text.contains(host.as_str()), "{text:?} contains {host:?}");
            }
        }
    }

    #[test]
    fn filesystem_commands_report_unavailable_storage() {
        let mut shell = new_shell();
        shell.scrollback.clear();
        run(&mut shell, "pwd");
        run(&mut shell, "cd games");
        run(&mut shell, "version");
        assert_eq!(
            entries(&shell),
            [
                command("pwd"),
                error("CARTRIDGE STORAGE UNAVAILABLE"),
                command("cd games"),
                error("CARTRIDGE STORAGE UNAVAILABLE"),
                command("version"),
                output("GLAUX G1  SYSTEM ROM 0.1.0"),
            ]
        );
    }

    #[test]
    fn break_is_ignored() {
        let mut shell = shell_with(&typed("ab"));
        let before = shell.clone();
        assert_eq!(
            shell.handle_input(InputEvent::Break, CartridgeSlot::Empty),
            None
        );
        assert_eq!(shell, before);
    }

    #[test]
    fn logged_text_is_split_and_tab_expanded() {
        let mut shell = new_shell();
        shell.scrollback.clear();
        let entry = |level, text: &str| LogEntry {
            level,
            text: text.into(),
        };
        shell.log([
            entry(LogLevel::Info, "hello"),
            entry(LogLevel::Debug, "a\t1\ttrue\tnil"),
        ]);
        shell.log([entry(LogLevel::Warn, "ab\tc\nnext")]);
        assert_eq!(
            entries(&shell),
            [
                Entry::Log(LogLevel::Info, "hello".into()),
                Entry::Log(LogLevel::Debug, "a   1   true    nil".into()),
                Entry::Log(LogLevel::Warn, "ab  c".into()),
                Entry::Log(LogLevel::Warn, "next".into()),
            ]
        );
    }

    #[test]
    fn log_levels_render_in_distinct_colors() {
        let mut shell = new_shell();
        shell.scrollback.clear();
        let levels = [
            LogLevel::Trace,
            LogLevel::Debug,
            LogLevel::Info,
            LogLevel::Warn,
        ];
        shell.log(levels.map(|level| LogEntry {
            level,
            text: "x".into(),
        }));
        let colors: Vec<u8> = shell
            .lines(40, 20)
            .into_iter()
            .filter(|line| line.text == "x")
            .map(|line| line.color)
            .collect();
        assert_eq!(colors, [TRACE, DIM, TEXT, WARN]);
    }

    #[test]
    fn load_and_run_are_returned_as_requests() {
        let fixture = Fixture::new("shell-requests");
        write_cartridge(&fixture.root().join("pong"), "Pong");
        let mut shell = Shell::new(
            SystemInfo {
                rom_version: Version::new(0, 1, 0),
            },
            fixture.storage(),
        );
        send(&mut shell, &typed("load pong"));
        let Some(ShellRequest::Load(cartridge)) = shell.handle_input(Enter, CartridgeSlot::Empty)
        else {
            panic!("expected a load request");
        };
        assert_eq!(cartridge.manifest().name, "Pong");

        send(&mut shell, &typed("run"));
        assert_eq!(
            shell.handle_input(Enter, CartridgeSlot::Loaded(&cartridge)),
            Some(ShellRequest::Run)
        );
        send(&mut shell, &typed("run"));
        assert_eq!(shell.handle_input(Enter, CartridgeSlot::Empty), None);
        assert_eq!(entries(&shell).last(), Some(&error("NO CARTRIDGE LOADED")));
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
    fn tab_completes_unload() {
        let mut shell = shell_with(&typed("unl"));
        send(&mut shell, &[Tab]);
        assert_eq!(shell.editor.text(), "unload ");
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
            [
                banner(),
                command(""),
                output(
                    "HELP  CLEAR  INFO  VERSION  PWD  LS  CD  LOAD  UNLOAD  RUN  RESUME  STOP  TONE  ENV  GATE"
                )
            ]
        );
        assert_eq!(shell.editor, LineEditor::default());
    }
}
