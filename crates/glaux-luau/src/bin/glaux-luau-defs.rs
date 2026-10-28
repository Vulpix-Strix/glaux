//! Writes or checks `crates/glaux-luau/definitions/glaux.d.luau`.
//!
//! ```text
//! glaux-luau-defs generate
//! glaux-luau-defs check
//! ```
//!
//! Both are meant to be run through `mise run luau-defs` and
//! `mise run luau-defs-check`.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use glaux_docs::Library;
use glaux_luau::definitions;

const USAGE: &str = "usage: glaux-luau-defs <generate | check>";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    match args.as_slice() {
        ["generate"] => run(false),
        ["check"] => run(true),
        _ => {
            eprintln!("{USAGE}");
            ExitCode::from(2)
        }
    }
}

fn output_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(glaux_luau::DEFINITIONS_PATH)
}

fn run(check_only: bool) -> ExitCode {
    let library = Library::embedded();
    let text = match definitions(library.api()) {
        Ok(text) => text,
        Err(error) => {
            eprintln!("cannot generate the definitions: {error}");
            return ExitCode::FAILURE;
        }
    };
    let path = output_path();
    if fs::read_to_string(&path).is_ok_and(|existing| existing == text) {
        eprintln!("{} is up to date", glaux_luau::DEFINITIONS_PATH);
        return ExitCode::SUCCESS;
    }
    if check_only {
        eprintln!(
            "{} is out of date: run `mise run luau-defs`",
            glaux_luau::DEFINITIONS_PATH
        );
        return ExitCode::FAILURE;
    }
    if let Err(error) = fs::write(&path, &text) {
        eprintln!("cannot write {}: {error}", path.display());
        return ExitCode::FAILURE;
    }
    eprintln!("{} updated", glaux_luau::DEFINITIONS_PATH);
    ExitCode::SUCCESS
}
