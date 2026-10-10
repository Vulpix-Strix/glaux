use super::*;
use pretty_assertions::assert_eq;

fn history(lines: &[&str]) -> History {
    let mut history = History::default();
    for line in lines {
        history.push(line);
    }
    history
}

fn entries(history: &History) -> Vec<&str> {
    history.entries().collect()
}

#[test]
fn evicts_oldest_entry_past_the_limit() {
    let mut history = History::default();
    for n in 0..HISTORY_LIMIT + 2 {
        history.push(&format!("cmd {n}"));
    }
    let entries = entries(&history);
    assert_eq!(entries.len(), HISTORY_LIMIT);
    assert_eq!(entries[0], "cmd 2");
    assert_eq!(
        entries.last(),
        Some(&format!("cmd {}", HISTORY_LIMIT + 1).as_str())
    );
}

#[test]
fn blank_lines_are_not_recorded() {
    assert_eq!(entries(&history(&["", "   ", "info"])), ["info"]);
}

#[test]
fn only_adjacent_duplicates_are_suppressed() {
    assert_eq!(
        entries(&history(&["help", "help", "info", "help"])),
        ["help", "info", "help"]
    );
}

#[test]
fn up_walks_older_and_stops_at_the_oldest() {
    let mut history = history(&["a", "b", "c"]);
    assert_eq!(history.older(""), Some("c"));
    assert_eq!(history.older("c"), Some("b"));
    assert_eq!(history.older("b"), Some("a"));
    assert_eq!(history.older("a"), None);
    assert_eq!(history.newer(), Some("b"));
}

#[test]
fn down_walks_newer_then_restores_the_draft() {
    let mut history = history(&["a", "b"]);
    history.older("dra");
    history.older("b");
    assert_eq!(history.newer(), Some("b"));
    assert_eq!(history.newer(), Some("dra"));
    assert_eq!(history.newer(), None, "navigation ended");
    assert_eq!(history.older("x"), Some("b"), "starts again from newest");
}

#[test]
fn down_without_navigation_does_nothing() {
    let mut history = history(&["a"]);
    assert_eq!(history.newer(), None);
}

#[test]
fn up_on_empty_history_does_nothing() {
    assert_eq!(History::default().older("draft"), None);
}

#[test]
fn reset_and_push_end_navigation() {
    let mut history = history(&["a", "b"]);
    history.older("");
    history.older("b");
    history.reset();
    assert_eq!(history.older(""), Some("b"));

    history.push("c");
    assert_eq!(history.newer(), None);
    assert_eq!(history.older(""), Some("c"));
}
