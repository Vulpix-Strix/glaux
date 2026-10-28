//! The type definitions of the Glaux API, handed to `luau-lsp` as a file.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

const FILE: &str = "glaux.d.luau";

/// Writes the definitions that ship inside this executable under `cache`
/// and returns the file. The folder is named after the Glaux version, so a
/// newer Glaux never reads definitions of an older one, and the file is
/// only rewritten when it differs.
pub(super) fn extract(cache: &Path) -> io::Result<PathBuf> {
    extract_text(cache, glaux_luau::DEFINITIONS)
}

fn extract_text(cache: &Path, text: &str) -> io::Result<PathBuf> {
    let dir = cache
        .join("lsp")
        .join(format!("definitions-{}", env!("CARGO_PKG_VERSION")));
    let file = dir.join(FILE);
    if fs::read_to_string(&file).is_ok_and(|existing| existing == text) {
        return Ok(file);
    }
    fs::create_dir_all(&dir)?;
    let partial = dir.join(format!("{FILE}.partial"));
    fs::write(&partial, text)?;
    fs::rename(&partial, &file)?;
    Ok(file)
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;
    use crate::scratch_dir::ScratchDir;

    #[test]
    fn the_shipped_definitions_are_extracted_once() {
        let scratch = ScratchDir::new("lsp-definitions");
        let file = extract_text(&scratch, "declare function a()\n").unwrap();
        assert_eq!(fs::read_to_string(&file).unwrap(), "declare function a()\n");
        let modified = fs::metadata(&file).unwrap().modified().unwrap();
        assert_eq!(
            extract_text(&scratch, "declare function a()\n").unwrap(),
            file
        );
        assert_eq!(fs::metadata(&file).unwrap().modified().unwrap(), modified);
    }

    #[test]
    fn different_definitions_replace_the_file_without_leftovers() {
        let scratch = ScratchDir::new("lsp-definitions-replaced");
        let file = extract_text(&scratch, "old").unwrap();
        assert_eq!(extract_text(&scratch, "new").unwrap(), file);
        assert_eq!(fs::read_to_string(&file).unwrap(), "new");
        let names: Vec<_> = fs::read_dir(file.parent().unwrap()).unwrap().collect();
        assert_eq!(names.len(), 1);
    }

    #[test]
    fn the_real_definitions_declare_the_api() {
        let scratch = ScratchDir::new("lsp-definitions-real");
        let file = extract(&scratch).unwrap();
        assert!(
            fs::read_to_string(file)
                .unwrap()
                .contains("declare function btn(")
        );
    }
}
