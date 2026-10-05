//! Non-desktop command-line tools.

mod sprites;

use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};

/// Glaux desktop application and development tools.
#[derive(Debug, Parser)]
#[command(version, about)]
pub(crate) struct Cli {
    /// A standalone development tool. Without one, Glaux opens the desktop.
    #[command(subcommand)]
    pub(crate) command: Option<Command>,
}

impl Cli {
    /// Parses this process's command line with clap's standard diagnostics.
    pub(crate) fn parse_env() -> Self {
        Self::parse()
    }
}

/// A standalone Glaux command.
#[derive(Debug, Subcommand)]
pub(crate) enum Command {
    /// Import assets into native G1 formats.
    Import(ImportArgs),
}

/// Commands in the asset-import namespace.
#[derive(Debug, Args)]
pub(crate) struct ImportArgs {
    #[command(subcommand)]
    command: ImportCommand,
}

/// A supported asset importer.
#[derive(Debug, Subcommand)]
enum ImportCommand {
    /// Import an indexed PNG spritesheet as G1 4 bpp object tiles.
    Sprites(SpriteArgs),
}

/// Arguments for the sprite importer.
#[derive(Debug, Args)]
struct SpriteArgs {
    /// Indexed PNG spritesheet to import.
    input: PathBuf,
    /// Destination for the raw tile data.
    #[arg(short, long)]
    output: Option<PathBuf>,
}

/// Executes a parsed standalone command.
pub(crate) fn run(command: Command) -> anyhow::Result<()> {
    match command {
        Command::Import(ImportArgs {
            command: ImportCommand::Sprites(args),
        }) => {
            let summary = sprites::import(&args.input, args.output.as_deref())?;
            println!(
                "Wrote {}: {} tiles, {} bytes, 4 bpp",
                summary.output.display(),
                summary.tile_count,
                summary.byte_count
            );
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use clap::{CommandFactory, Parser, error::ErrorKind};

    use super::*;

    #[test]
    fn bare_glaux_selects_the_desktop() {
        assert!(Cli::try_parse_from(["glaux"]).unwrap().command.is_none());
    }

    #[test]
    fn parses_the_nested_sprite_command_and_both_output_flags() {
        for flag in ["-o", "--output"] {
            let cli =
                Cli::try_parse_from(["glaux", "import", "sprites", "sheet.png", flag, "tiles.bin"])
                    .unwrap();
            let Some(Command::Import(ImportArgs {
                command: ImportCommand::Sprites(args),
            })) = cli.command
            else {
                panic!("expected the sprite importer")
            };
            assert_eq!(args.input, Path::new("sheet.png"));
            assert_eq!(args.output.as_deref(), Some(Path::new("tiles.bin")));
        }
    }

    #[test]
    fn clap_handles_help_unknown_commands_and_missing_arguments() {
        assert_eq!(
            Cli::try_parse_from(["glaux", "--help"]).unwrap_err().kind(),
            ErrorKind::DisplayHelp
        );
        assert_eq!(
            Cli::try_parse_from(["glaux", "import", "--help"])
                .unwrap_err()
                .kind(),
            ErrorKind::DisplayHelp
        );
        assert_eq!(
            Cli::try_parse_from(["glaux", "import", "map"])
                .unwrap_err()
                .kind(),
            ErrorKind::InvalidSubcommand
        );
        assert_eq!(
            Cli::try_parse_from(["glaux", "import", "sprites"])
                .unwrap_err()
                .kind(),
            ErrorKind::MissingRequiredArgument
        );

        Cli::command().debug_assert();
    }
}
