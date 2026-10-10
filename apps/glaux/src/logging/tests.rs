use super::*;
use crate::scratch_dir::ScratchDir;
use pretty_assertions::assert_eq;
use std::time::Duration;

fn name_at(secs: u64) -> String {
    session_log_name(UNIX_EPOCH + Duration::from_secs(secs))
}

#[test]
fn session_names_use_utc_timestamps() {
    assert_eq!(name_at(0), "glaux-1970-01-01T00-00-00Z.log");
    // 2024-02-29T23:59:59Z, a leap day.
    assert_eq!(name_at(1_709_251_199), "glaux-2024-02-29T23-59-59Z.log");
    // 2026-09-24T14:03:05Z.
    assert_eq!(name_at(1_790_258_585), "glaux-2026-09-24T14-03-05Z.log");
}

#[test]
fn stale_sessions_are_the_oldest_beyond_keep() {
    let names = [
        "glaux-2026-01-03T00-00-00Z.log",
        "glaux-2026-01-01T00-00-00Z.log",
        "glaux-2026-01-04T00-00-00Z.log",
        "glaux-2026-01-02T00-00-00Z.log",
    ];
    assert_eq!(
        stale_sessions(names, 2),
        [
            "glaux-2026-01-01T00-00-00Z.log",
            "glaux-2026-01-02T00-00-00Z.log"
        ]
    );
}

#[test]
fn nothing_is_stale_within_keep() {
    let names = [
        "glaux-2026-01-01T00-00-00Z.log",
        "glaux-2026-01-02T00-00-00Z.log",
    ];
    assert!(stale_sessions(names, 2).is_empty());
    assert!(stale_sessions(names, 10).is_empty());
}

#[test]
fn keep_zero_makes_every_session_stale() {
    let names = [
        "glaux-2026-01-02T00-00-00Z.log",
        "glaux-2026-01-01T00-00-00Z.log",
    ];
    assert_eq!(
        stale_sessions(names, 0),
        [
            "glaux-2026-01-01T00-00-00Z.log",
            "glaux-2026-01-02T00-00-00Z.log"
        ]
    );
}

#[test]
fn unrelated_files_are_never_stale() {
    let names = [
        "notes.txt",
        "other-1.log",
        "glaux-latest.log",
        "glaux-2026-01-01T00-00-00Z.log.bak",
        "glaux-2026-01-01T00-00-00Z-x.log",
        "glaux-2026-01-01T00-00-00Z.log",
        "glaux-2026-01-01T00-00-00Z-2.log",
    ];
    assert_eq!(
        stale_sessions(names, 0),
        [
            "glaux-2026-01-01T00-00-00Z-2.log",
            "glaux-2026-01-01T00-00-00Z.log"
        ]
    );
}

#[test]
fn file_logging_fails_softly_when_the_directory_cannot_be_created() {
    let dir = ScratchDir::new("blocked");
    let blocker = dir.join("not-a-dir");
    fs::write(&blocker, "").unwrap();

    let result = prepare_file_logging(&blocker.join("logs"), SystemTime::now());

    let err = result.unwrap_err();
    assert!(format!("{err:#}").contains("failed to create"), "{err:#}");
    // The fallback is stderr-only logging; `init` does not propagate this.
}

#[test]
fn file_logging_keeps_at_most_max_sessions() {
    let dir = ScratchDir::new("retention");
    let logs = dir.join("logs");
    fs::create_dir_all(&logs).unwrap();
    for i in 0..12 {
        fs::write(logs.join(name_at(i * 60)), "").unwrap();
    }
    fs::write(logs.join("notes.txt"), "").unwrap();

    let now = UNIX_EPOCH + Duration::from_secs(1_790_258_585);
    let session = prepare_file_logging(&logs, now).unwrap();
    assert!(session.pruning_errors.is_empty());

    let mut remaining: Vec<String> = fs::read_dir(&logs)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().into_string().unwrap())
        .collect();
    remaining.sort();

    let mut expected: Vec<String> = (3..12).map(|i| name_at(i * 60)).collect();
    expected.push(session_log_name(now));
    expected.push("notes.txt".to_string());
    expected.sort();
    assert_eq!(remaining, expected);
}

#[test]
fn same_second_sessions_get_their_own_file() {
    let dir = ScratchDir::new("collision");
    let now = UNIX_EPOCH + Duration::from_secs(60);

    prepare_file_logging(&dir, now).unwrap();
    prepare_file_logging(&dir, now).unwrap();

    assert!(dir.join("glaux-1970-01-01T00-01-00Z.log").is_file());
    assert!(dir.join("glaux-1970-01-01T00-01-00Z-2.log").is_file());
}
