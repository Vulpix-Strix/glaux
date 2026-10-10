use super::*;
use crate::cartridge::fixture::{manifest_ron, write_cartridge, write_manifest};
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
            output("help     Show available commands"),
            output("clear    Clear the console"),
            output("info     Show system information"),
            output("version  Show the System ROM version"),
            output("pwd      Show the current directory"),
            output("ls       List a directory"),
            output("cd       Change the current directory"),
            output("new      Create a new cartridge"),
            output("load     Load a cartridge"),
            output("unload   Unload the loaded cartridge"),
            output("run      Run the loaded cartridge"),
            output("resume   Resume the paused cartridge"),
            output("stop     Stop the paused cartridge"),
            output("tone     Play a test tone"),
            output("env      Set a channel envelope"),
            output("gate     Open or close a channel gate"),
        ]
    );
    assert_eq!(shell.editor, LineEditor::default());
    assert_eq!(
        shell.lines(COLUMNS, ROWS)[1..3],
        [Line::new(0, 1, "> ", TEXT), Line::new(2, 1, "help", TEXT)]
    );
    assert_eq!(
        editor_lines(&shell, COLUMNS),
        [Line::new(0, 18, "> ", TEXT), cursor(2, 18, " ")]
    );
}

#[test]
fn unknown_command_is_an_error_entry() {
    let mut shell = new_shell();
    run(&mut shell, "Unknown");
    assert_eq!(
        entries(&shell),
        [
            banner(),
            command("Unknown"),
            error("Unknown command: Unknown")
        ]
    );
    assert_eq!(
        shell.lines(COLUMNS, ROWS)[3],
        Line::new(0, 2, "Unknown command: Unknown", ERROR)
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
            error("Unterminated \" quote"),
            output("Close the quoted text with another \""),
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
            (1, "Unknown command: c8"),
            (2, "> "),
            (2, "c9"),
            (3, "Unknown command: c9"),
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
    write_manifest(&root.join("bad/fields"), "()");
    write_manifest(&root.join("bad/entry"), &manifest_ron("X", "../x.luau"));
    write_manifest(&root.join("bad/missing"), &manifest_ron("X", "main.luau"));
    #[cfg(unix)]
    {
        let escape = root.join("bad/escape");
        write_manifest(&escape, &manifest_ron("X", "main.luau"));
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
        "cd /games",
        "new pong",
        "new Pong",
    ] {
        run(&mut shell, line);
    }
    let entries = entries(&shell);
    assert!(entries.contains(&output("/games")));
    assert!(entries.contains(&error("No such file or directory: /games/nope")));
    assert!(entries.contains(&error("No cartridge.ron in /games/owl")));
    assert!(entries.contains(&error("/games/pong already exists")));

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
    let Some(ShellRequest::Load { cartridge, .. }) =
        shell.handle_input(Enter, CartridgeSlot::Empty)
    else {
        panic!("expected a load request");
    };
    assert_eq!(cartridge.manifest().cartridge.name, "Pong");

    send(&mut shell, &typed("run"));
    assert_eq!(
        shell.handle_input(
            Enter,
            CartridgeSlot::Loaded {
                cartridge: &cartridge,
                unsaved: false,
            }
        ),
        Some(ShellRequest::Run)
    );
    send(&mut shell, &typed("run"));
    assert_eq!(shell.handle_input(Enter, CartridgeSlot::Empty), None);
    assert_eq!(entries(&shell).last(), Some(&error("No cartridge loaded")));
}

#[test]
fn loads_are_announced_by_origin() {
    let (_fixture, pong) = {
        let fixture = Fixture::new("shell-announce");
        write_cartridge(&fixture.root().join("pong"), "Pong");
        let pong = LoadedCartridge::load(&fixture.storage(), "pong").unwrap();
        (fixture, pong)
    };
    let mut shell = new_shell();
    shell.scrollback.clear();
    shell.announce_loaded(pong.manifest(), LoadOrigin::Existing);
    assert_eq!(entries(&shell), [output("Loaded Pong 0.1.0")]);

    shell.scrollback.clear();
    shell.announce_loaded(pong.manifest(), LoadOrigin::Created);
    assert_eq!(
        shell.transcript(),
        [
            "Your new cartridge is ready!",
            "Pong",
            "",
            "Every great game starts somewhere.",
            "This one starts with you.",
            "",
            "Try 'run' to see your first creation.",
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
        [
            banner(),
            command(""),
            output(
                "help  clear  info  version  pwd  ls  cd  new  load  unload  run  resume  stop  tone  env  gate"
            )
        ]
    );
    assert_eq!(shell.editor, LineEditor::default());
}
