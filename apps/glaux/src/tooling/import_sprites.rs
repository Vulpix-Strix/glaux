//! `glaux import sprites` turns a PNG spritesheet into a cartridge's OBJ
//! tiles, tile attributes, object palettes and master-palette colors,
//! keeping every color exactly as drawn.
//!
//! One import replaces the cartridge's whole sprite set. See
//! [`import_publish`] for how it avoids overwriting
//! files it did not write.

use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use anyhow::{Context as _, bail};
use glaux_machine::palette::{Palette, Rgb};
use glaux_machine::system_rom::SystemRom;

use super::import_publish::{self, GroupBank, ImportRecord, PALETTE, Recovery, ensure_owned};
use crate::sprite_import::{FitSummary, SheetSource};

/// What to import and how.
#[derive(Debug, Clone, Default)]
pub(super) struct ImportOptions {
    pub(super) input: PathBuf,
    /// The cartridge directory, or the PNG's directory when `None`.
    pub(super) cartridge: Option<PathBuf>,
    /// The sprite-group sidecar; `<png stem>.sprites.ron` next to the PNG,
    /// if it exists, when `None`.
    pub(super) groups: Option<PathBuf>,
    /// An opaque color to treat as transparent.
    pub(super) transparent: Option<Rgb>,
    /// Report without touching any file.
    pub(super) dry_run: bool,
    /// Overwrite owned files this command did not write.
    pub(super) replace: bool,
}

/// What an import did, or would do in a dry run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ImportOutcome {
    pub(super) cartridge: PathBuf,
    pub(super) summary: FitSummary,
    pub(super) groups: Option<PathBuf>,
    /// The source of the import this one replaced.
    pub(super) previous: Option<String>,
    pub(super) recovery: Recovery,
    pub(super) written: bool,
}

/// Imports `options.input` into a cartridge, with `rom` supplying the
/// palettes the cartridge falls back to.
pub(super) fn import(options: &ImportOptions, rom: &SystemRom) -> anyhow::Result<ImportOutcome> {
    let input = &options.input;
    let cartridge = match &options.cartridge {
        Some(dir) => dir.clone(),
        None => input
            .parent()
            .map_or_else(|| PathBuf::from("."), Path::to_path_buf),
    };
    if !cartridge.join("cartridge.ron").is_file() {
        bail!(
            "{} is not a cartridge directory (it has no cartridge.ron); \
             pass the cartridge with --cartridge",
            cartridge.display()
        );
    }

    let recovery = if options.dry_run {
        Recovery::None
    } else {
        import_publish::recover(&cartridge)?
    };

    let source = SheetSource::read(input, options.groups.as_deref())?;
    let current = match fs::read(cartridge.join(PALETTE)) {
        Ok(bytes) => Palette::from_rgb888(&bytes)
            .with_context(|| format!("invalid {PALETTE} in {}", cartridge.display()))?,
        Err(err) if err.kind() == ErrorKind::NotFound => rom.palette().clone(),
        Err(err) => return Err(err).with_context(|| format!("failed to read {PALETTE}")),
    };
    let fitted = source
        .fit(
            options.transparent,
            &current,
            rom.palette(),
            rom.obj_palettes(),
        )
        .with_context(|| format!("cannot import {}", input.display()))?;

    let ownership = import_publish::ownership(&cartridge, rom.palette())?;
    ensure_owned(&ownership, options.replace)?;

    let outcome = ImportOutcome {
        cartridge: cartridge.clone(),
        summary: fitted.summary.clone(),
        groups: source.groups_path.clone(),
        previous: ownership.previous,
        recovery,
        written: !options.dry_run,
    };
    if options.dry_run {
        return Ok(outcome);
    }

    let outputs = [
        ("sprites.bin", fitted.tiles),
        ("objattr.bin", fitted.attributes),
        ("obp.bin", fitted.obj_palettes.as_bytes().to_vec()),
        (PALETTE, fitted.palette.to_rgb888().to_vec()),
    ];
    let record = ImportRecord::new(
        display_relative(input, &cartridge),
        source
            .groups_path
            .as_deref()
            .map(|path| display_relative(path, &cartridge)),
        fitted
            .summary
            .group_banks
            .iter()
            .map(|(name, bank)| GroupBank {
                name: name.clone(),
                bank: bank.get(),
            })
            .collect(),
        &outputs,
    );
    import_publish::publish(&cartridge, &outputs, &record.to_ron()?)?;
    Ok(outcome)
}

/// `path` relative to `base` with `/` separators when it lies inside it,
/// otherwise as given.
fn display_relative(path: &Path, base: &Path) -> String {
    let relative = fs::canonicalize(path)
        .ok()
        .zip(fs::canonicalize(base).ok())
        .and_then(|(path, base)| {
            path.strip_prefix(base).ok().map(|relative| {
                relative
                    .components()
                    .map(|part| part.as_os_str().to_string_lossy())
                    .collect::<Vec<_>>()
                    .join("/")
            })
        });
    relative.unwrap_or_else(|| path.display().to_string())
}

/// Parses `RRGGBB`, with an optional leading `#`.
pub(super) fn parse_hex_color(text: &str) -> Result<Rgb, String> {
    let digits = text.strip_prefix('#').unwrap_or(text);
    if digits.len() != 6 || !digits.chars().all(|ch| ch.is_ascii_hexdigit()) {
        return Err(format!("expected a color as RRGGBB, got {text:?}"));
    }
    u32::from_str_radix(digits, 16)
        .map(Rgb::from_hex)
        .map_err(|err| err.to_string())
}

#[cfg(test)]
mod tests;
