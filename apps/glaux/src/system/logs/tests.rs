use glaux_script::{LogEntry, LogLevel};
use pretty_assertions::assert_eq;

use super::store::STORE_LIMIT;
use super::{Category, LogStore, Severity};

fn entry(text: &str) -> LogEntry {
    LogEntry {
        level: LogLevel::Info,
        text: text.into(),
    }
}

#[test]
fn the_oldest_records_are_dropped_past_the_limit() {
    let mut store = LogStore::default();
    for n in 0..STORE_LIMIT + 5 {
        store.push_cartridge(vec![entry(&n.to_string())], 0);
    }
    assert_eq!(store.len(), STORE_LIMIT);
    assert_eq!(&*store.iter().next().unwrap().message, "5");
}

#[test]
fn the_error_that_ended_the_run_outlives_a_flood() {
    let mut store = LogStore::default();
    store.push_terminal(Category::Runtime, "main.luau:3: boom", None);
    for n in 0..STORE_LIMIT + 5 {
        store.push_cartridge(vec![entry(&n.to_string())], 0);
    }
    let first = store.iter().next().unwrap();
    assert_eq!(first.severity, Severity::Error);
    assert_eq!(&*first.message, "main.luau:3: boom");
    assert_eq!(store.len(), STORE_LIMIT + 1);
}

#[test]
fn refused_entries_are_reported_as_one_system_warning() {
    let mut store = LogStore::default();
    let made = store.push_cartridge(vec![entry("kept")], 40);
    assert_eq!(made.len(), 2);
    assert_eq!(&*made[1].message, "40 log messages skipped");
    assert_eq!(made[1].severity, Severity::Warn);
}

#[test]
fn a_new_run_starts_with_an_empty_log_and_keeps_counting() {
    let mut store = LogStore::default();
    store.push_terminal(Category::Runtime, "boom", None);
    let before = store.push_cartridge(vec![entry("a")], 0)[0].seq;
    store.begin_run();
    assert_eq!(store.len(), 0);
    assert!(store.terminal().is_none());
    assert!(store.push_cartridge(vec![entry("b")], 0)[0].seq > before);
}

mod panel {
    use glaux_input::{Button, InputEvent, Key};
    use glaux_script::{LogEntry, LogLevel};
    use pretty_assertions::assert_eq;

    use crate::cartridge::source::SourcePath;
    use crate::system::logs::{Category, LogPanel, LogStore, PanelAction, SourceLocation};

    fn entry(level: LogLevel, text: &str) -> LogEntry {
        LogEntry {
            level,
            text: text.into(),
        }
    }

    fn open_on(store: &LogStore) -> LogPanel {
        let mut panel = LogPanel::default();
        panel.toggle();
        panel.sync(store);
        panel
    }

    fn up(panel: &mut LogPanel, store: &LogStore) {
        panel.input(InputEvent::ButtonDown(Button::Up), store, false);
    }

    fn highlighted(panel: &LogPanel) -> Option<String> {
        panel
            .selected_record()
            .map(|record| record.message.to_string())
    }

    #[test]
    fn it_follows_the_newest_record_until_the_user_moves_up() {
        let mut store = LogStore::default();
        store.push_cartridge(
            vec![entry(LogLevel::Info, "a"), entry(LogLevel::Info, "b")],
            0,
        );
        let mut panel = open_on(&store);
        assert_eq!(highlighted(&panel).as_deref(), Some("b"));

        up(&mut panel, &store);
        assert_eq!(highlighted(&panel).as_deref(), Some("a"));

        // A new record does not drag the view away from what is being read.
        store.push_cartridge(vec![entry(LogLevel::Info, "c")], 0);
        panel.sync(&store);
        assert_eq!(highlighted(&panel).as_deref(), Some("a"));

        // Moving past the newest, or End, follows again.
        panel.input(InputEvent::KeyDown(Key::End), &store, false);
        assert_eq!(highlighted(&panel).as_deref(), Some("c"));
        store.push_cartridge(vec![entry(LogLevel::Info, "d")], 0);
        panel.sync(&store);
        assert_eq!(highlighted(&panel).as_deref(), Some("d"));
    }

    #[test]
    fn the_level_and_source_filters_hide_records_without_losing_them() {
        let mut store = LogStore::default();
        store.push_cartridge(
            vec![
                entry(LogLevel::Trace, "noise"),
                entry(LogLevel::Warn, "careful"),
            ],
            0,
        );
        store.push_terminal(Category::Runtime, "boom", None);
        let mut panel = open_on(&store);
        let listed = |panel: &LogPanel| panel.listing.rows.len();
        assert_eq!(listed(&panel), 3);

        // Info, then Warn: the trace record goes.
        panel.input(InputEvent::Text('l'), &store, false);
        panel.input(InputEvent::Text('l'), &store, false);
        panel.input(InputEvent::Text('l'), &store, false);
        assert_eq!(listed(&panel), 2);

        // Cartridge only: the system's error goes too.
        panel.input(InputEvent::Text('s'), &store, false);
        assert_eq!(listed(&panel), 1);
        assert_eq!(highlighted(&panel).as_deref(), Some("careful"));

        // Back to everything.
        panel.input(InputEvent::Text('s'), &store, false);
        panel.input(InputEvent::Text('s'), &store, false);
        panel.input(InputEvent::Text('l'), &store, false);
        panel.input(InputEvent::Text('l'), &store, false);
        assert_eq!(listed(&panel), 3);
    }

    #[test]
    fn enter_shows_a_script_only_where_there_is_an_editor_and_a_position() {
        let location = SourceLocation {
            path: SourcePath::parse("code/main.luau").unwrap(),
            line: 3,
        };
        let mut store = LogStore::default();
        store.push_terminal(Category::Runtime, "boom", Some(location.clone()));
        let mut panel = open_on(&store);
        let enter = InputEvent::Enter;
        assert_eq!(panel.input(enter, &store, false), PanelAction::None);
        assert_eq!(
            panel.input(enter, &store, true),
            PanelAction::Jump(location)
        );

        let mut store = LogStore::default();
        store.push_cartridge(vec![entry(LogLevel::Info, "plain")], 0);
        let mut panel = open_on(&store);
        assert_eq!(panel.input(enter, &store, true), PanelAction::None);
    }

    #[test]
    fn a_closed_panel_builds_nothing() {
        let mut store = LogStore::default();
        store.push_cartridge(vec![entry(LogLevel::Info, "a")], 0);
        let mut panel = LogPanel::default();
        panel.sync(&store);
        assert!(panel.listing.rows.is_empty());
        assert_eq!(panel.listing.revision, None);
    }

    #[test]
    fn tab_reads_one_record_whole_and_up_down_then_scroll_it_instead_of_moving() {
        use glaux_machine::framebuffer::Framebuffer;

        use crate::system::test_font::LAYOUT_FONT;

        let mut store = LogStore::default();
        let long = (0..40)
            .map(|n| format!("line {n}"))
            .collect::<Vec<_>>()
            .join("\n");
        store.push_cartridge(
            vec![
                entry(LogLevel::Info, "before"),
                entry(LogLevel::Info, &long),
            ],
            0,
        );
        let mut panel = open_on(&store);
        let mut frame = Framebuffer::default();
        let mut draw = |panel: &LogPanel| panel.render(&mut frame, &LAYOUT_FONT, &|c| c, false);

        panel.input(InputEvent::Tab { reverse: false }, &store, false);
        draw(&panel);
        panel.input(InputEvent::Down { select: false }, &store, false);
        panel.input(InputEvent::Up { select: false }, &store, false);
        panel.input(InputEvent::Down { select: false }, &store, false);
        draw(&panel);
        assert_eq!(panel.reading, Some(1));
        assert!(highlighted(&panel).unwrap().starts_with("line 0"));

        // Escape leaves the text, then closes the panel as before.
        panel.input(InputEvent::Break, &store, false);
        assert!(panel.is_open());
        up(&mut panel, &store);
        assert_eq!(highlighted(&panel).as_deref(), Some("before"));
        panel.input(InputEvent::Break, &store, false);
        assert!(!panel.is_open());
    }
}
