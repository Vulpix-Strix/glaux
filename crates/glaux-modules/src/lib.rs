//! Cartridge code modules, independent of any scripting runtime.
//!
//! A script names a module with a logical [`ModuleName`]; the runtime turns
//! it into a cartridge-relative [`ModulePath`] below [`MODULE_ROOT`] and asks
//! a [`ModuleSource`] for its bytes. The source decides where those bytes
//! come from (a development directory, a packed cartridge, ...), so runtimes
//! never see host paths or filesystem APIs.
//!
//! # Module names
//!
//! ```text
//! segment = [a-z_][a-z0-9_]*
//! module  = segment ("." segment)*
//! ```
//!
//! Each segment is one directory or file name: `game.enemies` is
//! `code/game/enemies.<extension>`. The grammar admits no separators, no
//! `..`, no drive or stream markers and no uppercase, so every name resolves
//! below [`MODULE_ROOT`] and every module file has exactly one name, even on
//! case-insensitive hosts. A name whose last segment is a source extension,
//! such as `player.luau`, is rejected: names never carry an extension.

use std::fmt;

/// Directory, relative to the cartridge root, that every module lives under.
pub const MODULE_ROOT: &str = "code";

/// Final segments rejected because they read as a file extension.
const SOURCE_EXTENSIONS: [&str; 2] = ["lua", "luau"];

/// A module name that follows the grammar in the [crate docs](crate).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ModuleName(String);

/// Why a string is not a [`ModuleName`]. Messages are safe to show to the
/// cartridge author.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ModuleNameError {
    #[error("module name is empty")]
    Empty,
    #[error("module name has an empty segment")]
    EmptySegment,
    #[error("segment '{0}' starts with a digit")]
    LeadingDigit(String),
    #[error("{0:?} is not allowed; use lowercase letters, digits, '_' and '.'")]
    InvalidCharacter(char),
    #[error("module names take no extension; use \"{stem}\", not \"{stem}.{extension}\"")]
    HasExtension { stem: String, extension: String },
}

impl ModuleName {
    pub fn parse(name: &str) -> Result<Self, ModuleNameError> {
        if name.is_empty() {
            return Err(ModuleNameError::Empty);
        }
        // Checked before the characters so that `player.LUAU` gets this
        // explanation rather than a complaint about uppercase.
        if let Some((stem, last)) = name.rsplit_once('.') {
            let lowered = last.to_ascii_lowercase();
            if SOURCE_EXTENSIONS.contains(&lowered.as_str()) {
                return Err(ModuleNameError::HasExtension {
                    stem: stem.to_owned(),
                    extension: last.to_owned(),
                });
            }
        }
        for segment in name.split('.') {
            validate_segment(segment)?;
        }
        Ok(Self(name.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The module's file, `code/<segment>/.../<segment>.<extension>`.
    /// `extension` is chosen by the runtime, without a leading dot.
    pub fn path(&self, extension: &str) -> ModulePath {
        let mut full = format!("{MODULE_ROOT}/");
        full.push_str(&self.0.replace('.', "/"));
        full.push('.');
        full.push_str(extension);
        ModulePath(full)
    }
}

impl fmt::Display for ModuleName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

fn validate_segment(segment: &str) -> Result<(), ModuleNameError> {
    let mut chars = segment.chars();
    let Some(first) = chars.next() else {
        return Err(ModuleNameError::EmptySegment);
    };
    if first.is_ascii_digit() {
        return Err(ModuleNameError::LeadingDigit(segment.to_owned()));
    }
    for c in std::iter::once(first).chain(chars) {
        if !(c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_') {
            return Err(ModuleNameError::InvalidCharacter(c));
        }
    }
    Ok(())
}

/// The cartridge-relative, `/`-separated file of a module, always below
/// [`MODULE_ROOT`]. Only [`ModuleName::path`] creates one, so a
/// [`ModuleSource`] never receives an unvalidated path.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ModulePath(String);

impl ModulePath {
    /// The path from the cartridge root, as in `code/game/enemies.luau`.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The path below [`MODULE_ROOT`], as in `game/enemies.luau`, for
    /// sources rooted at that directory.
    pub fn in_root(&self) -> &str {
        &self.0[MODULE_ROOT.len() + 1..]
    }
}

impl fmt::Display for ModulePath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Why a [`ModuleSource`] could not supply a module. The caller knows the
/// path and reports it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ModuleLoadError {
    #[error("module not found")]
    NotFound,
    #[error("module cannot be read")]
    Unreadable,
}

/// Supplies module source bytes by cartridge-relative path.
///
/// Implementations must confine lookups to the cartridge's [`MODULE_ROOT`]
/// directory: a module file must not resolve, through links or otherwise, to
/// anything outside it.
pub trait ModuleSource {
    fn load(&self, path: &ModulePath) -> Result<Vec<u8>, ModuleLoadError>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    fn luau_path(name: &str) -> (String, String) {
        let path = ModuleName::parse(name).unwrap().path("luau");
        (path.as_str().to_owned(), path.in_root().to_owned())
    }

    #[test]
    fn names_map_to_files_below_the_module_root() {
        let pair = |a: &str, b: &str| (a.to_owned(), b.to_owned());
        assert_eq!(luau_path("player"), pair("code/player.luau", "player.luau"));
        assert_eq!(luau_path("_util"), pair("code/_util.luau", "_util.luau"));
        assert_eq!(
            luau_path("game.enemies"),
            pair("code/game/enemies.luau", "game/enemies.luau")
        );
        assert_eq!(luau_path("a1.b_2"), pair("code/a1/b_2.luau", "a1/b_2.luau"));
    }

    #[test]
    fn names_outside_the_grammar_are_rejected() {
        use ModuleNameError::*;
        let cases = [
            ("", Empty),
            ("a..b", EmptySegment),
            (".a", EmptySegment),
            ("a.", EmptySegment),
            ("../x", EmptySegment),
            ("/x", InvalidCharacter('/')),
            ("a/b", InvalidCharacter('/')),
            ("a\\b", InvalidCharacter('\\')),
            ("c:x", InvalidCharacter(':')),
            ("Player", InvalidCharacter('P')),
            ("a-b", InvalidCharacter('-')),
            ("a b", InvalidCharacter(' ')),
            ("\0", InvalidCharacter('\0')),
            ("búho", InvalidCharacter('ú')),
            ("1up", LeadingDigit("1up".into())),
            ("game.2d", LeadingDigit("2d".into())),
        ];
        for (name, expected) in cases {
            assert_eq!(ModuleName::parse(name), Err(expected), "{name:?}");
        }
    }

    #[test]
    fn extensions_are_rejected_with_the_extensionless_name() {
        for (name, stem, extension) in [
            ("player.luau", "player", "luau"),
            ("player.lua", "player", "lua"),
            ("game.enemies.LUAU", "game.enemies", "LUAU"),
        ] {
            let err = ModuleName::parse(name).unwrap_err();
            assert_eq!(
                err,
                ModuleNameError::HasExtension {
                    stem: stem.into(),
                    extension: extension.into(),
                }
            );
            assert!(
                err.to_string().contains(&format!("use \"{stem}\"")),
                "{err}"
            );
        }
        // Only a final segment reads as an extension.
        assert!(ModuleName::parse("lua.player").is_ok());
        assert!(ModuleName::parse("luau").is_ok());
    }
}
