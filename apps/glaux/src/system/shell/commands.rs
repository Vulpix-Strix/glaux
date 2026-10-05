//! Built-in shell commands.

use glaux_machine::framebuffer::{HEIGHT, WIDTH};
use glaux_machine::machine::{GENERATION, NAME, TICK_RATE};
use glaux_machine::palette::PALETTE_LEN;

use super::super::{SystemInfo, identity_line};
use super::ShellRequest;
use super::parser::Invocation;
use super::scrollback::{Entry, Scrollback};
use crate::cartridge::{CartridgeManifest, LoadedCartridge};
use crate::cartridge_fs::{CartridgeStorage, EntryKind};

/// What a command can see and change while it runs.
pub(super) struct Context<'a> {
    pub(super) scrollback: &'a mut Scrollback,
    pub(super) info: &'a SystemInfo,
    pub(super) cartridges: &'a mut CartridgeStorage,
    pub(super) loaded: Option<&'a LoadedCartridge>,
    /// Set by a command that needs the system to change the cartridge
    /// lifecycle.
    pub(super) request: &'a mut Option<ShellRequest>,
}

impl Context<'_> {
    fn output(&mut self, line: impl Into<String>) {
        self.scrollback.push(Entry::Output(line.into()));
    }

    fn error(&mut self, line: impl Into<String>) {
        self.scrollback.push(Entry::Error(line.into()));
    }
}

type Handler = fn(&Invocation<'_>, &mut Context<'_>);

/// A built-in command and the metadata `help` and completion use.
#[derive(Debug)]
pub(super) struct Command {
    /// Lowercase name, matched case-insensitively and shown uppercase.
    pub(super) name: &'static str,
    /// Other names that run the command, matched like `name`.
    pub(super) aliases: &'static [&'static str],
    pub(super) usage: &'static str,
    pub(super) description: &'static str,
    /// Fewest positional arguments accepted; fewer is a usage error.
    min_args: usize,
    /// Most positional arguments accepted; more is a usage error.
    max_args: usize,
    handler: Handler,
}

/// Every built-in command, in the order `help` lists them.
pub(super) static COMMANDS: &[Command] = &[
    Command {
        name: "help",
        aliases: &["?"],
        usage: "HELP [COMMAND]",
        description: "SHOW AVAILABLE COMMANDS",
        min_args: 0,
        max_args: 1,
        handler: help,
    },
    Command {
        name: "clear",
        aliases: &["cls"],
        usage: "CLEAR",
        description: "CLEAR THE CONSOLE",
        min_args: 0,
        max_args: 0,
        handler: clear,
    },
    Command {
        name: "info",
        aliases: &[],
        usage: "INFO",
        description: "SHOW SYSTEM INFORMATION",
        min_args: 0,
        max_args: 0,
        handler: info,
    },
    Command {
        name: "version",
        aliases: &["ver"],
        usage: "VERSION",
        description: "SHOW THE SYSTEM ROM VERSION",
        min_args: 0,
        max_args: 0,
        handler: version,
    },
    Command {
        name: "pwd",
        aliases: &[],
        usage: "PWD",
        description: "SHOW THE CURRENT DIRECTORY",
        min_args: 0,
        max_args: 0,
        handler: pwd,
    },
    Command {
        name: "ls",
        aliases: &["dir"],
        usage: "LS [PATH]",
        description: "LIST A DIRECTORY",
        min_args: 0,
        max_args: 1,
        handler: ls,
    },
    Command {
        name: "cd",
        aliases: &[],
        usage: "CD [PATH]",
        description: "CHANGE THE CURRENT DIRECTORY",
        min_args: 0,
        max_args: 1,
        handler: cd,
    },
    Command {
        name: "load",
        aliases: &[],
        usage: "LOAD PATH",
        description: "LOAD A CARTRIDGE",
        min_args: 1,
        max_args: 1,
        handler: load,
    },
    Command {
        name: "run",
        aliases: &[],
        usage: "RUN",
        description: "RUN THE LOADED CARTRIDGE",
        min_args: 0,
        max_args: 0,
        handler: run,
    },
];

/// The command called `name` or one of its aliases, ignoring ASCII case.
pub(super) fn find(name: &str) -> Option<&'static Command> {
    COMMANDS.iter().find(|command| {
        command.name.eq_ignore_ascii_case(name)
            || command
                .aliases
                .iter()
                .any(|alias| alias.eq_ignore_ascii_case(name))
    })
}

/// Runs `invocation`, reporting unknown commands and usage errors in the
/// scrollback.
pub(super) fn execute(invocation: &Invocation<'_>, ctx: &mut Context<'_>) {
    let Some(command) = find(&invocation.name) else {
        ctx.error(unknown_command(&invocation.name));
        return;
    };
    if !(command.min_args..=command.max_args).contains(&invocation.args.len()) {
        ctx.error(format!("USAGE: {}", command.usage));
        return;
    }
    (command.handler)(invocation, ctx);
}

fn unknown_command(name: &str) -> String {
    format!("UNKNOWN COMMAND: {}", name.to_uppercase())
}

fn help(invocation: &Invocation<'_>, ctx: &mut Context<'_>) {
    if let Some(name) = invocation.args.first() {
        let Some(command) = find(name) else {
            ctx.error(unknown_command(name));
            return;
        };
        ctx.output(format!("USAGE: {}", command.usage));
        ctx.output(command.description);
        if !command.aliases.is_empty() {
            ctx.output(format!(
                "ALIASES: {}",
                command.aliases.join(" ").to_uppercase()
            ));
        }
        return;
    }

    let width = COMMANDS.iter().map(|c| c.name.len()).max().unwrap_or(0) + 2;
    for command in COMMANDS {
        let name = command.name.to_uppercase();
        ctx.output(format!("{name:<width$}{}", command.description));
    }
}

fn clear(_: &Invocation<'_>, ctx: &mut Context<'_>) {
    ctx.scrollback.clear();
}

/// The shell is only active while no cartridge runs, so it reports either
/// no cartridge or a loaded one. RUNTIME names the runtime the cartridge is
/// written for.
fn info(_: &Invocation<'_>, ctx: &mut Context<'_>) {
    let (runtime, cartridge, state) = match ctx.loaded {
        Some(loaded) => ("LUAU", cartridge_identity(loaded.manifest()), "LOADED"),
        None => ("NONE", "NONE".into(), "IDLE"),
    };
    let rows = [
        ("MACHINE", format!("{NAME} {GENERATION}")),
        ("SYSTEM ROM", ctx.info.rom_version.to_string()),
        (
            "DISPLAY",
            format!("{WIDTH}x{HEIGHT} INDEXED, {PALETTE_LEN} COLORS"),
        ),
        ("TICK RATE", format!("{TICK_RATE} HZ")),
        ("RUNTIME", runtime.into()),
        ("CARTRIDGE", cartridge),
        ("STATE", state.into()),
    ];
    let width = rows.iter().map(|(label, _)| label.len()).max().unwrap_or(0) + 2;
    for (label, value) in rows {
        ctx.output(format!("{label:<width$}{value}"));
    }
}

/// `Name version`, as a cartridge is named in shell output.
pub(in crate::system) fn cartridge_identity(manifest: &CartridgeManifest) -> String {
    format!("{} {}", manifest.name, manifest.version)
}

fn version(_: &Invocation<'_>, ctx: &mut Context<'_>) {
    let line = identity_line(&ctx.info.rom_version);
    ctx.output(line);
}

fn pwd(_: &Invocation<'_>, ctx: &mut Context<'_>) {
    match ctx.cartridges.cwd() {
        Ok(cwd) => ctx.output(cwd),
        Err(err) => ctx.error(err.to_string()),
    }
}

/// Lists one entry per line, marking directories with a trailing `/`.
fn ls(invocation: &Invocation<'_>, ctx: &mut Context<'_>) {
    match ctx
        .cartridges
        .list(invocation.args.first().map(String::as_str))
    {
        Ok(entries) => {
            for entry in entries {
                match entry.kind {
                    EntryKind::Directory => ctx.output(format!("{}/", entry.name)),
                    EntryKind::File => ctx.output(entry.name),
                }
            }
        }
        Err(err) => ctx.error(err.to_string()),
    }
}

/// Changes to `PATH`, or to `/` without one.
fn cd(invocation: &Invocation<'_>, ctx: &mut Context<'_>) {
    let path = invocation.args.first().map_or("/", String::as_str);
    if let Err(err) = ctx.cartridges.change_dir(path) {
        ctx.error(err.to_string());
    }
}

/// Validates the cartridge in the directory at `PATH` and asks for it to
/// become the loaded cartridge. A failed load keeps the cartridge loaded
/// before it.
fn load(invocation: &Invocation<'_>, ctx: &mut Context<'_>) {
    let path = &invocation.args[0];
    match LoadedCartridge::load(ctx.cartridges, path) {
        Ok(cartridge) => {
            let line = format!("LOADED {}", cartridge_identity(cartridge.manifest()));
            *ctx.request = Some(ShellRequest::Load(cartridge));
            ctx.output(line);
        }
        Err(err) => ctx.error(err.to_string()),
    }
}

/// Asks the system to start the loaded cartridge.
fn run(_: &Invocation<'_>, ctx: &mut Context<'_>) {
    if ctx.loaded.is_none() {
        ctx.error("NO CARTRIDGE LOADED");
        return;
    }
    *ctx.request = Some(ShellRequest::Run);
}

#[cfg(test)]
mod tests {
    use super::super::parser::parse;
    use super::*;
    use crate::cartridge::fixture::{write_cartridge, write_manifest};
    use crate::cartridge_fs::fixture::Fixture;
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
        loaded: Option<&LoadedCartridge>,
    ) -> (Vec<Entry>, Option<ShellRequest>) {
        let mut scrollback = Scrollback::default();
        let info = SystemInfo { rom_version };
        let mut request = None;
        let mut ctx = Context {
            scrollback: &mut scrollback,
            info: &info,
            cartridges,
            loaded,
            request: &mut request,
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
        let (entries, request) = execute_line(line, rom_version, cartridges, loaded.as_ref());
        if let Some(ShellRequest::Load(cartridge)) = request {
            *loaded = Some(cartridge);
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

    #[test]
    fn find_ignores_case() {
        assert_eq!(find("help").map(|c| c.name), Some("help"));
        assert_eq!(find("HeLp").map(|c| c.name), Some("help"));
        assert_eq!(find("nope").map(|c| c.name), None);
    }

    #[test]
    fn find_resolves_aliases() {
        assert_eq!(find("?").map(|c| c.name), Some("help"));
        assert_eq!(find("CLS").map(|c| c.name), Some("clear"));
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
                "HELP     SHOW AVAILABLE COMMANDS",
                "CLEAR    CLEAR THE CONSOLE",
                "INFO     SHOW SYSTEM INFORMATION",
                "VERSION  SHOW THE SYSTEM ROM VERSION",
                "PWD      SHOW THE CURRENT DIRECTORY",
                "LS       LIST A DIRECTORY",
                "CD       CHANGE THE CURRENT DIRECTORY",
                "LOAD     LOAD A CARTRIDGE",
                "RUN      RUN THE LOADED CARTRIDGE",
            ])
        );
    }

    #[test]
    fn help_describes_one_command() {
        assert_eq!(
            run("help CLEAR"),
            output(&["USAGE: CLEAR", "CLEAR THE CONSOLE", "ALIASES: CLS"])
        );
        assert_eq!(
            run("? info"),
            output(&["USAGE: INFO", "SHOW SYSTEM INFORMATION"])
        );
    }

    #[test]
    fn help_for_unknown_command_is_an_error() {
        assert_eq!(run("help nope"), error("UNKNOWN COMMAND: NOPE"));
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
            loaded: None,
            request: &mut None,
        };
        execute(&parse("clear").unwrap().unwrap(), &mut ctx);
        assert_eq!(scrollback.iter().len(), 0);
    }

    #[test]
    fn unknown_command_is_reported_uppercase() {
        assert_eq!(run("foo"), error("UNKNOWN COMMAND: FOO"));
        assert_eq!(run("\"Super Owl\" x"), error("UNKNOWN COMMAND: SUPER OWL"));
    }

    #[test]
    fn extra_arguments_are_usage_errors() {
        assert_eq!(run("clear now"), error("USAGE: CLEAR"));
        assert_eq!(run("info x"), error("USAGE: INFO"));
        assert_eq!(run("version x"), error("USAGE: VERSION"));
        assert_eq!(run("help a b"), error("USAGE: HELP [COMMAND]"));
        assert_eq!(run("pwd x"), error("USAGE: PWD"));
        assert_eq!(run("ls a b"), error("USAGE: LS [PATH]"));
        assert_eq!(run("cd a b"), error("USAGE: CD [PATH]"));
        assert_eq!(run("load a b"), error("USAGE: LOAD PATH"));
        assert_eq!(run("run x"), error("USAGE: RUN"));
    }

    #[test]
    fn missing_arguments_are_usage_errors() {
        assert_eq!(run("load"), error("USAGE: LOAD PATH"));
    }

    #[test]
    fn pwd_starts_at_root() {
        let fixture = Fixture::new("cmd-pwd");
        assert_eq!(run_fs("pwd", &mut fixture.storage()), output(&["/"]));
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
        ] {
            assert_eq!(run(line), error("CARTRIDGE STORAGE UNAVAILABLE"), "{line}");
        }
    }

    #[test]
    fn other_commands_work_without_storage() {
        assert_eq!(run("version"), output(&["GLAUX G1  SYSTEM ROM 0.3.1"]));
        assert_eq!(run("info").len(), 7);
        assert_eq!(run("help").len(), COMMANDS.len());
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
            error("NO SUCH FILE OR DIRECTORY: /nope")
        );
        assert_eq!(
            run_fs("ls b.txt", &mut fs),
            error("NOT A DIRECTORY: /b.txt")
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
            error("NO SUCH FILE OR DIRECTORY: /games/nope")
        );
        assert_eq!(
            run_fs("cd /b.txt", &mut fs),
            error("NOT A DIRECTORY: /b.txt")
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
        assert_eq!(
            run_load("load pong", &mut fs, &mut loaded),
            output(&["LOADED Pong 0.1.0"])
        );
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
        write_manifest(&fixture.root().join("games/owl"), "{");
        let mut fs = fixture.storage();
        let mut loaded = None;

        assert_eq!(
            run_load("load games/owl", &mut fs, &mut loaded),
            error("CART.JSON IS NOT VALID JSON (LINE 1)")
        );
        assert_eq!(loaded, None);

        run_load("load /games/pong", &mut fs, &mut loaded);
        let pong = loaded.clone();
        for line in ["load games/owl", "load nope", "load b.txt", "load tools"] {
            assert_eq!(run_load(line, &mut fs, &mut loaded).len(), 1, "{line}");
            assert_eq!(loaded, pong, "{line}");
        }
        assert!(
            run_load("info", &mut fs, &mut loaded)
                .contains(&Entry::Output("CARTRIDGE   Pong 0.1.0".into()))
        );
    }

    #[test]
    fn load_replaces_the_loaded_cartridge() {
        let fixture = Fixture::new("cmd-load-replace");
        write_cartridge(&fixture.root().join("pong"), "Pong");
        write_cartridge(&fixture.root().join("owl"), "Owl");
        let mut fs = fixture.storage();
        let mut loaded = None;
        run_load("load pong", &mut fs, &mut loaded);
        assert_eq!(
            run_load("load owl", &mut fs, &mut loaded),
            output(&["LOADED Owl 0.1.0"])
        );
        assert_eq!(
            loaded.map(|c| c.manifest().name.clone()),
            Some("Owl".into())
        );
    }

    #[test]
    fn run_needs_a_loaded_cartridge() {
        assert_eq!(run("run"), error("NO CARTRIDGE LOADED"));
        let fixture = Fixture::new("cmd-run-none");
        let (entries, request) =
            execute_line("RUN", Version::new(0, 3, 1), &mut fixture.storage(), None);
        assert_eq!((entries, request), (error("NO CARTRIDGE LOADED"), None));
    }

    #[test]
    fn run_requests_the_loaded_cartridge_to_start() {
        let fixture = Fixture::new("cmd-run-request");
        write_cartridge(&fixture.root().join("pong"), "Pong");
        let mut fs = fixture.storage();
        let mut loaded = None;
        run_load("load pong", &mut fs, &mut loaded);
        let (entries, request) =
            execute_line("run", Version::new(0, 3, 1), &mut fs, loaded.as_ref());
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
            None,
        );
        assert_eq!(entries, output(&["LOADED Pong 0.1.0"]));
        assert!(
            matches!(&request, Some(ShellRequest::Load(c)) if c.manifest().name == "Pong"),
            "{request:?}"
        );
    }
}
