use pretty_assertions::assert_eq;

use crate::{
    LogLevel, LogSink, MAX_ENTRY_BYTES, MAX_ENTRY_LINES, MAX_ORDINARY_ENTRIES, MAX_WARN_ENTRIES,
};

fn fill(sink: &LogSink, level: LogLevel, count: u32) -> u32 {
    (0..count)
        .filter(|_| {
            let admitted = sink.admit(level);
            if admitted {
                sink.push(level, "x".into());
            }
            admitted
        })
        .count() as u32
}

#[test]
fn a_flood_of_ordinary_levels_does_not_use_the_warn_allowance() {
    let sink = LogSink::default();
    let kept = fill(&sink, LogLevel::Trace, MAX_ORDINARY_ENTRIES + 10);
    assert_eq!(kept, MAX_ORDINARY_ENTRIES);
    assert_eq!(fill(&sink, LogLevel::Warn, 3), 3);
    assert_eq!(sink.take_dropped(), 10);
}

#[test]
fn warns_have_their_own_limit_and_taking_renews_both() {
    let sink = LogSink::default();
    assert_eq!(
        fill(&sink, LogLevel::Warn, MAX_WARN_ENTRIES + 1),
        MAX_WARN_ENTRIES
    );
    assert_eq!(sink.take().len() as u32, MAX_WARN_ENTRIES);
    assert_eq!(sink.take_dropped(), 1);
    assert_eq!(sink.take_dropped(), 0);
    assert_eq!(fill(&sink, LogLevel::Warn, 1), 1);
}

#[test]
fn long_text_is_cut_on_a_character_boundary() {
    let sink = LogSink::default();
    // Two-byte characters, so the byte limit falls inside one when odd.
    let text = "é".repeat(MAX_ENTRY_BYTES);
    assert!(sink.admit(LogLevel::Info));
    sink.push(LogLevel::Info, text);
    let kept = sink.take().remove(0).text;
    assert_eq!(kept.len(), MAX_ENTRY_BYTES);
    assert!(kept.chars().all(|c| c == 'é'));

    assert!(sink.admit(LogLevel::Info));
    sink.push(LogLevel::Info, format!("a{}", "é".repeat(MAX_ENTRY_BYTES)));
    // "a" takes one byte, so the last two-byte character would end one byte
    // past the limit and is dropped whole.
    assert_eq!(sink.take().remove(0).text.len(), MAX_ENTRY_BYTES - 1);
}

#[test]
fn extra_lines_are_cut() {
    let sink = LogSink::default();
    let text = vec!["row"; MAX_ENTRY_LINES + 5].join("\n");
    assert!(sink.admit(LogLevel::Debug));
    sink.push(LogLevel::Debug, text);
    assert_eq!(sink.take()[0].text.lines().count(), MAX_ENTRY_LINES);
}
