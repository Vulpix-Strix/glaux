//! Built-in shell commands.

use glaux_audio::MAX_ENVELOPE_MS;
use glaux_machine::framebuffer::{HEIGHT, WIDTH};
use glaux_machine::machine::{GENERATION, NAME, TICK_RATE};
use glaux_machine::palette::PALETTE_LEN;
use glaux_machine::psg::{
    Adsr, CHANNELS, MAX_FREQUENCY, MAX_SUSTAIN, MAX_VOLUME, SAMPLES_PER_MS, Waveform,
};

use super::super::{SystemInfo, identity_line};
use super::parser::{Invocation, OptionError, Split, quote, split_options};
use super::scrollback::{Entry, Scrollback};
use super::{CartridgeSlot, LoadOrigin, ShellRequest, Tone, Voice};
use crate::cartridge::{CartridgeManifest, LoadedCartridge};
use crate::cartridge_fs::{CartridgeStorage, EntryKind};
use crate::cartridge_template::{self, CreateError, NewCartridge};

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

    /// Reports `problem` followed by `usage` as the way to fix it.
    fn usage_error(&mut self, problem: impl Into<String>, usage: &str) {
        self.error(problem);
        self.output(format!("Usage: {usage}"));
    }

    /// Reports `problem` followed by `command` as the line that was most
    /// likely meant.
    fn suggest(&mut self, problem: impl Into<String>, command: impl std::fmt::Display) {
        self.error(problem);
        self.output(format!("Try: {command}"));
    }
}

type Handler = fn(&Invocation<'_>, &mut Context<'_>);

/// A built-in command and the metadata `help` and completion use.
#[derive(Debug)]
pub(super) struct Command {
    /// Lowercase name, matched case-insensitively and shown as is.
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
    /// Whether the one argument is a path, so extra words are most likely
    /// the rest of a path with spaces that was not quoted.
    takes_path: bool,
    /// Valueless `--flags` the command accepts anywhere among its
    /// arguments, by name without the leading `--`. They are taken out
    /// before arguments are counted.
    flags: &'static [&'static str],
}

/// Every built-in command, in the order `help` lists them.
pub(super) static COMMANDS: &[Command] = &[
    Command {
        name: "help",
        aliases: &["?"],
        usage: "help [<command>]",
        description: "Show available commands",
        min_args: 0,
        max_args: 1,
        handler: help,
        takes_path: false,
        flags: &[],
    },
    Command {
        name: "clear",
        aliases: &["cls"],
        usage: "clear",
        description: "Clear the console",
        min_args: 0,
        max_args: 0,
        handler: clear,
        takes_path: false,
        flags: &[],
    },
    Command {
        name: "info",
        aliases: &[],
        usage: "info",
        description: "Show system information",
        min_args: 0,
        max_args: 0,
        handler: info,
        takes_path: false,
        flags: &[],
    },
    Command {
        name: "version",
        aliases: &["ver"],
        usage: "version",
        description: "Show the System ROM version",
        min_args: 0,
        max_args: 0,
        handler: version,
        takes_path: false,
        flags: &[],
    },
    Command {
        name: "pwd",
        aliases: &[],
        usage: "pwd",
        description: "Show the current directory",
        min_args: 0,
        max_args: 0,
        handler: pwd,
        takes_path: false,
        flags: &[],
    },
    Command {
        name: "ls",
        aliases: &["dir"],
        usage: "ls [<path>]",
        description: "List a directory",
        min_args: 0,
        max_args: 1,
        handler: ls,
        takes_path: true,
        flags: &[],
    },
    Command {
        name: "cd",
        aliases: &[],
        usage: "cd [<path>]",
        description: "Change the current directory",
        min_args: 0,
        max_args: 1,
        handler: cd,
        takes_path: true,
        flags: &[],
    },
    Command {
        name: "new",
        aliases: &[],
        usage: NEW_USAGE,
        description: "Create a new cartridge",
        min_args: 1,
        // `new` explains extra words itself; see `explain_new_words`.
        max_args: usize::MAX,
        handler: new,
        takes_path: false,
        flags: DISCARD_FLAGS,
    },
    Command {
        name: "load",
        aliases: &[],
        usage: "load <path> [--discard]",
        description: "Load a cartridge",
        min_args: 1,
        max_args: 1,
        handler: load,
        takes_path: true,
        flags: DISCARD_FLAGS,
    },
    Command {
        name: "unload",
        aliases: &[],
        usage: "unload [--discard]",
        description: "Unload the loaded cartridge",
        min_args: 0,
        max_args: 0,
        handler: unload,
        takes_path: false,
        flags: DISCARD_FLAGS,
    },
    Command {
        name: "run",
        aliases: &[],
        usage: "run",
        description: "Run the loaded cartridge",
        min_args: 0,
        max_args: 0,
        handler: run,
        takes_path: false,
        flags: &[],
    },
    Command {
        name: "resume",
        aliases: &[],
        usage: "resume",
        description: "Resume the paused cartridge",
        min_args: 0,
        max_args: 0,
        handler: resume,
        takes_path: false,
        flags: &[],
    },
    Command {
        name: "stop",
        aliases: &[],
        usage: "stop",
        description: "Stop the paused cartridge",
        min_args: 0,
        max_args: 0,
        handler: stop,
        takes_path: false,
        flags: &[],
    },
    Command {
        name: "tone",
        aliases: &[],
        usage: TONE_USAGE,
        description: "Play a test tone",
        min_args: 1,
        max_args: 4,
        handler: tone,
        takes_path: false,
        flags: &[],
    },
    Command {
        name: "env",
        aliases: &[],
        usage: ENV_USAGE,
        description: "Set a channel envelope",
        min_args: 5,
        max_args: 5,
        handler: env,
        takes_path: false,
        flags: &[],
    },
    Command {
        name: "gate",
        aliases: &[],
        usage: GATE_USAGE,
        description: "Open or close a channel gate",
        min_args: 2,
        max_args: 2,
        handler: gate,
        takes_path: false,
        flags: &[],
    },
];

const NEW_USAGE: &str = "new <id> [--name <name>] [--version <semver>] [--author <author>] [--lang luau|rhai] [--discard]";

/// Flags of the commands that replace the loaded cartridge.
const DISCARD_FLAGS: &[&str] = &["discard"];

/// Options `new` accepts, by name without the leading `--`.
const NEW_OPTIONS: &[&str] = &["name", "version", "author", "lang"];

const TONE_USAGE: &str = "tone <ch> <hz> [<vol> [<wave>]] | tone [<ch>] off";

const TONE_OFF_USAGE: &str = "tone [<ch>] off";

/// A, D and R are milliseconds; S uses the PSG's 0-15 sustain scale.
const ENV_USAGE: &str = "env <ch> <a> <d> <s> <r>";

const GATE_USAGE: &str = "gate <ch> on|off";

/// Volume TONE uses when none is given.
const DEFAULT_TONE_VOLUME: u8 = 8;

/// Reported by commands that would replace or restart a paused cartridge.
const PAUSED_ERROR: &str = "Cartridge paused: resume or stop it first";

/// Reported by commands that act on a paused cartridge when there is none.
const NOT_PAUSED_ERROR: &str = "No paused cartridge";

/// Reported by commands that act on the loaded cartridge when there is none.
const NOT_LOADED_ERROR: &str = "No cartridge loaded";

/// Refuses, with the line that would proceed, when the command in
/// `invocation` would replace a loaded cartridge whose edits are unsaved
/// and `--discard` was not given. Returns whether it refused.
fn refuse_unsaved(invocation: &Invocation<'_>, ctx: &mut Context<'_>) -> bool {
    let CartridgeSlot::Loaded {
        cartridge,
        unsaved: true,
    } = ctx.slot
    else {
        return false;
    };
    if invocation.flags.contains(&"discard") {
        return false;
    }
    let identity = cartridge_identity(cartridge.manifest());
    let mut line = format!("{} --discard", invocation.name.to_ascii_lowercase());
    for arg in &invocation.args {
        line.push(' ');
        line.push_str(&quote(arg));
    }
    ctx.suggest(
        format!("{identity} has unsaved graphics changes; --discard drops them"),
        line,
    );
    true
}

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
    let flagged;
    let invocation = if command.flags.is_empty() {
        invocation
    } else {
        flagged = take_flags(invocation, command.flags);
        &flagged
    };
    let given = invocation.args.len();
    if given < command.min_args {
        ctx.usage_error(missing_argument(command.usage, given), command.usage);
        return;
    }
    if given > command.max_args {
        too_many_arguments(command, invocation, ctx);
        return;
    }
    (command.handler)(invocation, ctx);
}

/// `invocation` with the words naming one of `flags` taken out of its
/// arguments and recorded in [`Invocation::flags`].
fn take_flags<'a>(invocation: &Invocation<'a>, flags: &[&'static str]) -> Invocation<'a> {
    let mut taken = invocation.clone();
    taken.args.retain(|arg| {
        let flag = arg
            .strip_prefix("--")
            .and_then(|name| flags.iter().find(|&&flag| flag == name));
        if let Some(&flag) = flag
            && !taken.flags.contains(&flag)
        {
            taken.flags.push(flag);
        }
        flag.is_none()
    });
    taken
}

/// Names the first argument missing when `given` were given, from the
/// required `<placeholders>` of the first form in `usage`.
fn missing_argument(usage: &str, given: usize) -> String {
    let first_form = usage.split('|').next().unwrap_or(usage);
    first_form
        .split_whitespace()
        .skip(1)
        .filter(|token| token.starts_with('<'))
        .nth(given)
        .map_or_else(
            || "Missing an argument".into(),
            |name| format!("Missing {name}"),
        )
}

/// Explains why `invocation` has more arguments than `command` takes.
fn too_many_arguments(command: &Command, invocation: &Invocation<'_>, ctx: &mut Context<'_>) {
    if command.max_args == 0 {
        ctx.error(format!("{} doesn't take any arguments", command.name));
        return;
    }
    // Quotes already typed make the intended split unclear, so only an
    // unquoted path gets a corrected line.
    let raw = invocation.raw_args.trim_end();
    if command.takes_path && !raw.contains(['"', '\'']) {
        // Taken flags lose the spacing typed between the other words.
        let path = if invocation.flags.is_empty() {
            raw.to_owned()
        } else {
            invocation.args.join(" ")
        };
        let flags: String = invocation
            .flags
            .iter()
            .map(|flag| format!(" --{flag}"))
            .collect();
        ctx.suggest(
            "Paths with spaces need quotes",
            format!("{}{flags} {}", command.name, quote(&path)),
        );
        return;
    }
    ctx.usage_error("Too many arguments", command.usage);
}

fn unknown_command(name: &str) -> String {
    format!("Unknown command: {name}")
}

fn usage(usage: &str) -> String {
    format!("Usage: {usage}")
}

fn help(invocation: &Invocation<'_>, ctx: &mut Context<'_>) {
    if let Some(name) = invocation.args.first() {
        let Some(command) = find(name) else {
            ctx.error(unknown_command(name));
            return;
        };
        ctx.output(usage(command.usage));
        ctx.output(command.description);
        if !command.aliases.is_empty() {
            ctx.output(format!("Aliases: {}", command.aliases.join(" ")));
        }
        return;
    }

    let width = COMMANDS.iter().map(|c| c.name.len()).max().unwrap_or(0) + 2;
    for command in COMMANDS {
        ctx.output(format!("{:<width$}{}", command.name, command.description));
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
        CartridgeSlot::Loaded { cartridge, .. } => {
            ("LUAU", cartridge_identity(cartridge.manifest()), "LOADED")
        }
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
    format!("{} {}", manifest.cartridge.name, manifest.cartridge.version)
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

/// Creates a cartridge named `<id>` in the current directory and asks for
/// it to become the loaded cartridge, which the system announces. Nothing is
/// created, and the loaded cartridge is kept, on any error. Refused while a
/// cartridge is paused.
fn new(invocation: &Invocation<'_>, ctx: &mut Context<'_>) {
    if matches!(ctx.slot, CartridgeSlot::Paused(_)) {
        ctx.error(PAUSED_ERROR);
        return;
    }
    if refuse_unsaved(invocation, ctx) {
        return;
    }
    let split = match split_options(&invocation.args, NEW_OPTIONS) {
        Ok(split) => split,
        Err(err) => {
            ctx.error(err.to_string());
            if let OptionError::Unknown(_) = err {
                ctx.output("Options: --name, --version, --author, --lang, --discard");
            }
            return;
        }
    };
    let (id, after) = match split.positional[..] {
        [] => {
            let options = owned_options(&split);
            ctx.suggest("Missing <id>", new_line("my-game", &options));
            return;
        }
        [id] => (id.word, id.after),
        _ => {
            explain_new_words(&split, ctx);
            return;
        }
    };
    let option = |name: &str| {
        split
            .options
            .iter()
            .find_map(|&(option, value)| (option == name).then_some(value))
    };
    let created = NewCartridge::new(id, option("name"), option("version"), option("author"))
        .and_then(|spec| match option("lang") {
            Some(lang) => spec.with_language(lang),
            None => Ok(spec),
        })
        .and_then(|spec| cartridge_template::create(ctx.cartridges, &spec));
    match created {
        Ok(cartridge) => {
            *ctx.request = Some(ShellRequest::Load {
                cartridge: Box::new(cartridge),
                origin: LoadOrigin::Created,
            });
        }
        // `new --name My Game`: the only word is not an ID but likely the
        // rest of the value before it.
        Err(CreateError::InvalidId(_)) if after.is_some() => {
            let mut options = owned_options(&split);
            if let Some(index) = after {
                let value = &mut options[index].1;
                value.push(' ');
                value.push_str(id);
            }
            let suggested = options
                .iter()
                .find(|(name, _)| *name == "name")
                .and_then(|(_, name)| cartridge_template::suggest_id(name))
                .unwrap_or_else(|| "my-game".into());
            ctx.suggest(
                "Values with spaces need quotes",
                new_line(&suggested, &options),
            );
        }
        Err(err @ CreateError::InvalidId(_)) => {
            ctx.error(err.to_string());
            if let Some(suggested) = cartridge_template::suggest_id(id) {
                ctx.output(format!(
                    "Try: {}",
                    new_line(&suggested, &owned_options(&split))
                ));
            }
        }
        Err(err) => ctx.error(err.to_string()),
    }
}

/// `split`'s options with their values owned, so they can be edited.
fn owned_options(split: &Split<'_>) -> Vec<(&'static str, String)> {
    split
        .options
        .iter()
        .map(|&(name, value)| (name, value.to_owned()))
        .collect()
}

/// The `new` command line for `id` and `options`, quoted as needed.
fn new_line(id: &str, options: &[(&'static str, String)]) -> String {
    let mut line = format!("new {}", quote(id));
    for (name, value) in options {
        line.push_str(&format!(" --{name} {}", quote(value)));
    }
    line
}

/// Explains why `new` got more than one positional word, suggesting the
/// line that was most likely meant when that is clear.
fn explain_new_words(split: &Split<'_>, ctx: &mut Context<'_>) {
    let words = &split.positional;
    // `new my-game --name My New Game`: the ID comes first and every other
    // word continues the value of the option before it.
    if words[0].after.is_none() && words[1..].iter().all(|word| word.after.is_some()) {
        let mut options = owned_options(split);
        for word in &words[1..] {
            if let Some(index) = word.after {
                let value = &mut options[index].1;
                value.push(' ');
                value.push_str(word.word);
            }
        }
        ctx.suggest(
            "Values with spaces need quotes",
            new_line(words[0].word, &options),
        );
        return;
    }
    // `new My Game`: a display name typed where the ID goes.
    if split.options.is_empty() {
        let typed = words
            .iter()
            .map(|word| word.word)
            .collect::<Vec<_>>()
            .join(" ");
        if let Some(id) = cartridge_template::suggest_id(&typed) {
            let mut options = Vec::new();
            if typed != cartridge_template::display_name(&id) {
                options.push(("name", typed));
            }
            ctx.suggest(
                "The id must be one word, without spaces",
                new_line(&id, &options),
            );
            return;
        }
    }
    ctx.usage_error(
        "Too many arguments; values with spaces need quotes",
        NEW_USAGE,
    );
}

/// Validates the cartridge in the directory at `<path>` and asks for it to
/// become the loaded cartridge, which the system announces. A failed load
/// keeps the cartridge loaded before it. Refused without looking at
/// `<path>` while a cartridge is paused.
fn load(invocation: &Invocation<'_>, ctx: &mut Context<'_>) {
    if matches!(ctx.slot, CartridgeSlot::Paused(_)) {
        ctx.error(PAUSED_ERROR);
        return;
    }
    if refuse_unsaved(invocation, ctx) {
        return;
    }
    let path = &invocation.args[0];
    match LoadedCartridge::load(ctx.cartridges, path) {
        Ok(cartridge) => {
            *ctx.request = Some(ShellRequest::Load {
                cartridge: Box::new(cartridge),
                origin: LoadOrigin::Existing,
            });
        }
        Err(err) => ctx.error(err.to_string()),
    }
}

/// Asks the system to remove the normally loaded cartridge. Refused while
/// its edits are unsaved, unless `--discard` is given.
fn unload(invocation: &Invocation<'_>, ctx: &mut Context<'_>) {
    match ctx.slot {
        CartridgeSlot::Empty => ctx.error(NOT_LOADED_ERROR),
        CartridgeSlot::Loaded { .. } => {
            if !refuse_unsaved(invocation, ctx) {
                *ctx.request = Some(ShellRequest::Unload);
            }
        }
        CartridgeSlot::Paused(_) => ctx.error(PAUSED_ERROR),
    }
}

/// Asks the system to start the loaded cartridge. A paused cartridge is
/// never restarted.
fn run(_: &Invocation<'_>, ctx: &mut Context<'_>) {
    match ctx.slot {
        CartridgeSlot::Empty => ctx.error(NOT_LOADED_ERROR),
        CartridgeSlot::Loaded { .. } => *ctx.request = Some(ShellRequest::Run),
        CartridgeSlot::Paused(_) => ctx.error(PAUSED_ERROR),
    }
}

/// Asks the system to continue the paused cartridge.
fn resume(_: &Invocation<'_>, ctx: &mut Context<'_>) {
    match ctx.slot {
        CartridgeSlot::Paused(_) => *ctx.request = Some(ShellRequest::Resume),
        CartridgeSlot::Empty | CartridgeSlot::Loaded { .. } => ctx.error(NOT_PAUSED_ERROR),
    }
}

/// Asks the system to dispose of the paused cartridge's VM.
fn stop(_: &Invocation<'_>, ctx: &mut Context<'_>) {
    match ctx.slot {
        CartridgeSlot::Paused(_) => *ctx.request = Some(ShellRequest::Stop),
        CartridgeSlot::Empty | CartridgeSlot::Loaded { .. } => ctx.error(NOT_PAUSED_ERROR),
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
        ctx.error(format!("Channel must be 0-{}", CHANNELS - 1));
    }
    channel
}

/// Sets or clears the diagnostic oscillators the system plays on the PSG
/// channels while the shell is shown. Setting a tone leaves the gate alone;
/// clearing one also closes its gate.
fn tone(invocation: &Invocation<'_>, ctx: &mut Context<'_>) {
    let args = &invocation.args;
    if args[0].eq_ignore_ascii_case("off") {
        match &args[1..] {
            [] => {}
            [channel] => {
                ctx.suggest("The channel goes before off", format!("tone {channel} off"));
                return;
            }
            _ => {
                ctx.usage_error("Too many arguments", TONE_OFF_USAGE);
                return;
            }
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
        ctx.usage_error("Missing <hz>", TONE_USAGE);
        return;
    };
    if arg.eq_ignore_ascii_case("off") {
        if args.len() > 2 {
            ctx.usage_error("Too many arguments", TONE_OFF_USAGE);
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
        ctx.error(format!("Frequency must be 1-{MAX_FREQUENCY} Hz"));
        return;
    };
    let volume = match args.get(2) {
        None => DEFAULT_TONE_VOLUME,
        Some(arg) => match arg.parse::<u8>().ok().filter(|v| *v <= MAX_VOLUME) {
            Some(volume) => volume,
            None => {
                ctx.error(format!("Volume must be 0-{MAX_VOLUME}"));
                return;
            }
        },
    };
    let waveform = match args.get(3) {
        None => Waveform::Square,
        Some(arg) => match parse_waveform(arg) {
            Some(waveform) => waveform,
            None => {
                ctx.error("Wave must be square, triangle, saw or noise");
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
        ctx.error(format!("Time must be 0-{MAX_ENVELOPE_MS} ms"));
        return;
    };
    let Some(sustain) = args[3].parse::<u8>().ok().filter(|s| *s <= MAX_SUSTAIN) else {
        ctx.error(format!("Sustain must be 0-{MAX_SUSTAIN}"));
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
        ctx.suggest(
            format!("Gate must be on or off, not {}", args[1]),
            format!("gate {channel} on"),
        );
        return;
    };
    ctx.voices[channel].gate = open;
    ctx.output(format!(
        "GATE {channel} {}",
        if open { "ON" } else { "OFF" }
    ));
}

#[cfg(test)]
mod tests;
