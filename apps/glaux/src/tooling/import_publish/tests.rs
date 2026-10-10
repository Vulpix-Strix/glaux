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
