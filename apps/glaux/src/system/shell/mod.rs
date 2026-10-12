//! The system shell: a command line over a scrollback of earlier output.

mod commands;
mod completion;
mod history;
mod parser;
mod scrollback;

use glaux_input::InputEvent;
use glaux_machine::psg::{Adsr, CHANNELS, Waveform};
use glaux_script::{LogEntry, LogLevel};

use super::line_editor::LineEditor;
use super::{ACCENT, BACKGROUND, DIM, ERROR, Line, SystemInfo, TEXT, TRACE, WARN};
use crate::cartridge::{CartridgeManifest, Language, LoadedCartridge};
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

const BANNER: &str = "Ready. Type 'help' for a list of commands.";
const PROMPT: &str = "> ";

/// Shown whenever a cartridge in an experimental language is created or
/// run.
pub(super) const EXPERIMENTAL_LANGUAGE_WARNING: &str =
    "RHAI SUPPORT IS EXPERIMENTAL. THE SCRIPTING API MAY CHANGE WITHOUT NOTICE.";
/// Shell ticks the cursor spends in each blink phase: 600 ms at 60 Hz.
pub(super) const BLINK_TICKS: u32 = 36;

/// A cartridge lifecycle change asked for by a shell command and carried
/// out by the system.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShellRequest {
    /// Make this validated cartridge the loaded one, replacing any other,
    /// then announce it with [`Shell::announce_loaded`].
    Load {
        cartridge: Box<LoadedCartridge>,
        origin: LoadOrigin,
    },
    /// Start the loaded cartridge.
    Run,
    /// Remove the loaded cartridge from the shell.
    Unload,
    /// Continue the paused cartridge with the VM it had.
    Resume,
    /// Dispose of the paused cartridge's VM, keeping the cartridge loaded.
    Stop,
}

/// Where a cartridge asked to be loaded comes from, which decides how its
/// loading is announced.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoadOrigin {
    /// A cartridge that already existed in storage.
    Existing,
    /// A cartridge the shell has just created.
    Created,
}

/// The cartridge the system holds, as the shell sees it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CartridgeSlot<'a> {
    Empty,
    /// Loaded, with no VM. `unsaved` tells whether the editors hold changes
    /// to it that are not written back.
    Loaded {
        cartridge: &'a LoadedCartridge,
        unsaved: bool,
    },
    /// Its VM is alive but paused while the shell is active.
    Paused(&'a LoadedCartridge),
}

impl<'a> CartridgeSlot<'a> {
    /// A slot that is empty or holds a loaded cartridge with `unsaved`
    /// edits or none.
    pub fn loaded(cartridge: Option<&'a LoadedCartridge>, unsaved: bool) -> Self {
        cartridge.map_or(Self::Empty, |cartridge| Self::Loaded { cartridge, unsaved })
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
            InputEvent::Break
                | InputEvent::ButtonDown(_)
                | InputEvent::ButtonUp(_)
                | InputEvent::KeyDown(_)
                | InputEvent::KeyUp(_)
                | InputEvent::NextView
                | InputEvent::PreviousView
                | InputEvent::Undo
                | InputEvent::Redo
                | InputEvent::Save
                | InputEvent::OpenFile
                | InputEvent::CloseDocument
                | InputEvent::NextDocument
                | InputEvent::PreviousDocument
        ) && !event.is_pointer()
        {
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
            // The system handles Break around the shell: it opens the
            // workspace from the idle shell, and a paused cartridge is left
            // with the RESUME and STOP commands.
            InputEvent::Break => {}
            // Game buttons and keys belong to a running cartridge; views and
            // the pointer belong to the workspace.
            InputEvent::ButtonDown(_)
            | InputEvent::ButtonUp(_)
            | InputEvent::KeyDown(_)
            | InputEvent::KeyUp(_)
            | InputEvent::NextView
            | InputEvent::PreviousView
            | InputEvent::Undo
            | InputEvent::Redo
            | InputEvent::Save
            | InputEvent::OpenFile
            | InputEvent::CloseDocument
            | InputEvent::NextDocument
            | InputEvent::PreviousDocument
            | InputEvent::PointerMove(_)
            | InputEvent::PointerDown { .. }
            | InputEvent::PointerUp { .. }
            | InputEvent::PointerCancel
            | InputEvent::Wheel { .. }
            | InputEvent::WheelHorizontal { .. } => {}
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

    /// Reports that the cartridge described by `manifest` has become the
    /// loaded cartridge. The system calls this only once the change has
    /// happened, so success is never reported early.
    pub(super) fn announce_loaded(&mut self, manifest: &CartridgeManifest, origin: LoadOrigin) {
        match origin {
            LoadOrigin::Existing => {
                self.report(format!("Loaded {}", cartridge_identity(manifest)));
            }
            LoadOrigin::Created => {
                self.warn_if_experimental(manifest.runtime.language);
                for line in [
                    "Your new cartridge is ready!",
                    manifest.cartridge.name.as_str(),
                    "",
                    "Every great game starts somewhere.",
                    "This one starts with you.",
                    "",
                    "Try 'run' to see your first creation.",
                ] {
                    self.report(line);
                }
            }
        }
    }

    /// Adds a system warning line to the scrollback.
    pub(super) fn report_warning(&mut self, line: impl Into<String>) {
        self.scrollback.push(Entry::Warning(line.into()));
    }

    /// Warns that cartridges in `language` use an API that may change, if
    /// they do.
    pub(super) fn warn_if_experimental(&mut self, language: Language) {
        if language.is_experimental() {
            tracing::warn!(
                language = language.name(),
                "experimental scripting language"
            );
            self.report_warning(EXPERIMENTAL_LANGUAGE_WARNING);
        }
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
                Entry::Warning(text) => format!("WARN: {text}"),
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
                let parser::ParseError::UnterminatedQuote { quote, .. } = err;
                self.scrollback.push(Entry::Output(format!(
                    "Close the quoted text with another {quote}"
                )));
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
                Entry::Warning(text) => (text, WARN, false),
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
        Entry::Banner(text)
        | Entry::Output(text)
        | Entry::Error(text)
        | Entry::Warning(text)
        | Entry::Log(_, text) => text.chars().count(),
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
mod tests;
