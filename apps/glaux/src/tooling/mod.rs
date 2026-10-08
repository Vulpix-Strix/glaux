//! Non-desktop command-line tools.

mod import_publish;
mod import_sprites;
mod palette_fit;
mod sprites;

use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};
use glaux_machine::palette::Rgb;

use import_publish::Recovery;
use import_sprites::{ImportOptions, ImportOutcome, parse_hex_color};

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
    /// Import assets into a cartridge.
    Import(ImportArgs),
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

/// Commands in the cartridge-import namespace.
#[derive(Debug, Args)]
pub(crate) struct ImportArgs {
    #[command(subcommand)]
    command: ImportCommand,
}

/// A supported cartridge importer.
#[derive(Debug, Subcommand)]
enum ImportCommand {
    /// Import a PNG spritesheet into a cartridge, keeping its colors.
    ///
    /// Replaces the cartridge's sprites and sprite palettes: sprites.bin,
    /// objattr.bin, obp.bin and the imported colors of palette.bin. Files
    /// this command did not write are never overwritten without --replace.
    Sprites(ImportSpriteArgs),
}

/// Arguments for the sprite importer.
#[derive(Debug, Args)]
struct ImportSpriteArgs {
    /// PNG spritesheet to import. Transparent pixels must have alpha 0.
    input: PathBuf,
    /// Cartridge directory to import into [default: the PNG's directory].
    #[arg(long)]
    cartridge: Option<PathBuf>,
    /// Sprite-group file [default: <PNG name>.sprites.ron next to the PNG, if present].
    #[arg(long)]
    groups: Option<PathBuf>,
    /// Treat this opaque color, as RRGGBB, as transparent.
    #[arg(long, value_name = "RRGGBB", value_parser = parse_hex_color)]
    transparent: Option<Rgb>,
    /// Report what the import would do without writing anything.
    #[arg(long)]
    dry_run: bool,
    /// Overwrite sprite files this command did not write.
    #[arg(long)]
    replace: bool,
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
        Command::Import(ImportArgs {
            command: ImportCommand::Sprites(args),
        }) => {
            let rom = crate::rom::load()?;
            let outcome = import_sprites::import(
                &ImportOptions {
                    input: args.input.clone(),
                    cartridge: args.cartridge,
                    groups: args.groups,
                    transparent: args.transparent,
                    dry_run: args.dry_run,
                    replace: args.replace,
                },
                &rom,
            )?;
            print!("{}", import_report(&args.input, &outcome));
            Ok(())
        }
    }
}

/// The plain-language summary printed after an import.
fn import_report(input: &std::path::Path, outcome: &ImportOutcome) -> String {
    let summary = &outcome.summary;
    let mut report = String::new();
    match outcome.recovery {
        Recovery::None => {}
        Recovery::RolledForward => report.push_str("Finished an interrupted earlier import.\n"),
        Recovery::Discarded => report.push_str("Discarded an unfinished earlier import.\n"),
    }
    let verb = if outcome.written {
        "Imported"
    } else {
        "Dry run: would import"
    };
    let new_colors = match &summary.allocated {
        Some(range) => format!(
            "{} new at master {}-{}",
            range.clone().count(),
            range.start(),
            range.end()
        ),
        None => "none new".to_string(),
    };
    report.push_str(&format!(
        "{verb} {} into {}: {} tiles, {} colors ({} reused, {new_colors}), {} of {} sprite palettes.\n",
        input.display(),
        outcome.cartridge.display(),
        summary.source_tiles,
        summary.colors,
        summary.reused,
        summary.banks_used,
        glaux_machine::obj_palette::OBJ_PALETTE_BANKS,
    ));
    if outcome.groups.is_some() {
        for (name, bank) in &summary.group_banks {
            report.push_str(&format!("  {name} -> OBP{}\n", bank.get()));
        }
    } else {
        report.push_str("No sprite groups: palette overrides in spr() apply tile by tile.\n");
    }
    if let Some(previous) = &outcome.previous {
        report.push_str(&format!(
            "Replaced the sprites and sprite palettes of the previous import of {previous}.\n"
        ));
    }
    report
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

    #[test]
    fn parses_the_sprite_importer_flags() {
        let cli = Cli::try_parse_from([
            "glaux",
            "import",
            "sprites",
            "art/sheet.png",
            "--cartridge",
            "game",
            "--groups",
            "sheet.ron",
            "--transparent",
            "#ff00ff",
            "--dry-run",
            "--replace",
        ])
        .unwrap();
        let Some(Command::Import(ImportArgs {
            command: ImportCommand::Sprites(args),
        })) = cli.command
        else {
            panic!("expected the sprite importer")
        };
        assert_eq!(args.input, Path::new("art/sheet.png"));
        assert_eq!(args.cartridge.as_deref(), Some(Path::new("game")));
        assert_eq!(args.groups.as_deref(), Some(Path::new("sheet.ron")));
        assert_eq!(args.transparent, Some(Rgb::from_hex(0xff00ff)));
        assert!(args.dry_run && args.replace);

        assert_eq!(
            Cli::try_parse_from([
                "glaux",
                "import",
                "sprites",
                "a.png",
                "--transparent",
                "red"
            ])
            .unwrap_err()
            .kind(),
            ErrorKind::ValueValidation
        );
    }
}
