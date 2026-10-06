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
    /// Convert assets into native G1 formats.
    Convert(ConvertArgs),
}

/// Commands in the asset-conversion namespace.
#[derive(Debug, Args)]
pub(crate) struct ConvertArgs {
    #[command(subcommand)]
    command: ConvertCommand,
}

/// A supported asset converter.
#[derive(Debug, Subcommand)]
enum ConvertCommand {
    /// Convert an indexed PNG spritesheet into G1 4 bpp object tiles.
    Sprites(SpriteArgs),
}

/// Arguments for the sprite converter.
#[derive(Debug, Args)]
struct SpriteArgs {
    /// Indexed PNG spritesheet to convert.
    input: PathBuf,
    /// Destination for the raw tile data.
    #[arg(short, long)]
    output: Option<PathBuf>,
}

/// Executes a parsed standalone command.
pub(crate) fn run(command: Command) -> anyhow::Result<()> {
    match command {
        Command::Convert(ConvertArgs {
            command: ConvertCommand::Sprites(args),
        }) => {
            let summary = sprites::convert(&args.input, args.output.as_deref())?;
            println!(
                "Converted {} tiles into {} OBJ slots ({} bytes, 4 bpp): {}",
                summary.source_tiles,
                summary.obj_slots,
                summary.byte_count,
                summary.output.display()
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
            let cli = Cli::try_parse_from([
                "glaux",
                "convert",
                "sprites",
                "sheet.png",
                flag,
                "tiles.bin",
            ])
            .unwrap();
            let Some(Command::Convert(ConvertArgs {
                command: ConvertCommand::Sprites(args),
            })) = cli.command
            else {
                panic!("expected the sprite converter")
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
            Cli::try_parse_from(["glaux", "convert", "--help"])
                .unwrap_err()
                .kind(),
            ErrorKind::DisplayHelp
        );
        assert_eq!(
            Cli::try_parse_from(["glaux", "convert", "map"])
                .unwrap_err()
                .kind(),
            ErrorKind::InvalidSubcommand
        );
        assert_eq!(
            Cli::try_parse_from(["glaux", "convert", "sprites"])
                .unwrap_err()
                .kind(),
            ErrorKind::MissingRequiredArgument
        );

        Cli::command().debug_assert();
    }
}
