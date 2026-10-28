use std::fs;
use std::sync::atomic::{AtomicU32, Ordering};
use std::thread::sleep;

use notify::event::{CreateKind, Flag, RemoveKind};
use pretty_assertions::assert_eq;

use super::*;

const DEBOUNCE: Duration = Duration::from_millis(40);

struct Dir(PathBuf);

impl Dir {
    fn new(label: &str) -> Self {
        static NEXT: AtomicU32 = AtomicU32::new(0);
        let path = std::env::temp_dir().join(format!(
            "glaux-fswatch-{label}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).unwrap();
        Self(fs::canonicalize(path).unwrap())
    }
}

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn watcher() -> FsWatcher {
    FsWatcher::new(DEBOUNCE, || {}).unwrap()
}

/// Polls until `done` accepts what was collected, or fails after a while.
fn collect(watcher: &mut FsWatcher, done: impl Fn(&[FsEvent]) -> bool) -> Vec<FsEvent> {
    let mut all = Vec::new();
    for _ in 0..300 {
        all.extend(watcher.poll());
        if done(&all) {
            return all;
        }
        sleep(Duration::from_millis(10));
    }
    panic!("the expected events never settled: {all:?}");
}

/// Nothing more arrives for a while.
fn quiet(watcher: &mut FsWatcher) -> Vec<FsEvent> {
    sleep(DEBOUNCE * 5);
    let mut all = watcher.poll();
    sleep(DEBOUNCE * 3);
    all.extend(watcher.poll());
    all
}

fn touches(events: &[FsEvent], path: &Path) -> bool {
    events.iter().any(|event| match event {
        FsEvent::Created(p) | FsEvent::Modified(p) | FsEvent::Removed(p) => p == path,
        FsEvent::Renamed { from, to } => from == path || to == path,
        FsEvent::Rescan(_) => false,
    })
}

#[test]
fn a_new_file_is_reported_once_it_settles() {
    let dir = Dir::new("create");
    let mut watcher = watcher();
    watcher.watch(&dir.0, true).unwrap();
    let file = dir.0.join("a.luau");
    fs::write(&file, "one").unwrap();
    let events = collect(&mut watcher, |e| touches(e, &file));
    assert!(
        events
            .iter()
            .all(|e| matches!(e, FsEvent::Created(_) | FsEvent::Modified(_)))
    );
    assert_eq!(
        events
            .iter()
            .filter(|e| touches(&[(*e).clone()], &file))
            .count(),
        1
    );
}

#[test]
fn a_burst_of_writes_is_one_event() {
    let dir = Dir::new("burst");
    let file = dir.0.join("a.luau");
    fs::write(&file, "0").unwrap();
    let mut watcher = watcher();
    watcher.watch(&dir.0, true).unwrap();
    for n in 0..20 {
        fs::write(&file, n.to_string()).unwrap();
        sleep(Duration::from_millis(2));
    }
    let mut events = collect(&mut watcher, |e| touches(e, &file));
    events.extend(quiet(&mut watcher));
    assert_eq!(events.len(), 1, "{events:?}");
    assert!(matches!(
        events[0],
        FsEvent::Modified(_) | FsEvent::Created(_)
    ));
}

#[test]
fn a_removed_file_is_reported() {
    let dir = Dir::new("remove");
    let file = dir.0.join("a.luau");
    fs::write(&file, "x").unwrap();
    let mut watcher = watcher();
    watcher.watch(&dir.0, true).unwrap();
    fs::remove_file(&file).unwrap();
    let events = collect(&mut watcher, |e| touches(e, &file));
    assert_eq!(events.last(), Some(&FsEvent::Removed(file)));
}

#[test]
fn a_save_through_a_temporary_file_is_a_modification_of_the_original() {
    let dir = Dir::new("atomic");
    let file = dir.0.join("a.luau");
    fs::write(&file, "old").unwrap();
    let mut watcher = watcher();
    watcher.watch(&dir.0, true).unwrap();
    let temp = dir.0.join(".a.luau.tmp");
    fs::write(&temp, "new").unwrap();
    fs::rename(&temp, &file).unwrap();
    let mut events = collect(&mut watcher, |e| touches(e, &file));
    events.extend(quiet(&mut watcher));
    assert_eq!(fs::read_to_string(&file).unwrap(), "new");
    let about_file: Vec<_> = events
        .iter()
        .filter(|e| touches(&[(*e).clone()], &file))
        .collect();
    assert_eq!(about_file.len(), 1, "{events:?}");
    assert!(
        matches!(about_file[0], FsEvent::Modified(p) if *p == file),
        "{events:?}"
    );
    // The temporary file is gone by the time anything is reported.
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, FsEvent::Created(p) if *p == temp))
    );
}

#[test]
fn a_rename_is_reported_as_the_old_name_gone_and_the_new_one_present() {
    let dir = Dir::new("rename");
    let (old, new) = (dir.0.join("old.luau"), dir.0.join("new.luau"));
    fs::write(&old, "x").unwrap();
    let mut watcher = watcher();
    watcher.watch(&dir.0, true).unwrap();
    fs::rename(&old, &new).unwrap();
    let events = collect(&mut watcher, |e| touches(e, &old) && touches(e, &new));
    // Backends differ in whether they pair the two names.
    let paired = events.contains(&FsEvent::Renamed {
        from: old.clone(),
        to: new.clone(),
    });
    let split = events.contains(&FsEvent::Removed(old))
        && events
            .iter()
            .any(|e| matches!(e, FsEvent::Created(p) | FsEvent::Modified(p) if *p == new));
    assert!(paired || split, "{events:?}");
}

#[test]
fn files_in_subdirectories_are_seen_and_new_directories_are_watched() {
    let dir = Dir::new("nested");
    let mut watcher = watcher();
    watcher.watch(&dir.0, true).unwrap();
    let sub = dir.0.join("a").join("b");
    fs::create_dir_all(&sub).unwrap();
    collect(&mut watcher, |e| touches(e, &dir.0.join("a")));
    // Give the backend a moment to register the new directories.
    sleep(Duration::from_millis(150));
    quiet(&mut watcher);
    let file = sub.join("deep.luau");
    fs::write(&file, "x").unwrap();
    collect(&mut watcher, |e| touches(e, &file));
}

#[test]
fn a_non_recursive_watch_ignores_what_is_below() {
    let dir = Dir::new("flat");
    let sub = dir.0.join("sub");
    fs::create_dir(&sub).unwrap();
    let mut watcher = watcher();
    watcher.watch(&dir.0, false).unwrap();
    quiet(&mut watcher);
    let below = sub.join("x.luau");
    fs::write(&below, "x").unwrap();
    let beside = dir.0.join("y.luau");
    fs::write(&beside, "y").unwrap();
    let mut events = collect(&mut watcher, |e| touches(e, &beside));
    events.extend(quiet(&mut watcher));
    assert!(!touches(&events, &below), "{events:?}");
}

#[test]
fn an_unwatched_root_reports_nothing_more() {
    let dir = Dir::new("unwatch");
    let mut watcher = watcher();
    let (id, _) = watcher.watch(&dir.0, true).unwrap();
    watcher.unwatch(id);
    fs::write(dir.0.join("a.luau"), "x").unwrap();
    assert_eq!(quiet(&mut watcher), []);
}

#[test]
fn watching_something_that_is_not_there_is_an_error() {
    let dir = Dir::new("missing");
    let error = watcher().watch(&dir.0.join("nope"), true).unwrap_err();
    assert!(matches!(error, WatchError::Path { .. }), "{error}");
}

#[test]
fn the_wake_callback_runs_when_events_arrive() {
    let dir = Dir::new("wake");
    let woken = std::sync::Arc::new(AtomicU32::new(0));
    let seen = std::sync::Arc::clone(&woken);
    let mut watcher = FsWatcher::new(DEBOUNCE, move || {
        seen.fetch_add(1, Ordering::Relaxed);
    })
    .unwrap();
    watcher.watch(&dir.0, true).unwrap();
    fs::write(dir.0.join("a.luau"), "x").unwrap();
    for _ in 0..300 {
        if woken.load(Ordering::Relaxed) > 0 {
            return;
        }
        sleep(Duration::from_millis(10));
    }
    panic!("the watcher never woke its consumer");
}

#[test]
fn next_due_follows_the_quiet_interval() {
    let dir = Dir::new("due");
    let mut watcher = watcher();
    let (_, root) = watcher.watch(&dir.0, true).unwrap();
    assert_eq!(watcher.next_due(), None);
    let began = Instant::now();
    let path = root.join("a.luau");
    fs::write(&path, "x").unwrap();
    watcher.ingest(
        Ok(Event::new(EventKind::Create(CreateKind::File)).add_path(path.clone())),
        began,
    );
    assert_eq!(watcher.next_due(), Some(began + DEBOUNCE));
    assert_eq!(watcher.poll_at(began + DEBOUNCE / 2), []);
    assert_eq!(watcher.poll_at(began + DEBOUNCE), [FsEvent::Created(path)]);
    assert_eq!(watcher.next_due(), None);
}

#[test]
fn lost_events_become_a_rescan_of_the_root_and_absorb_what_settled_under_it() {
    let dir = Dir::new("rescan");
    let other = Dir::new("rescan-other");
    let mut watcher = watcher();
    let (_, root) = watcher.watch(&dir.0, true).unwrap();
    let (_, elsewhere) = watcher.watch(&other.0, true).unwrap();
    let began = Instant::now();
    let gone = root.join("gone.luau");
    watcher.ingest(
        Ok(Event::new(EventKind::Remove(RemoveKind::File)).add_path(gone)),
        began,
    );
    let kept = elsewhere.join("kept.luau");
    watcher.ingest(
        Ok(Event::new(EventKind::Remove(RemoveKind::File)).add_path(kept.clone())),
        began,
    );
    watcher.ingest(
        Ok(Event::new(EventKind::Other)
            .set_flag(Flag::Rescan)
            .add_path(root.join("any"))),
        began,
    );
    let mut events = watcher.poll_at(began + DEBOUNCE);
    events.sort_by_key(|event| format!("{event:?}"));
    assert_eq!(events, [FsEvent::Removed(kept), FsEvent::Rescan(root)]);
}

#[test]
fn a_backend_error_without_paths_rescans_every_root() {
    let one = Dir::new("error-one");
    let two = Dir::new("error-two");
    let mut watcher = watcher();
    let (_, a) = watcher.watch(&one.0, true).unwrap();
    let (_, b) = watcher.watch(&two.0, true).unwrap();
    let began = Instant::now();
    watcher.ingest(Err(notify::Error::generic("the buffer overflowed")), began);
    let mut events = watcher.poll_at(began + DEBOUNCE);
    events.sort_by_key(|event| format!("{event:?}"));
    let mut expected = vec![FsEvent::Rescan(a), FsEvent::Rescan(b)];
    expected.sort_by_key(|event| format!("{event:?}"));
    assert_eq!(events, expected);
}
