//! Built-in shell commands.

use glaux_machine::framebuffer::{HEIGHT, WIDTH};
use semver::Version;

use super::parser::Invocation;
use super::scrollback::{Entry, Scrollback};

/// What a command can see and change while it runs.
pub(super) struct Context<'a> {
    pub(super) scrollback: &'a mut Scrollback,
    pub(super) rom_version: &'a Version,
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
        max_args: 1,
        handler: help,
    },
    Command {
        name: "clear",
        aliases: &["cls"],
        usage: "CLEAR",
        description: "CLEAR THE CONSOLE",
        max_args: 0,
        handler: clear,
    },
    Command {
        name: "info",
        aliases: &[],
        usage: "INFO",
        description: "SHOW SYSTEM INFORMATION",
        max_args: 0,
        handler: info,
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
    if invocation.args.len() > command.max_args {
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

fn info(_: &Invocation<'_>, ctx: &mut Context<'_>) {
    ctx.output("GLAUX G1");
    ctx.output(format!("SYSTEM ROM {}", ctx.rom_version));
    ctx.output(format!("DISPLAY {WIDTH}x{HEIGHT} INDEXED"));
}

#[cfg(test)]
mod tests {
    use super::super::parser::parse;
    use super::*;
    use pretty_assertions::assert_eq;

    /// Runs `line` on an empty scrollback and returns what it printed.
    fn run(line: &str) -> Vec<Entry> {
        let mut scrollback = Scrollback::default();
        let version = Version::new(0, 3, 1);
        let mut ctx = Context {
            scrollback: &mut scrollback,
            rom_version: &version,
        };
        execute(&parse(line).unwrap().unwrap(), &mut ctx);
        scrollback.iter().cloned().collect()
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
                "HELP   SHOW AVAILABLE COMMANDS",
                "CLEAR  CLEAR THE CONSOLE",
                "INFO   SHOW SYSTEM INFORMATION",
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
    fn info_reports_known_system_facts() {
        assert_eq!(
            run("info"),
            output(&["GLAUX G1", "SYSTEM ROM 0.3.1", "DISPLAY 320x240 INDEXED"])
        );
    }

    #[test]
    fn clear_empties_the_scrollback() {
        let mut scrollback = Scrollback::default();
        scrollback.push(Entry::Output("old".into()));
        let version = Version::new(0, 1, 0);
        let mut ctx = Context {
            scrollback: &mut scrollback,
            rom_version: &version,
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
        assert_eq!(run("help a b"), error("USAGE: HELP [COMMAND]"));
    }
}
