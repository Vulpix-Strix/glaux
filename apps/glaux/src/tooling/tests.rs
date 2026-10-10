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
