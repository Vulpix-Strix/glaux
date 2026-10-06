//! Built-in shell commands.

use glaux_machine::framebuffer::{HEIGHT, WIDTH};
use glaux_machine::machine::{GENERATION, NAME, TICK_RATE};
use glaux_machine::palette::PALETTE_LEN;
use glaux_machine::psg::{
    Adsr, CHANNELS, MAX_ENVELOPE_SAMPLES, MAX_FREQUENCY, MAX_SUSTAIN, MAX_VOLUME, SAMPLE_RATE,
    Waveform,
};

use super::super::{SystemInfo, identity_line};
use super::parser::Invocation;
use super::scrollback::{Entry, Scrollback};
use super::{CartridgeSlot, ShellRequest, Tone, Voice};
use crate::cartridge::{CartridgeManifest, LoadedCartridge};
use crate::cartridge_fs::{CartridgeStorage, EntryKind};

/// What a command can see and change while it runs.
pub(super) struct Context<'a> {
    pub(super) scrollback: &'a mut Scrollback,
    pub(super) info: &'a SystemInfo,
    pub(super) cartridges: &'a mut CartridgeStorage,
    pub(super) slot: CartridgeSlot<'a>,
    /// Set by a command that needs the system to change the cartridge
    /// lifecycle.
    pub(super) request: &'a mut Option<ShellRequest>,
    /// The shell's diagnostic voice for each PSG channel.
    pub(super) voices: &'a mut [Voice; CHANNELS],
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
        name: "unload",
        aliases: &[],
        usage: "UNLOAD",
        description: "UNLOAD THE LOADED CARTRIDGE",
        min_args: 0,
        max_args: 0,
        handler: unload,
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
    Command {
        name: "resume",
        aliases: &[],
        usage: "RESUME",
        description: "RESUME THE PAUSED CARTRIDGE",
        min_args: 0,
        max_args: 0,
        handler: resume,
    },
    Command {
        name: "stop",
        aliases: &[],
        usage: "STOP",
        description: "STOP THE PAUSED CARTRIDGE",
        min_args: 0,
        max_args: 0,
        handler: stop,
    },
    Command {
        name: "tone",
        aliases: &[],
        usage: TONE_USAGE,
        description: "PLAY A TEST TONE",
        min_args: 1,
        max_args: 4,
        handler: tone,
    },
    Command {
        name: "env",
        aliases: &[],
        usage: ENV_USAGE,
        description: "SET A CHANNEL ENVELOPE",
        min_args: 5,
        max_args: 5,
        handler: env,
    },
    Command {
        name: "gate",
        aliases: &[],
        usage: GATE_USAGE,
        description: "OPEN OR CLOSE A CHANNEL GATE",
        min_args: 2,
        max_args: 2,
        handler: gate,
    },
];

const TONE_USAGE: &str = "TONE CH HZ [VOL [WAVE]] | TONE [CH] OFF";

/// A, D and R are milliseconds; S uses the PSG's 0-15 sustain scale.
const ENV_USAGE: &str = "ENV CH A D S R";

const GATE_USAGE: &str = "GATE CH ON|OFF";

/// PSG samples in one millisecond, which ENV times are given in.
const SAMPLES_PER_MS: u32 = SAMPLE_RATE / 1000;

const _: () = assert!(
    SAMPLE_RATE.is_multiple_of(1000),
    "ENV milliseconds must convert to whole samples"
);

/// Longest ENV time, in milliseconds.
const MAX_ENVELOPE_MS: u32 = MAX_ENVELOPE_SAMPLES / SAMPLES_PER_MS;

/// Volume TONE uses when none is given.
const DEFAULT_TONE_VOLUME: u8 = 8;

/// Reported by commands that would replace or restart a paused cartridge.
const PAUSED_ERROR: &str = "CARTRIDGE PAUSED: RESUME OR STOP IT FIRST";

/// Reported by commands that act on a paused cartridge when there is none.
const NOT_PAUSED_ERROR: &str = "NO PAUSED CARTRIDGE";

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

/// The shell is only active while no cartridge runs, so it reports no
/// cartridge, a loaded one or a paused one. RUNTIME names the runtime the
/// cartridge is written for.
fn info(_: &Invocation<'_>, ctx: &mut Context<'_>) {
    let (runtime, cartridge, state) = match ctx.slot {
        CartridgeSlot::Empty => ("NONE", "NONE".into(), "IDLE"),
        CartridgeSlot::Loaded(loaded) => ("LUAU", cartridge_identity(loaded.manifest()), "LOADED"),
        CartridgeSlot::Paused(paused) => ("LUAU", cartridge_identity(paused.manifest()), "PAUSED"),
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
/// before it. Refused without looking at `PATH` while a cartridge is paused.
fn load(invocation: &Invocation<'_>, ctx: &mut Context<'_>) {
    if matches!(ctx.slot, CartridgeSlot::Paused(_)) {
        ctx.error(PAUSED_ERROR);
        return;
    }
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

/// Asks the system to remove the normally loaded cartridge.
fn unload(_: &Invocation<'_>, ctx: &mut Context<'_>) {
    match ctx.slot {
        CartridgeSlot::Empty => ctx.error("NO CARTRIDGE LOADED"),
        CartridgeSlot::Loaded(_) => *ctx.request = Some(ShellRequest::Unload),
        CartridgeSlot::Paused(_) => ctx.error(PAUSED_ERROR),
    }
}

/// Asks the system to start the loaded cartridge. A paused cartridge is
/// never restarted.
fn run(_: &Invocation<'_>, ctx: &mut Context<'_>) {
    match ctx.slot {
        CartridgeSlot::Empty => ctx.error("NO CARTRIDGE LOADED"),
        CartridgeSlot::Loaded(_) => *ctx.request = Some(ShellRequest::Run),
        CartridgeSlot::Paused(_) => ctx.error(PAUSED_ERROR),
    }
}

/// Asks the system to continue the paused cartridge.
fn resume(_: &Invocation<'_>, ctx: &mut Context<'_>) {
    match ctx.slot {
        CartridgeSlot::Paused(_) => *ctx.request = Some(ShellRequest::Resume),
        CartridgeSlot::Empty | CartridgeSlot::Loaded(_) => ctx.error(NOT_PAUSED_ERROR),
    }
}

/// Asks the system to dispose of the paused cartridge's VM.
fn stop(_: &Invocation<'_>, ctx: &mut Context<'_>) {
    match ctx.slot {
        CartridgeSlot::Paused(_) => *ctx.request = Some(ShellRequest::Stop),
        CartridgeSlot::Empty | CartridgeSlot::Loaded(_) => ctx.error(NOT_PAUSED_ERROR),
    }
}

/// TONE's printed name for `waveform`.
fn waveform_name(waveform: Waveform) -> &'static str {
    match waveform {
        Waveform::Square => "SQUARE",
        Waveform::Triangle => "TRIANGLE",
        Waveform::Saw => "SAW",
        Waveform::Noise => "NOISE",
    }
}

/// Parses a TONE waveform name, ignoring case.
fn parse_waveform(name: &str) -> Option<Waveform> {
    match name.to_ascii_uppercase().as_str() {
        "SQUARE" => Some(Waveform::Square),
        "TRIANGLE" => Some(Waveform::Triangle),
        "SAW" => Some(Waveform::Saw),
        "NOISE" => Some(Waveform::Noise),
        _ => None,
    }
}

/// Parses a channel argument, reporting an out-of-range or malformed one.
fn parse_channel(arg: &str, ctx: &mut Context<'_>) -> Option<usize> {
    let channel = arg
        .parse::<usize>()
        .ok()
        .filter(|channel| *channel < CHANNELS);
    if channel.is_none() {
        ctx.error(format!("CHANNEL MUST BE 0-{}", CHANNELS - 1));
    }
    channel
}

/// Sets or clears the diagnostic oscillators the system plays on the PSG
/// channels while the shell is shown. Setting a tone leaves the gate alone;
/// clearing one also closes its gate.
fn tone(invocation: &Invocation<'_>, ctx: &mut Context<'_>) {
    let args = &invocation.args;
    if args[0].eq_ignore_ascii_case("off") {
        if args.len() > 1 {
            ctx.error("USAGE: TONE [CH] OFF");
            return;
        }
        for voice in ctx.voices.iter_mut() {
            voice.tone = None;
            voice.gate = false;
        }
        ctx.output("TONE OFF");
        return;
    }
    let Some(channel) = parse_channel(&args[0], ctx) else {
        return;
    };
    let Some(arg) = args.get(1) else {
        ctx.error(format!("USAGE: {TONE_USAGE}"));
        return;
    };
    if arg.eq_ignore_ascii_case("off") {
        if args.len() > 2 {
            ctx.error("USAGE: TONE [CH] OFF");
            return;
        }
        let voice = &mut ctx.voices[channel];
        voice.tone = None;
        voice.gate = false;
        ctx.output(format!("TONE {channel} OFF"));
        return;
    }
    let Some(frequency) = arg
        .parse::<u16>()
        .ok()
        .filter(|hz| (1..=MAX_FREQUENCY).contains(hz))
    else {
        ctx.error(format!("FREQUENCY MUST BE 1-{MAX_FREQUENCY} HZ"));
        return;
    };
    let volume = match args.get(2) {
        None => DEFAULT_TONE_VOLUME,
        Some(arg) => match arg.parse::<u8>().ok().filter(|v| *v <= MAX_VOLUME) {
            Some(volume) => volume,
            None => {
                ctx.error(format!("VOLUME MUST BE 0-{MAX_VOLUME}"));
                return;
            }
        },
    };
    let waveform = match args.get(3) {
        None => Waveform::Square,
        Some(arg) => match parse_waveform(arg) {
            Some(waveform) => waveform,
            None => {
                ctx.error("WAVE MUST BE SQUARE, TRIANGLE, SAW OR NOISE");
                return;
            }
        },
    };
    ctx.voices[channel].tone = Some(Tone {
        waveform,
        frequency,
        volume,
    });
    ctx.output(format!(
        "TONE {channel} {} {frequency} HZ VOLUME {volume}",
        waveform_name(waveform)
    ));
}

/// Sets a channel's envelope from attack, decay and release times in
/// milliseconds and a sustain level. It takes effect on the channel's
/// envelope at once, including a ramp in progress.
fn env(invocation: &Invocation<'_>, ctx: &mut Context<'_>) {
    let args = &invocation.args;
    let Some(channel) = parse_channel(&args[0], ctx) else {
        return;
    };
    let parse_ms = |arg: &String| arg.parse::<u32>().ok().filter(|ms| *ms <= MAX_ENVELOPE_MS);
    let (Some(attack), Some(decay), Some(release)) =
        (parse_ms(&args[1]), parse_ms(&args[2]), parse_ms(&args[4]))
    else {
        ctx.error(format!("TIME MUST BE 0-{MAX_ENVELOPE_MS} MS"));
        return;
    };
    let Some(sustain) = args[3].parse::<u8>().ok().filter(|s| *s <= MAX_SUSTAIN) else {
        ctx.error(format!("SUSTAIN MUST BE 0-{MAX_SUSTAIN}"));
        return;
    };
    ctx.voices[channel].adsr = Adsr {
        attack: attack * SAMPLES_PER_MS,
        decay: decay * SAMPLES_PER_MS,
        sustain,
        release: release * SAMPLES_PER_MS,
    };
    ctx.output(format!(
        "ENV {channel} A {attack} MS D {decay} MS S {sustain} R {release} MS"
    ));
}

/// Opens or closes a channel's gate, starting its attack or release.
fn gate(invocation: &Invocation<'_>, ctx: &mut Context<'_>) {
    let args = &invocation.args;
    let Some(channel) = parse_channel(&args[0], ctx) else {
        return;
    };
    let open = if args[1].eq_ignore_ascii_case("on") {
        true
    } else if args[1].eq_ignore_ascii_case("off") {
        false
    } else {
        ctx.error(format!("USAGE: {GATE_USAGE}"));
        return;
    };
    ctx.voices[channel].gate = open;
    ctx.output(format!(
        "GATE {channel} {}",
        if open { "ON" } else { "OFF" }
    ));
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
            CartridgeSlot::loaded(loaded.as_ref()),
        );
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
                "UNLOAD   UNLOAD THE LOADED CARTRIDGE",
                "RUN      RUN THE LOADED CARTRIDGE",
                "RESUME   RESUME THE PAUSED CARTRIDGE",
                "STOP     STOP THE PAUSED CARTRIDGE",
                "TONE     PLAY A TEST TONE",
                "ENV      SET A CHANNEL ENVELOPE",
                "GATE     OPEN OR CLOSE A CHANNEL GATE",
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
            slot: CartridgeSlot::Empty,
            request: &mut None,
            voices: &mut [Voice::default(); CHANNELS],
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
        assert_eq!(run("unload x"), error("USAGE: UNLOAD"));
        assert_eq!(run("run x"), error("USAGE: RUN"));
        assert_eq!(run("resume x"), error("USAGE: RESUME"));
        assert_eq!(run("stop x"), error("USAGE: STOP"));
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
        let (entries, request) = execute_line(
            "RUN",
            Version::new(0, 3, 1),
            &mut fixture.storage(),
            CartridgeSlot::Empty,
        );
        assert_eq!((entries, request), (error("NO CARTRIDGE LOADED"), None));
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
            (error("NO CARTRIDGE LOADED"), None)
        );
        assert_eq!(
            execute_line(
                "UNLOAD",
                Version::new(0, 3, 1),
                &mut fixture.storage(),
                CartridgeSlot::Loaded(&pong),
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
            CartridgeSlot::loaded(loaded.as_ref()),
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
        assert_eq!(entries, output(&["LOADED Pong 0.1.0"]));
        assert!(
            matches!(&request, Some(ShellRequest::Load(c)) if c.manifest().name == "Pong"),
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
        for slot in [CartridgeSlot::Empty, CartridgeSlot::Loaded(&pong)] {
            for line in ["resume", "stop"] {
                let result =
                    execute_line(line, Version::new(0, 3, 1), &mut fixture.storage(), slot);
                assert_eq!(result, (error("NO PAUSED CARTRIDGE"), None), "{line}");
            }
        }
    }

    #[test]
    fn load_and_run_refuse_to_replace_a_paused_cartridge() {
        let (fixture, pong) = pong("cmd-paused-refuse");
        for line in ["run", "unload", "load pong", "load /does/not/exist"] {
            assert_eq!(
                execute_paused(line, &fixture, &pong),
                (error("CARTRIDGE PAUSED: RESUME OR STOP IT FIRST"), None),
                "{line}"
            );
        }
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
        for (line, message) in [
            ("tone 440", "CHANNEL MUST BE 0-3"),
            ("tone 4 440", "CHANNEL MUST BE 0-3"),
            ("tone x 440", "CHANNEL MUST BE 0-3"),
            ("tone -1 off", "CHANNEL MUST BE 0-3"),
            ("tone 1", "USAGE: TONE CH HZ [VOL [WAVE]] | TONE [CH] OFF"),
            ("tone 1 0", "FREQUENCY MUST BE 1-24000 HZ"),
            ("tone 1 24001", "FREQUENCY MUST BE 1-24000 HZ"),
            ("tone 1 loud", "FREQUENCY MUST BE 1-24000 HZ"),
            ("tone 1 -5", "FREQUENCY MUST BE 1-24000 HZ"),
            ("tone 1 440 16", "VOLUME MUST BE 0-15"),
            ("tone 1 440 x", "VOLUME MUST BE 0-15"),
            (
                "tone 1 440 8 sine",
                "WAVE MUST BE SQUARE, TRIANGLE, SAW OR NOISE",
            ),
            ("tone off 3", "USAGE: TONE [CH] OFF"),
            ("tone 1 off 3", "USAGE: TONE [CH] OFF"),
            ("tone", "USAGE: TONE CH HZ [VOL [WAVE]] | TONE [CH] OFF"),
            (
                "tone 1 440 8 saw x",
                "USAGE: TONE CH HZ [VOL [WAVE]] | TONE [CH] OFF",
            ),
        ] {
            let mut voices = before;
            assert_eq!(run_voice(line, &mut voices), error(message), "{line}");
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
        for (line, message) in [
            ("env 4 10 80 8 200", "CHANNEL MUST BE 0-3"),
            ("env x 10 80 8 200", "CHANNEL MUST BE 0-3"),
            ("env 0 10001 80 8 200", "TIME MUST BE 0-10000 MS"),
            ("env 0 10 -1 8 200", "TIME MUST BE 0-10000 MS"),
            ("env 0 10 80 8 slow", "TIME MUST BE 0-10000 MS"),
            ("env 0 10 80 16 200", "SUSTAIN MUST BE 0-15"),
            ("env 0 10 80 x 200", "SUSTAIN MUST BE 0-15"),
            ("env 0 10 80 8", "USAGE: ENV CH A D S R"),
            ("env 0 10 80 8 200 1", "USAGE: ENV CH A D S R"),
        ] {
            let mut voices = before;
            assert_eq!(run_voice(line, &mut voices), error(message), "{line}");
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
        for (line, message) in [
            ("gate 4 on", "CHANNEL MUST BE 0-3"),
            ("gate on 0", "CHANNEL MUST BE 0-3"),
            ("gate 0 open", "USAGE: GATE CH ON|OFF"),
            ("gate 0", "USAGE: GATE CH ON|OFF"),
            ("gate 0 on now", "USAGE: GATE CH ON|OFF"),
        ] {
            let mut voices = before;
            assert_eq!(run_voice(line, &mut voices), error(message), "{line}");
            assert_eq!(voices, before, "{line}");
        }
    }
}
