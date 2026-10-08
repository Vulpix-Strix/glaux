//! Writes imported sprite assets into a cartridge directory.
//!
//! An import owns `sprites.bin`, `objattr.bin`, `obp.bin` and the importer
//! region of `palette.bin`. It records checksums of what it wrote in
//! `sprites.import.ron`, refuses to overwrite owned files it did not write,
//! and publishes through a staging directory: outputs are written and
//! synced there, a `COMMIT` marker is synced last, and only then are the
//! files renamed into place. An import interrupted after `COMMIT` is
//! finished by the next one; one interrupted before it is discarded.
//!
//! Each rename replaces one file atomically, so every asset is always
//! wholly old or wholly new. The set as a whole is not: between the first
//! and last rename, or after a crash in that window until the next import,
//! the cartridge holds a mix of old and new assets.

use std::fs::{self, File};
use std::io::{ErrorKind, Write as _};
use std::path::Path;

use anyhow::{Context as _, bail};
use glaux_machine::palette::Palette;
use serde::{Deserialize, Serialize};

use super::palette_fit::CART_COLOR_START;

/// The import record, at the cartridge root.
pub(super) const RECORD: &str = "sprites.import.ron";
/// Staging directory for an import in progress, at the cartridge root.
pub(super) const STAGING: &str = ".glaux-import";
/// Marker that makes a staged import complete.
const COMMIT: &str = "COMMIT";
pub(super) const PALETTE: &str = "palette.bin";
/// Asset files an import owns, in publication order. The record follows
/// them.
pub(super) const OWNED: [&str; 4] = ["sprites.bin", "objattr.bin", "obp.bin", PALETTE];

/// Version of the record format.
const RECORD_FORMAT: u32 = 1;

/// What the last import wrote.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ImportRecord {
    pub(super) format: u32,
    /// The imported PNG, relative to the cartridge when it lies inside it.
    pub(super) source: String,
    /// The sprite-group sidecar used, if any, given like `source`.
    pub(super) groups: Option<String>,
    pub(super) group_banks: Vec<GroupBank>,
    pub(super) files: Vec<FileDigest>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct GroupBank {
    pub(super) name: String,
    pub(super) bank: u8,
}

/// Length and FNV-1a 64 checksum of one owned file. For `palette.bin` the
/// checksum covers only the importer region, since the entries below it
/// belong to the cartridge.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct FileDigest {
    pub(super) name: String,
    pub(super) length: usize,
    pub(super) fnv1a64: u64,
}

impl ImportRecord {
    pub(super) fn new(
        source: String,
        groups: Option<String>,
        group_banks: Vec<GroupBank>,
        outputs: &[(&str, Vec<u8>)],
    ) -> Self {
        Self {
            format: RECORD_FORMAT,
            source,
            groups,
            group_banks,
            files: outputs
                .iter()
                .map(|(name, bytes)| digest(name, bytes))
                .collect(),
        }
    }

    pub(super) fn to_ron(&self) -> anyhow::Result<String> {
        let mut text = ron::ser::to_string_pretty(self, ron::ser::PrettyConfig::default())
            .context("failed to encode the import record")?;
        text.push('\n');
        Ok(text)
    }
}

fn digest(name: &str, bytes: &[u8]) -> FileDigest {
    FileDigest {
        name: name.to_owned(),
        length: bytes.len(),
        fnv1a64: fnv1a64(owned_bytes(name, bytes)),
    }
}

/// The part of an owned file an import is responsible for.
fn owned_bytes<'a>(name: &str, bytes: &'a [u8]) -> &'a [u8] {
    if name == PALETTE {
        bytes.get(CART_COLOR_START * 3..).unwrap_or(&[])
    } else {
        bytes
    }
}

/// 64-bit FNV-1a. Detects edits to imported files; it is not a defense
/// against deliberate collisions.
fn fnv1a64(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, &byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

/// The state of the cartridge's owned files compared with its record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Ownership {
    /// The source of the previous import, when the files are still what it
    /// wrote.
    pub(super) previous: Option<String>,
    /// Why each conflicting file may not be overwritten.
    pub(super) conflicts: Vec<String>,
}

/// Compares the owned files in `cartridge` with its import record.
///
/// A file conflicts when it exists but the record does not list it, or its
/// contents changed since the record was written. A `palette.bin` whose
/// importer region still holds `rom_palette`'s colors never conflicts:
/// only entries the importer does not own were customized.
pub(super) fn ownership(cartridge: &Path, rom_palette: &Palette) -> anyhow::Result<Ownership> {
    let record = match fs::read_to_string(cartridge.join(RECORD)) {
        Ok(text) => match ron::from_str::<ImportRecord>(&text) {
            Ok(record) => Some(record),
            Err(err) => {
                return Ok(Ownership {
                    previous: None,
                    conflicts: vec![format!("{RECORD} cannot be read ({err})")],
                });
            }
        },
        Err(err) if err.kind() == ErrorKind::NotFound => None,
        Err(err) => {
            return Err(err).with_context(|| format!("failed to read {RECORD}"));
        }
    };

    let rom_region = digest(PALETTE, &rom_palette.to_rgb888());
    let mut conflicts = Vec::new();
    for name in OWNED {
        let bytes = match fs::read(cartridge.join(name)) {
            Ok(bytes) => bytes,
            Err(err) if err.kind() == ErrorKind::NotFound => continue,
            Err(err) => return Err(err).with_context(|| format!("failed to read {name}")),
        };
        let current = digest(name, &bytes);
        let recorded = record
            .as_ref()
            .and_then(|record| record.files.iter().find(|file| file.name == name));
        match recorded {
            Some(recorded) if *recorded == current => {}
            Some(_) => conflicts.push(format!("{name} was changed after the last import")),
            None if name == PALETTE && current == rom_region => {}
            None => conflicts.push(format!("{name} was not written by `glaux import sprites`")),
        }
    }
    Ok(Ownership {
        previous: record.filter(|_| conflicts.is_empty()).map(|r| r.source),
        conflicts,
    })
}

/// What [`recover`] found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Recovery {
    /// No interrupted import.
    None,
    /// A committed import had stopped partway through publishing and was
    /// finished.
    RolledForward,
    /// An import had stopped before committing and was discarded.
    Discarded,
}

/// Finishes or discards an import interrupted in `cartridge`.
pub(super) fn recover(cartridge: &Path) -> anyhow::Result<Recovery> {
    let staging = cartridge.join(STAGING);
    if !staging.is_dir() {
        return Ok(Recovery::None);
    }
    if staging.join(COMMIT).is_file() {
        move_staged(&staging, cartridge)?;
        Ok(Recovery::RolledForward)
    } else {
        fs::remove_dir_all(&staging)
            .with_context(|| format!("failed to discard {}", staging.display()))?;
        Ok(Recovery::Discarded)
    }
}

/// Stages `outputs` and the record text, commits them, and renames them
/// into `cartridge`.
///
/// Call [`recover`] first: an existing staging directory is an error.
pub(super) fn publish(
    cartridge: &Path,
    outputs: &[(&str, Vec<u8>)],
    record: &str,
) -> anyhow::Result<()> {
    let staging = cartridge.join(STAGING);
    fs::create_dir(&staging).with_context(|| format!("failed to create {}", staging.display()))?;
    for (name, bytes) in outputs {
        write_synced(&staging.join(name), bytes)?;
    }
    write_synced(&staging.join(RECORD), record.as_bytes())?;
    write_synced(&staging.join(COMMIT), &[])?;
    sync_dir(&staging)?;
    move_staged(&staging, cartridge)
}

/// Renames every staged file still present into `cartridge`, the record
/// last, then removes the staging directory.
fn move_staged(staging: &Path, cartridge: &Path) -> anyhow::Result<()> {
    for name in OWNED.into_iter().chain([RECORD]) {
        let staged = staging.join(name);
        if !staged.is_file() {
            continue;
        }
        let target = cartridge.join(name);
        fs::rename(&staged, &target)
            .with_context(|| format!("failed to replace {}", target.display()))?;
    }
    sync_dir(cartridge)?;
    fs::remove_dir_all(staging).with_context(|| format!("failed to remove {}", staging.display()))
}

fn write_synced(path: &Path, bytes: &[u8]) -> anyhow::Result<()> {
    let mut file =
        File::create(path).with_context(|| format!("failed to create {}", path.display()))?;
    file.write_all(bytes)
        .and_then(|()| file.sync_all())
        .with_context(|| format!("failed to write {}", path.display()))
}

/// Makes renames and new entries in `dir` durable where the platform allows
/// syncing a directory.
fn sync_dir(dir: &Path) -> anyhow::Result<()> {
    #[cfg(unix)]
    File::open(dir)
        .and_then(|dir| dir.sync_all())
        .with_context(|| format!("failed to sync {}", dir.display()))?;
    #[cfg(not(unix))]
    let _ = dir;
    Ok(())
}

/// Fails with every conflict unless `replace` allows overwriting them.
pub(super) fn ensure_owned(ownership: &Ownership, replace: bool) -> anyhow::Result<()> {
    if ownership.conflicts.is_empty() || replace {
        return Ok(());
    }
    bail!(
        "refusing to overwrite sprite assets this command did not write:\n  {}\n\
         Re-run with --replace to overwrite them.",
        ownership.conflicts.join("\n  ")
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cartridge_fs::fixture::Fixture;
    use pretty_assertions::assert_eq;

    fn outputs(fill: u8) -> Vec<(&'static str, Vec<u8>)> {
        OWNED
            .iter()
            .map(|&name| {
                let length = if name == PALETTE { 768 } else { 32 };
                (name, vec![fill; length])
            })
            .collect()
    }

    fn record_text(outputs: &[(&str, Vec<u8>)]) -> String {
        ImportRecord::new("sprites.png".into(), None, Vec::new(), outputs)
            .to_ron()
            .unwrap()
    }

    fn import(dir: &Path, fill: u8) {
        assert_eq!(recover(dir).unwrap(), Recovery::None);
        let outputs = outputs(fill);
        publish(dir, &outputs, &record_text(&outputs)).unwrap();
    }

    fn contents(dir: &Path) -> Vec<Option<Vec<u8>>> {
        OWNED
            .iter()
            .map(|name| fs::read(dir.join(name)).ok())
            .collect()
    }

    fn rom() -> Palette {
        Palette::provisional()
    }

    #[test]
    fn fnv1a64_matches_the_reference_vectors() {
        assert_eq!(fnv1a64(b""), 0xcbf2_9ce4_8422_2325);
        assert_eq!(fnv1a64(b"a"), 0xaf63_dc4c_8601_ec8c);
        assert_eq!(fnv1a64(b"foobar"), 0x8594_4171_f739_67e8);
    }

    #[test]
    fn a_first_import_and_a_reimport_are_allowed() {
        let fixture = Fixture::new("import-first");
        let dir = &fixture.root();
        assert_eq!(
            ownership(dir, &rom()).unwrap(),
            Ownership {
                previous: None,
                conflicts: Vec::new()
            }
        );
        import(dir, 1);
        assert_eq!(
            contents(dir),
            vec![Some(vec![1; 32]); 3]
                .into_iter()
                .chain([Some(vec![1; 768])])
                .collect::<Vec<_>>()
        );
        assert!(!dir.join(STAGING).exists());

        let owned = ownership(dir, &rom()).unwrap();
        assert_eq!(owned.previous.as_deref(), Some("sprites.png"));
        assert!(owned.conflicts.is_empty());
    }

    #[test]
    fn edited_and_foreign_files_conflict() {
        let fixture = Fixture::new("import-conflicts");
        let dir = &fixture.root();
        fs::write(dir.join("sprites.bin"), [7; 32]).unwrap();
        assert_eq!(
            ownership(dir, &rom()).unwrap().conflicts,
            ["sprites.bin was not written by `glaux import sprites`"]
        );

        import(dir, 1);
        fs::write(dir.join("obp.bin"), [2; 32]).unwrap();
        let owned = ownership(dir, &rom()).unwrap();
        assert_eq!(
            owned.conflicts,
            ["obp.bin was changed after the last import"]
        );
        assert_eq!(owned.previous, None);
        let err = ensure_owned(&owned, false).unwrap_err().to_string();
        assert!(err.contains("obp.bin was changed"), "{err}");
        assert!(err.contains("--replace"), "{err}");
        assert!(ensure_owned(&owned, true).is_ok());
    }

    #[test]
    fn palette_entries_below_the_region_belong_to_the_cartridge() {
        let fixture = Fixture::new("import-palette-owner");
        let dir = &fixture.root();
        let mut custom = rom().to_rgb888();
        custom[..CART_COLOR_START * 3].fill(9);
        fs::write(dir.join(PALETTE), custom).unwrap();
        assert!(ownership(dir, &rom()).unwrap().conflicts.is_empty());

        custom[CART_COLOR_START * 3] ^= 1;
        fs::write(dir.join(PALETTE), custom).unwrap();
        assert_eq!(
            ownership(dir, &rom()).unwrap().conflicts,
            ["palette.bin was not written by `glaux import sprites`"]
        );

        import(dir, 1);
        let mut edited = fs::read(dir.join(PALETTE)).unwrap();
        edited[0] = 0x42;
        fs::write(dir.join(PALETTE), edited).unwrap();
        assert!(ownership(dir, &rom()).unwrap().conflicts.is_empty());
    }

    #[test]
    fn an_unreadable_record_is_a_conflict() {
        let fixture = Fixture::new("import-bad-record");
        fs::write(fixture.root().join(RECORD), "(nonsense").unwrap();
        let conflicts = ownership(&fixture.root(), &rom()).unwrap().conflicts;
        assert_eq!(conflicts.len(), 1);
        assert!(conflicts[0].starts_with("sprites.import.ron cannot be read"));
    }

    /// Stages an import of `fill` by hand, as one interrupted after moving
    /// `moved` files into place would leave it.
    fn interrupted(dir: &Path, fill: u8, committed: bool, moved: usize) {
        let staging = dir.join(STAGING);
        fs::create_dir(&staging).unwrap();
        let outputs = outputs(fill);
        for (name, bytes) in &outputs {
            fs::write(staging.join(name), bytes).unwrap();
        }
        fs::write(staging.join(RECORD), record_text(&outputs)).unwrap();
        if committed {
            fs::write(staging.join(COMMIT), []).unwrap();
        }
        for name in &OWNED[..moved] {
            fs::rename(staging.join(name), dir.join(name)).unwrap();
        }
    }

    #[test]
    fn a_committed_import_is_rolled_forward() {
        let fixture = Fixture::new("import-roll-forward");
        let dir = &fixture.root();
        import(dir, 1);
        interrupted(dir, 2, true, 2);
        assert_eq!(fs::read(dir.join("obp.bin")).unwrap(), [1; 32]);

        assert_eq!(recover(dir).unwrap(), Recovery::RolledForward);
        assert!(!dir.join(STAGING).exists());
        assert_eq!(fs::read(dir.join("sprites.bin")).unwrap(), [2; 32]);
        assert_eq!(fs::read(dir.join("obp.bin")).unwrap(), [2; 32]);
        assert_eq!(fs::read(dir.join(PALETTE)).unwrap(), [2; 768]);
        assert!(ownership(dir, &rom()).unwrap().conflicts.is_empty());
    }

    #[test]
    fn an_uncommitted_import_is_discarded() {
        let fixture = Fixture::new("import-discard");
        let dir = &fixture.root();
        import(dir, 1);
        interrupted(dir, 2, false, 0);

        assert_eq!(recover(dir).unwrap(), Recovery::Discarded);
        assert!(!dir.join(STAGING).exists());
        assert_eq!(fs::read(dir.join("sprites.bin")).unwrap(), [1; 32]);
        assert!(ownership(dir, &rom()).unwrap().conflicts.is_empty());
    }

    #[test]
    fn publishing_over_a_leftover_staging_directory_fails_untouched() {
        let fixture = Fixture::new("import-leftover");
        let dir = &fixture.root();
        import(dir, 1);
        fs::create_dir(dir.join(STAGING)).unwrap();
        let outputs = outputs(2);
        assert!(publish(dir, &outputs, &record_text(&outputs)).is_err());
        assert_eq!(fs::read(dir.join("sprites.bin")).unwrap(), [1; 32]);
    }
}
