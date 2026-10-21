use super::super::parser::parse;
use super::*;
use crate::cartridge::Language;
use crate::cartridge::fixture::{manifest_ron, write_cartridge, write_manifest};
use crate::cartridge_fs::fixture::Fixture;
use glaux_machine::psg::MAX_ENVELOPE_SAMPLES;
use pretty_assertions::assert_eq;
use semver::Version;

/// Runs `line` on an empty scrollback and returns what it printed.
fn run_in(line: &str, rom_version: Version, cartridges: &mut CartridgeStorage) -> Vec<Entry> {
    run_loaded(line, rom_version, cartridges, &mut None)
}

/// Runs `line` with `loaded` as the loaded cartridge and returns what it
/// printed and the request it made.
fn execute_line(
    line: &str,
    rom_version: Version,
    cartridges: &mut CartridgeStorage,
    slot: CartridgeSlot<'_>,
) -> (Vec<Entry>, Option<ShellRequest>) {
    let mut scrollback = Scrollback::default();
    let info = SystemInfo { rom_version };
    let mut request = None;
    let mut ctx = Context {
        scrollback: &mut scrollback,
        info: &info,
        cartridges,
        slot,
        request: &mut request,
        voices: &mut [Voice::default(); CHANNELS],
    };
    execute(&parse(line).unwrap().unwrap(), &mut ctx);
    (scrollback.iter().cloned().collect(), request)
}

/// Like [`run_in`], with `loaded` as the loaded cartridge. A load request
/// is applied to `loaded` as the system would.
fn run_loaded(
    line: &str,
    rom_version: Version,
    cartridges: &mut CartridgeStorage,
    loaded: &mut Option<LoadedCartridge>,
) -> Vec<Entry> {
    let (entries, request) = execute_line(
        line,
        rom_version,
        cartridges,
        CartridgeSlot::loaded(loaded.as_ref(), false),
    );
    if let Some(ShellRequest::Load { cartridge, .. }) = request {
        *loaded = Some(*cartridge);
    }
    entries
}

fn no_cartridges() -> CartridgeStorage {
    CartridgeStorage::Unavailable
}

fn run_with_rom(line: &str, rom_version: Version) -> Vec<Entry> {
    run_in(line, rom_version, &mut no_cartridges())
}

fn run_fs(line: &str, cartridges: &mut CartridgeStorage) -> Vec<Entry> {
    run_in(line, Version::new(0, 3, 1), cartridges)
}

fn run(line: &str) -> Vec<Entry> {
    run_with_rom(line, Version::new(0, 3, 1))
}

fn output(lines: &[&str]) -> Vec<Entry> {
    lines.iter().map(|&l| Entry::Output(l.into())).collect()
}

fn error(line: &str) -> Vec<Entry> {
    vec![Entry::Error(line.into())]
}

/// An error explaining `problem`, followed by `hint` on how to fix it.
fn hinted(problem: &str, hint: &str) -> Vec<Entry> {
    vec![Entry::Error(problem.into()), Entry::Output(hint.into())]
}

#[test]
fn names_and_aliases_are_unique_and_lowercase() {
    let mut all: Vec<&str> = COMMANDS
        .iter()
        .flat_map(|c| std::iter::once(c.name).chain(c.aliases.iter().copied()))
        .collect();
    assert!(all.iter().all(|n| *n == n.to_lowercase()));
    let len = all.len();
    all.sort_unstable();
    all.dedup();
    assert_eq!(all.len(), len);
}

#[test]
fn help_lists_every_command() {
    assert_eq!(
        run("help"),
        output(&[
            "help     Show available commands",
            "clear    Clear the console",
            "info     Show system information",
            "credits  Show the credits and licenses",
            "docs     Open the documentation",
            "version  Show the System ROM version",
            "pwd      Show the current directory",
            "ls       List a directory",
            "cd       Change the current directory",
            "new      Create a new cartridge",
            "load     Load a cartridge",
            "unload   Unload the loaded cartridge",
            "run      Run the loaded cartridge",
            "resume   Resume the paused cartridge",
            "stop     Stop the paused cartridge",
            "tone     Play a test tone",
            "env      Set a channel envelope",
            "gate     Open or close a channel gate",
        ])
    );
}

#[test]
fn help_describes_one_command() {
    assert_eq!(
        run("help CLEAR"),
        output(&["Usage: clear", "Clear the console", "Aliases: cls"])
    );
    assert_eq!(
        run("? info"),
        output(&["Usage: info", "Show system information"])
    );
}

#[test]
fn help_for_unknown_command_is_an_error() {
    assert_eq!(run("help nOpe"), error("Unknown command: nOpe"));
}

#[test]
fn info_reports_system_state() {
    assert_eq!(
        run("info"),
        output(&[
            "MACHINE     GLAUX G1",
            "SYSTEM ROM  0.3.1",
            "DISPLAY     320x240 INDEXED, 256 COLORS",
            "TICK RATE   60 HZ",
            "RUNTIME     NONE",
            "CARTRIDGE   NONE",
            "STATE       IDLE",
        ])
    );
}

#[test]
fn version_reports_identity() {
    assert_eq!(run("version"), output(&["GLAUX G1  SYSTEM ROM 0.3.1"]));
    assert_eq!(run("VER"), output(&["GLAUX G1  SYSTEM ROM 0.3.1"]));
}

#[test]
fn info_and_version_use_the_injected_rom_version() {
    let rom = || Version::new(3, 7, 0);
    assert!(run_with_rom("info", rom()).contains(&Entry::Output("SYSTEM ROM  3.7.0".into())));
    assert_eq!(
        run_with_rom("version", rom()),
        output(&["GLAUX G1  SYSTEM ROM 3.7.0"])
    );
}

#[test]
fn clear_empties_the_scrollback() {
    let mut scrollback = Scrollback::default();
    scrollback.push(Entry::Output("old".into()));
    let info = SystemInfo {
        rom_version: Version::new(0, 1, 0),
    };
    let mut cartridges = no_cartridges();
    let mut ctx = Context {
        scrollback: &mut scrollback,
        info: &info,
        cartridges: &mut cartridges,
        slot: CartridgeSlot::Empty,
        request: &mut None,
        voices: &mut [Voice::default(); CHANNELS],
    };
    execute(&parse("clear").unwrap().unwrap(), &mut ctx);
    assert_eq!(scrollback.iter().len(), 0);
}

#[test]
fn unknown_command_keeps_the_typed_name() {
    assert_eq!(run("Foo"), error("Unknown command: Foo"));
    assert_eq!(run("\"Super Owl\" x"), error("Unknown command: Super Owl"));
}

#[test]
fn arguments_to_commands_without_any_are_refused() {
    for name in [
        "clear", "info", "version", "pwd", "unload", "run", "resume", "stop",
    ] {
        assert_eq!(
            run(&format!("{name} x")),
            error(&format!("{name} doesn't take any arguments")),
            "{name}"
        );
    }
    assert_eq!(run("CLS now"), error("clear doesn't take any arguments"));
}

#[test]
fn unquoted_paths_with_spaces_get_a_quoted_suggestion() {
    assert_eq!(
        run("load my games/pong"),
        hinted(
            "Paths with spaces need quotes",
            r#"Try: load "my games/pong""#
        )
    );
    assert_eq!(
        run("cd  My  Games  "),
        hinted("Paths with spaces need quotes", r#"Try: cd "My  Games""#)
    );
    assert_eq!(
        run("dir a b"),
        hinted("Paths with spaces need quotes", r#"Try: ls "a b""#)
    );
    // With quotes already typed, the intended split is unclear.
    assert_eq!(
        run(r#"load "my games" pong"#),
        hinted("Too many arguments", "Usage: load <path> [--discard]")
    );
    // Flags keep their place in front of the quoted path.
    assert_eq!(
        run("load --discard my games/pong"),
        hinted(
            "Paths with spaces need quotes",
            r#"Try: load --discard "my games/pong""#
        )
    );
}

#[test]
fn other_extra_arguments_show_the_usage() {
    assert_eq!(
        run("help a b"),
        hinted("Too many arguments", "Usage: help [<command>]")
    );
}

#[test]
fn missing_arguments_are_named() {
    assert_eq!(
        run("load"),
        hinted("Missing <path>", "Usage: load <path> [--discard]")
    );
    assert_eq!(
        run("new"),
        hinted("Missing <id>", &format!("Usage: {NEW_USAGE}"))
    );
    assert_eq!(missing_argument("x <a> [<b>] <c>", 1), "Missing <c>");
    assert_eq!(missing_argument("x <a>", 5), "Missing an argument");
}

#[test]
fn filesystem_commands_need_storage() {
    for line in [
        "pwd",
        "ls",
        "ls games",
        "dir",
        "cd",
        "cd games",
        "load games",
        "new games",
    ] {
        assert_eq!(run(line), error("Cartridge storage unavailable"), "{line}");
    }
}

#[test]
fn ls_marks_directories() {
    let fixture = Fixture::new("cmd-ls");
    let mut fs = fixture.storage();
    assert_eq!(
        run_fs("ls", &mut fs),
        output(&["games/", "tools/", "Zeta/", "A.txt", "b.txt", "readme"])
    );
    assert_eq!(run_fs("DIR games", &mut fs), output(&["owl/"]));
    assert_eq!(run_fs("ls games/owl", &mut fs), []);
    assert_eq!(
        run_fs("ls nope", &mut fs),
        error("No such file or directory: /nope")
    );
    assert_eq!(
        run_fs("ls b.txt", &mut fs),
        error("Not a directory: /b.txt")
    );
}

#[test]
fn cd_changes_what_pwd_reports() {
    let fixture = Fixture::new("cmd-cd");
    let mut fs = fixture.storage();
    assert_eq!(run_fs("cd games/owl", &mut fs), []);
    assert_eq!(run_fs("pwd", &mut fs), output(&["/games/owl"]));
    assert_eq!(run_fs("cd ..", &mut fs), []);
    assert_eq!(run_fs("pwd", &mut fs), output(&["/games"]));
    assert_eq!(run_fs("cd", &mut fs), []);
    assert_eq!(run_fs("pwd", &mut fs), output(&["/"]));
}

#[test]
fn cd_failures_are_errors_and_keep_the_directory() {
    let fixture = Fixture::new("cmd-cd-errors");
    let mut fs = fixture.storage();
    run_fs("cd games", &mut fs);
    assert_eq!(
        run_fs("cd nope", &mut fs),
        error("No such file or directory: /games/nope")
    );
    assert_eq!(
        run_fs("cd /b.txt", &mut fs),
        error("Not a directory: /b.txt")
    );
    assert_eq!(run_fs("pwd", &mut fs), output(&["/games"]));
}

/// Runs `line` against `cartridges` and the loaded cartridge in `loaded`.
fn run_load(
    line: &str,
    cartridges: &mut CartridgeStorage,
    loaded: &mut Option<LoadedCartridge>,
) -> Vec<Entry> {
    run_loaded(line, Version::new(0, 3, 1), cartridges, loaded)
}

fn info_state(entries: &[Entry]) -> Vec<Entry> {
    entries[4..].to_vec()
}

#[test]
fn load_reports_the_cartridge_and_info_shows_it() {
    let fixture = Fixture::new("cmd-load");
    write_cartridge(&fixture.root().join("games/pong"), "Pong");
    let mut fs = fixture.storage();
    let mut loaded = None;

    assert_eq!(
        info_state(&run_load("info", &mut fs, &mut loaded)),
        output(&["RUNTIME     NONE", "CARTRIDGE   NONE", "STATE       IDLE"])
    );
    run_load("cd games", &mut fs, &mut loaded);
    assert_eq!(run_load("load pong", &mut fs, &mut loaded), []);
    assert_eq!(
        info_state(&run_load("info", &mut fs, &mut loaded)),
        output(&[
            "RUNTIME     LUAU",
            "CARTRIDGE   Pong 0.1.0",
            "STATE       LOADED"
        ])
    );
}

#[test]
fn failed_load_keeps_the_loaded_cartridge() {
    let fixture = Fixture::new("cmd-load-keep");
    write_cartridge(&fixture.root().join("games/pong"), "Pong");
    write_manifest(
        &fixture.root().join("games/owl"),
        &manifest_ron("Owl", "code/main.luau").replace("format: 1", "format: 2"),
    );
    let mut fs = fixture.storage();
    let mut loaded = None;

    assert_eq!(
        run_load("load games/owl", &mut fs, &mut loaded),
        error("cartridge.ron: unsupported format 2")
    );
    assert_eq!(loaded, None);

    run_load("load /games/pong", &mut fs, &mut loaded);
    let pong = loaded.clone();
    for line in [
        "load games/owl",
        "load nope",
        "load b.txt",
        "load tools",
        "new games",
        "new Bad",
    ] {
        let entries = run_load(line, &mut fs, &mut loaded);
        assert!(matches!(entries.first(), Some(Entry::Error(_))), "{line}");
        assert_eq!(loaded, pong, "{line}");
    }
    assert!(
        run_load("info", &mut fs, &mut loaded)
            .contains(&Entry::Output("CARTRIDGE   Pong 0.1.0".into()))
    );
}

#[test]
fn load_over_a_loaded_cartridge_requests_the_new_one() {
    let fixture = Fixture::new("cmd-load-replace");
    write_cartridge(&fixture.root().join("pong"), "Pong");
    write_cartridge(&fixture.root().join("owl"), "Owl");
    let mut fs = fixture.storage();
    let mut pong = None;
    run_load("load pong", &mut fs, &mut pong);

    let (entries, request) = execute_line(
        "load owl",
        Version::new(0, 3, 1),
        &mut fs,
        CartridgeSlot::loaded(pong.as_ref(), false),
    );
    assert_eq!(entries, []);
    match request {
        Some(ShellRequest::Load { cartridge, origin }) => {
            assert_eq!(cartridge.manifest().cartridge.name, "Owl");
            assert_eq!(origin, LoadOrigin::Existing);
        }
        other => panic!("expected a load request, got {other:?}"),
    }
}

#[test]
fn run_needs_a_loaded_cartridge() {
    assert_eq!(run("run"), error("No cartridge loaded"));
    let fixture = Fixture::new("cmd-run-none");
    let (entries, request) = execute_line(
        "RUN",
        Version::new(0, 3, 1),
        &mut fixture.storage(),
        CartridgeSlot::Empty,
    );
    assert_eq!((entries, request), (error("No cartridge loaded"), None));
}

#[test]
fn unload_needs_and_requests_a_loaded_cartridge() {
    let fixture = Fixture::new("cmd-unload");
    let (_, pong) = pong("cmd-unload-loaded");
    assert_eq!(
        execute_line(
            "unload",
            Version::new(0, 3, 1),
            &mut fixture.storage(),
            CartridgeSlot::Empty,
        ),
        (error("No cartridge loaded"), None)
    );
    assert_eq!(
        execute_line(
            "UNLOAD",
            Version::new(0, 3, 1),
            &mut fixture.storage(),
            CartridgeSlot::Loaded {
                cartridge: &pong,
                unsaved: false,
            },
        ),
        (vec![], Some(ShellRequest::Unload))
    );
}

#[test]
fn run_requests_the_loaded_cartridge_to_start() {
    let fixture = Fixture::new("cmd-run-request");
    write_cartridge(&fixture.root().join("pong"), "Pong");
    let mut fs = fixture.storage();
    let mut loaded = None;
    run_load("load pong", &mut fs, &mut loaded);
    let (entries, request) = execute_line(
        "run",
        Version::new(0, 3, 1),
        &mut fs,
        CartridgeSlot::loaded(loaded.as_ref(), false),
    );
    assert_eq!((entries, request), (vec![], Some(ShellRequest::Run)));
}

#[test]
fn load_is_a_request() {
    let fixture = Fixture::new("cmd-load-request");
    write_cartridge(&fixture.root().join("pong"), "Pong");
    let (entries, request) = execute_line(
        "load pong",
        Version::new(0, 3, 1),
        &mut fixture.storage(),
        CartridgeSlot::Empty,
    );
    assert_eq!(entries, []);
    assert!(
        matches!(
            &request,
            Some(ShellRequest::Load { cartridge, origin: LoadOrigin::Existing })
                if cartridge.manifest().cartridge.name == "Pong"
        ),
        "{request:?}"
    );
}

/// The `Pong` cartridge, loaded from a fresh fixture named `name`.
fn pong(name: &str) -> (Fixture, LoadedCartridge) {
    let fixture = Fixture::new(name);
    write_cartridge(&fixture.root().join("pong"), "Pong");
    let cartridge = LoadedCartridge::load(&fixture.storage(), "pong").unwrap();
    (fixture, cartridge)
}

fn execute_paused(
    line: &str,
    fixture: &Fixture,
    paused: &LoadedCartridge,
) -> (Vec<Entry>, Option<ShellRequest>) {
    execute_line(
        line,
        Version::new(0, 3, 1),
        &mut fixture.storage(),
        CartridgeSlot::Paused(paused),
    )
}

#[test]
fn info_reports_a_paused_cartridge() {
    let (fixture, pong) = pong("cmd-info-paused");
    let (entries, _) = execute_paused("info", &fixture, &pong);
    assert_eq!(
        info_state(&entries),
        output(&[
            "RUNTIME     LUAU",
            "CARTRIDGE   Pong 0.1.0",
            "STATE       PAUSED"
        ])
    );
}

#[test]
fn resume_and_stop_request_the_paused_cartridge() {
    let (fixture, pong) = pong("cmd-resume-stop");
    assert_eq!(
        execute_paused("resume", &fixture, &pong),
        (vec![], Some(ShellRequest::Resume))
    );
    assert_eq!(
        execute_paused("STOP", &fixture, &pong),
        (vec![], Some(ShellRequest::Stop))
    );
}

#[test]
fn resume_and_stop_need_a_paused_cartridge() {
    let (fixture, pong) = pong("cmd-resume-stop-none");
    for slot in [
        CartridgeSlot::Empty,
        CartridgeSlot::Loaded {
            cartridge: &pong,
            unsaved: false,
        },
    ] {
        for line in ["resume", "stop"] {
            let result = execute_line(line, Version::new(0, 3, 1), &mut fixture.storage(), slot);
            assert_eq!(result, (error("No paused cartridge"), None), "{line}");
        }
    }
}

#[test]
fn load_and_run_refuse_to_replace_a_paused_cartridge() {
    let (fixture, pong) = pong("cmd-paused-refuse");
    for line in [
        "run",
        "unload",
        "load pong",
        "load /does/not/exist",
        "new paused-game",
    ] {
        assert_eq!(
            execute_paused(line, &fixture, &pong),
            (error("Cartridge paused: resume or stop it first"), None),
            "{line}"
        );
    }
    assert!(!fixture.root().join("paused-game").exists());
}

/// Runs the `new` command `line` at the root of `fixture` and returns
/// what it printed and the cartridge it asked to load as created.
fn run_new(fixture: &Fixture, line: &str) -> (Vec<Entry>, Option<LoadedCartridge>) {
    let (entries, request) = execute_line(
        line,
        Version::new(0, 3, 1),
        &mut fixture.storage(),
        CartridgeSlot::Empty,
    );
    let created = match request {
        None => None,
        Some(ShellRequest::Load {
            cartridge,
            origin: LoadOrigin::Created,
        }) => Some(*cartridge),
        Some(other) => panic!("unexpected request {other:?}"),
    };
    (entries, created)
}

/// Every entry name in the host directory `dir`, sorted.
fn host_listing(dir: &std::path::Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().into_string().unwrap())
        .collect();
    names.sort();
    names
}

#[test]
fn new_creates_a_cartridge_and_leaves_the_announcement_to_the_system() {
    let fixture = Fixture::new("cmd-new");
    for (line, id, name) in [
        ("new my-first-game", "my-first-game", "My First Game"),
        ("NEW other-game", "other-game", "Other Game"),
    ] {
        let (entries, created) = run_new(&fixture, line);
        assert_eq!(entries, [], "{line}");
        assert_eq!(
            created.map(|c| c.manifest().cartridge.name.clone()),
            Some(name.to_owned()),
            "{line}"
        );
        assert!(fixture.root().join(id).join("cartridge.ron").is_file());
    }
}

#[test]
fn new_lang_chooses_the_language_and_luau_is_the_default() {
    let fixture = Fixture::new("cmd-new-lang");
    for (line, id, language, entry, absent) in [
        (
            "new plain",
            "plain",
            Language::Luau,
            "code/main.luau",
            "code/main.rhai",
        ),
        (
            "new lua-game --lang luau",
            "lua-game",
            Language::Luau,
            "code/main.luau",
            "code/main.rhai",
        ),
        (
            "new rhai-game --lang rhai",
            "rhai-game",
            Language::Rhai,
            "code/main.rhai",
            "code/main.luau",
        ),
        (
            "new --lang RHAI loud",
            "loud",
            Language::Rhai,
            "code/main.rhai",
            "code/main.luau",
        ),
    ] {
        let (entries, created) = run_new(&fixture, line);
        assert_eq!(entries, [], "{line}");
        let manifest = created.unwrap().manifest().clone();
        assert_eq!(manifest.runtime.language, language, "{line}");
        assert_eq!(manifest.runtime.entry, entry, "{line}");
        let dir = fixture.root().join(id);
        assert!(dir.join(entry).is_file(), "{line}");
        assert!(!dir.join(absent).exists(), "{line}");
    }
}

#[test]
fn new_takes_quoted_option_values() {
    let fixture = Fixture::new("cmd-new-options");
    let (entries, created) = run_new(
        &fixture,
        r#"new space-adventure --name "Space Adventure" --version 1.2.0 --author "Cuervolu""#,
    );
    assert_eq!(entries, []);
    let manifest = created.unwrap().manifest().clone();
    assert_eq!(manifest.cartridge.name, "Space Adventure");
    assert_eq!(manifest.cartridge.version, Version::new(1, 2, 0));
    assert_eq!(manifest.cartridge.author.as_deref(), Some("Cuervolu"));
}

#[test]
fn new_errors_create_nothing() {
    let fixture = Fixture::new("cmd-new-errors");
    let before = host_listing(&fixture.root());
    for (line, message) in [
        (
            "new a --version 1.0",
            error("Invalid version '1.0': use semver, like 1.2.0"),
        ),
        (r#"new a --name """#, error("The name must not be empty")),
        ("new a --name", error("Option --name needs a value")),
        (
            "new a --color red",
            hinted(
                "Unknown option: --color",
                "Options: --name, --version, --author, --lang, --discard",
            ),
        ),
        (
            "new a --name x --name=y",
            error("Option --name given more than once"),
        ),
        ("new games", error("/games already exists")),
        (
            "new a --lang python",
            error("Unsupported language 'python': use luau or rhai"),
        ),
        ("new a --lang", error("Option --lang needs a value")),
        (
            "new a --lang rhai --lang luau",
            error("Option --lang given more than once"),
        ),
        (
            "new a --language rhai",
            hinted(
                "Unknown option: --language",
                "Options: --name, --version, --author, --lang, --discard",
            ),
        ),
    ] {
        assert_eq!(run_new(&fixture, line), (message, None), "{line}");
    }
    assert_eq!(host_listing(&fixture.root()), before);
}

/// What `new` printed for `line`, which must create nothing.
fn new_diagnostic(fixture: &Fixture, line: &str) -> Vec<Entry> {
    let before = host_listing(&fixture.root());
    let (entries, created) = run_new(fixture, line);
    assert!(created.is_none(), "{line}");
    assert_eq!(host_listing(&fixture.root()), before, "{line}");
    entries
}

#[test]
fn new_suggests_quoting_values_with_spaces() {
    let fixture = Fixture::new("cmd-new-quotes");
    let quotes = "Values with spaces need quotes";
    for (line, suggestion) in [
        (
            "new my-game --name My New Game",
            r#"Try: new my-game --name "My New Game""#,
        ),
        (
            "new my-game --name=My New Game",
            r#"Try: new my-game --name "My New Game""#,
        ),
        (
            "new my-game --name Space Owl --author Ana Ruiz --version 1.0.0",
            r#"Try: new my-game --name "Space Owl" --author "Ana Ruiz" --version 1.0.0"#,
        ),
        (
            r#"new my-game --name "Say" Hi"#,
            r#"Try: new my-game --name "Say Hi""#,
        ),
        (
            r#"new my-game --name 'Say "Hi"' Now"#,
            r#"Try: new my-game --name "Say \"Hi\" Now""#,
        ),
        ("new --name My Game", r#"Try: new my-game --name "My Game""#),
        (
            "new --author Ana Ruiz",
            r#"Try: new my-game --author "Ana Ruiz""#,
        ),
    ] {
        assert_eq!(
            new_diagnostic(&fixture, line),
            hinted(quotes, suggestion),
            "{line}"
        );
    }
}

#[test]
fn new_suggests_an_id_for_a_name_typed_in_its_place() {
    let fixture = Fixture::new("cmd-new-id");
    let one_word = "The id must be one word, without spaces";
    for (line, expected) in [
        ("new My Game", hinted(one_word, "Try: new my-game")),
        (
            "new my game",
            hinted(one_word, r#"Try: new my-game --name "my game""#),
        ),
        (
            "new My-Game",
            hinted(
                "Invalid cartridge id 'My-Game': use lowercase letters, digits and dashes, like my-game",
                "Try: new my-game",
            ),
        ),
        (
            "new Space_Owl --name Owl",
            hinted(
                "Invalid cartridge id 'Space_Owl': use lowercase letters, digits and dashes, like my-game",
                "Try: new space-owl --name Owl",
            ),
        ),
        (
            "new --name Owl",
            hinted("Missing <id>", "Try: new my-game --name Owl"),
        ),
    ] {
        assert_eq!(new_diagnostic(&fixture, line), expected, "{line}");
    }
}

#[test]
fn unclear_extra_words_for_new_show_the_usage() {
    let fixture = Fixture::new("cmd-new-unclear");
    let unclear = hinted(
        "Too many arguments; values with spaces need quotes",
        &format!("Usage: {NEW_USAGE}"),
    );
    for line in ["new my game --name Owl", "new Ñandú Quest"] {
        assert_eq!(new_diagnostic(&fixture, line), unclear, "{line}");
    }
    // Positional words may still come anywhere when there is only one.
    let (_, created) = run_new(&fixture, r#"new --name "Late Id" late-id"#);
    assert_eq!(
        created.map(|c| c.manifest().cartridge.name.clone()),
        Some("Late Id".into())
    );
}

/// Runs `line` with `voices` as the shell's voices and returns what it
/// printed.
fn run_voice(line: &str, voices: &mut [Voice; CHANNELS]) -> Vec<Entry> {
    let mut scrollback = Scrollback::default();
    let info = SystemInfo {
        rom_version: Version::new(0, 3, 1),
    };
    let mut ctx = Context {
        scrollback: &mut scrollback,
        info: &info,
        cartridges: &mut no_cartridges(),
        slot: CartridgeSlot::Empty,
        request: &mut None,
        voices,
    };
    execute(&parse(line).unwrap().unwrap(), &mut ctx);
    scrollback.iter().cloned().collect()
}

fn silent() -> [Voice; CHANNELS] {
    [Voice::default(); CHANNELS]
}

fn tone(waveform: Waveform, frequency: u16, volume: u8) -> Option<Tone> {
    Some(Tone {
        waveform,
        frequency,
        volume,
    })
}

fn tones(voices: &[Voice; CHANNELS]) -> [Option<Tone>; CHANNELS] {
    voices.map(|voice| voice.tone)
}

fn gates(voices: &[Voice; CHANNELS]) -> [bool; CHANNELS] {
    voices.map(|voice| voice.gate)
}

#[test]
fn tone_sets_one_channel_with_default_volume_and_wave() {
    let mut voices = silent();
    assert_eq!(
        run_voice("tone 1 440", &mut voices),
        output(&["TONE 1 SQUARE 440 HZ VOLUME 8"])
    );
    assert_eq!(
        tones(&voices),
        [None, tone(Waveform::Square, 440, 8), None, None]
    );
}

#[test]
fn tone_takes_a_volume_and_wave() {
    let mut voices = silent();
    for (line, printed) in [
        ("TONE 0 440 8 SQUARE", "TONE 0 SQUARE 440 HZ VOLUME 8"),
        ("tone 1 660 6 triangle", "TONE 1 TRIANGLE 660 HZ VOLUME 6"),
        ("tone 2 220 5 Saw", "TONE 2 SAW 220 HZ VOLUME 5"),
        ("tone 3 1200 4 NOISE", "TONE 3 NOISE 1200 HZ VOLUME 4"),
    ] {
        assert_eq!(run_voice(line, &mut voices), output(&[printed]), "{line}");
    }
    assert_eq!(
        tones(&voices),
        [
            tone(Waveform::Square, 440, 8),
            tone(Waveform::Triangle, 660, 6),
            tone(Waveform::Saw, 220, 5),
            tone(Waveform::Noise, 1200, 4),
        ]
    );
}

#[test]
fn tone_leaves_the_gate_alone() {
    let mut voices = silent();
    run_voice("tone 0 440", &mut voices);
    assert_eq!(gates(&voices), [false; CHANNELS]);

    run_voice("gate 0 on", &mut voices);
    run_voice("tone 0 880 12 saw", &mut voices);
    assert_eq!(gates(&voices), [true, false, false, false]);
}

#[test]
fn tone_off_clears_the_tone_and_gate_of_one_channel_or_all() {
    let playing = [Voice {
        tone: tone(Waveform::Saw, 440, 8),
        adsr: Adsr {
            attack: 480,
            decay: 960,
            sustain: 8,
            release: 4800,
        },
        gate: true,
    }; CHANNELS];
    let off = Voice {
        tone: None,
        gate: false,
        ..playing[0]
    };

    let mut voices = playing;
    assert_eq!(
        run_voice("tone 2 OFF", &mut voices),
        output(&["TONE 2 OFF"])
    );
    assert_eq!(voices, [playing[0], playing[1], off, playing[3]]);

    let mut voices = playing;
    assert_eq!(run_voice("tone off", &mut voices), output(&["TONE OFF"]));
    assert_eq!(voices, [off; CHANNELS]);
}

#[test]
fn tone_rejects_bad_arguments_and_keeps_the_voices() {
    let mut before = silent();
    before[1].tone = tone(Waveform::Triangle, 440, 8);
    before[1].gate = true;
    let tone_usage = "Usage: tone <ch> <hz> [<vol> [<wave>]] | tone [<ch>] off";
    for (line, message) in [
        ("tone 440", error("Channel must be 0-3")),
        ("tone 4 440", error("Channel must be 0-3")),
        ("tone x 440", error("Channel must be 0-3")),
        ("tone -1 off", error("Channel must be 0-3")),
        ("tone 1", hinted("Missing <hz>", tone_usage)),
        ("tone 1 0", error("Frequency must be 1-24000 Hz")),
        ("tone 1 24001", error("Frequency must be 1-24000 Hz")),
        ("tone 1 loud", error("Frequency must be 1-24000 Hz")),
        ("tone 1 -5", error("Frequency must be 1-24000 Hz")),
        ("tone 1 440 16", error("Volume must be 0-15")),
        ("tone 1 440 x", error("Volume must be 0-15")),
        (
            "tone 1 440 8 sine",
            error("Wave must be square, triangle, saw or noise"),
        ),
        (
            "tone off 3",
            hinted("The channel goes before off", "Try: tone 3 off"),
        ),
        (
            "tone off 3 4",
            hinted("Too many arguments", "Usage: tone [<ch>] off"),
        ),
        (
            "tone 1 off 3",
            hinted("Too many arguments", "Usage: tone [<ch>] off"),
        ),
        ("tone", hinted("Missing <ch>", tone_usage)),
        (
            "tone 1 440 8 saw x",
            hinted("Too many arguments", tone_usage),
        ),
    ] {
        let mut voices = before;
        assert_eq!(run_voice(line, &mut voices), message, "{line}");
        assert_eq!(voices, before, "{line}");
    }
}

#[test]
fn tone_accepts_the_range_limits() {
    let mut voices = silent();
    run_voice("tone 0 1 0", &mut voices);
    run_voice("tone 3 24000 15", &mut voices);
    assert_eq!(
        tones(&voices),
        [
            tone(Waveform::Square, 1, 0),
            None,
            None,
            tone(Waveform::Square, 24000, 15),
        ]
    );
}

#[test]
fn env_sets_one_channel_in_samples() {
    let mut voices = silent();
    assert_eq!(
        run_voice("env 2 10 80 8 200", &mut voices),
        output(&["ENV 2 A 10 MS D 80 MS S 8 R 200 MS"])
    );
    assert_eq!(
        voices[2].adsr,
        Adsr {
            attack: 480,
            decay: 3840,
            sustain: 8,
            release: 9600,
        }
    );
    for index in [0, 1, 3] {
        assert_eq!(voices[index], Voice::default());
    }
}

#[test]
fn env_accepts_the_range_limits() {
    let mut voices = silent();
    run_voice("ENV 0 0 0 0 0", &mut voices);
    run_voice("ENV 3 10000 10000 15 10000", &mut voices);
    assert_eq!(
        voices[0].adsr,
        Adsr {
            attack: 0,
            decay: 0,
            sustain: 0,
            release: 0,
        }
    );
    assert_eq!(
        voices[3].adsr,
        Adsr {
            attack: MAX_ENVELOPE_SAMPLES,
            decay: MAX_ENVELOPE_SAMPLES,
            sustain: MAX_SUSTAIN,
            release: MAX_ENVELOPE_SAMPLES,
        }
    );
}

#[test]
fn env_rejects_bad_arguments_and_keeps_the_voices() {
    let before = silent();
    let env_usage = "Usage: env <ch> <a> <d> <s> <r>";
    for (line, message) in [
        ("env 4 10 80 8 200", error("Channel must be 0-3")),
        ("env x 10 80 8 200", error("Channel must be 0-3")),
        ("env 0 10001 80 8 200", error("Time must be 0-10000 ms")),
        ("env 0 10 -1 8 200", error("Time must be 0-10000 ms")),
        ("env 0 10 80 8 slow", error("Time must be 0-10000 ms")),
        ("env 0 10 80 16 200", error("Sustain must be 0-15")),
        ("env 0 10 80 x 200", error("Sustain must be 0-15")),
        ("env 0 10 80 8", hinted("Missing <r>", env_usage)),
        (
            "env 0 10 80 8 200 1",
            hinted("Too many arguments", env_usage),
        ),
    ] {
        let mut voices = before;
        assert_eq!(run_voice(line, &mut voices), message, "{line}");
        assert_eq!(voices, before, "{line}");
    }
}

#[test]
fn gate_opens_and_closes_one_channel() {
    let mut voices = silent();
    assert_eq!(run_voice("gate 1 on", &mut voices), output(&["GATE 1 ON"]));
    assert_eq!(run_voice("GATE 3 On", &mut voices), output(&["GATE 3 ON"]));
    assert_eq!(gates(&voices), [false, true, false, true]);
    assert_eq!(
        run_voice("gate 1 OFF", &mut voices),
        output(&["GATE 1 OFF"])
    );
    assert_eq!(gates(&voices), [false, false, false, true]);
}

#[test]
fn gate_rejects_bad_arguments_and_keeps_the_voices() {
    let before = silent();
    let gate_usage = "Usage: gate <ch> on|off";
    for (line, message) in [
        ("gate 4 on", error("Channel must be 0-3")),
        ("gate on 0", error("Channel must be 0-3")),
        (
            "gate 0 open",
            hinted("Gate must be on or off, not open", "Try: gate 0 on"),
        ),
        ("gate 0", hinted("Missing an argument", gate_usage)),
        ("gate 0 on now", hinted("Too many arguments", gate_usage)),
    ] {
        let mut voices = before;
        assert_eq!(run_voice(line, &mut voices), message, "{line}");
        assert_eq!(voices, before, "{line}");
    }
}

fn docs_request(line: &str) -> (Vec<Entry>, Option<ShellRequest>) {
    execute_line(
        line,
        Version::new(0, 3, 1),
        &mut no_cartridges(),
        CartridgeSlot::Empty,
    )
}

#[test]
fn docs_opens_the_front_page_or_the_page_it_names() {
    assert_eq!(
        docs_request("docs"),
        (Vec::new(), Some(ShellRequest::Docs(DocsStart::Home)))
    );
    let (entries, request) = docs_request("docs guide/getting-started");
    assert_eq!(entries, Vec::new());
    let Some(ShellRequest::Docs(DocsStart::Page(id))) = request else {
        panic!("expected a page, got {request:?}");
    };
    assert_eq!(id.as_str(), "guide/getting-started");
    let (_, request) = docs_request("docs spr");
    let Some(ShellRequest::Docs(DocsStart::Page(id))) = request else {
        panic!("expected a page, got {request:?}");
    };
    assert_eq!(id.as_str(), "api/graphics/spr");
}

#[test]
fn docs_on_an_unknown_topic_stays_in_the_shell() {
    assert_eq!(
        docs_request("docs zzzzqq"),
        (
            vec![
                Entry::Error("No documentation for zzzzqq".into()),
                Entry::Output("Try: docs".into()),
            ],
            None
        )
    );
}

#[test]
fn docs_takes_at_most_one_topic() {
    let (entries, request) = docs_request("docs spr gate");
    assert_eq!(request, None);
    assert_eq!(entries[0], Entry::Error("Too many arguments".into()));
}

#[test]
fn docs_waits_for_a_paused_cartridge() {
    let (fixture, pong) = pong("cmd-docs-paused");
    assert_eq!(
        execute_paused("docs", &fixture, &pong),
        (error("Cartridge paused: resume or stop it first"), None)
    );
}
