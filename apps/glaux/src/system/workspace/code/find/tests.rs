use super::*;
use pretty_assertions::assert_eq;

const TAB: InputEvent = InputEvent::Tab { reverse: false };
const BACK_TAB: InputEvent = InputEvent::Tab { reverse: true };

#[test]
fn tab_cycles_through_the_parts_shown() {
    let mut bar = FindBar::new("", false);
    let mut seen = Vec::new();
    for _ in 0..3 {
        bar.handle_input(TAB);
        seen.push(bar.focus());
    }
    assert_eq!(
        seen,
        [
            Some(FindFocus::MatchCase),
            Some(FindFocus::WholeWord),
            Some(FindFocus::Query)
        ]
    );
    bar.reopen(None, true);
    bar.handle_input(TAB);
    assert_eq!(bar.focus(), Some(FindFocus::Replacement));
    bar.handle_input(BACK_TAB);
    bar.handle_input(BACK_TAB);
    assert_eq!(bar.focus(), Some(FindFocus::WholeWord), "wraps backward");
}

#[test]
fn enter_acts_on_the_focused_part() {
    let mut bar = FindBar::new("x", true);
    assert_eq!(bar.handle_input(InputEvent::Enter), FindAction::Next);
    bar.handle_input(TAB);
    assert_eq!(bar.handle_input(InputEvent::Enter), FindAction::ReplaceOne);
    bar.handle_input(TAB);
    bar.handle_input(TAB);
    assert_eq!(bar.handle_input(InputEvent::Enter), FindAction::ReplaceAll);
    bar.handle_input(TAB);
    assert_eq!(bar.handle_input(InputEvent::Enter), FindAction::Search);
    assert!(bar.options().match_case);
    bar.handle_input(TAB);
    assert_eq!(bar.handle_input(InputEvent::Text(' ')), FindAction::Search);
    assert!(bar.options().whole_word);
}

#[test]
fn typing_a_query_searches_and_other_commands_are_left_to_the_view() {
    let mut bar = FindBar::new("", false);
    assert_eq!(bar.handle_input(InputEvent::Text('a')), FindAction::Search);
    assert_eq!(bar.query(), "a");
    assert_eq!(bar.handle_input(InputEvent::Save), FindAction::Unhandled);
    assert_eq!(bar.handle_input(InputEvent::Break), FindAction::Close);
    bar.unfocus();
    assert_eq!(
        bar.handle_input(InputEvent::Text('b')),
        FindAction::Unhandled
    );
    assert_eq!(bar.query(), "a");
}

#[test]
fn reopening_keeps_the_query_unless_given_one() {
    let mut bar = FindBar::new("speed", false);
    bar.unfocus();
    bar.reopen(None, false);
    assert_eq!(
        (bar.query(), bar.focus()),
        ("speed".into(), Some(FindFocus::Query))
    );
    // The whole query is selected, so typing replaces it.
    bar.handle_input(InputEvent::Text('x'));
    assert_eq!(bar.query(), "x");
    bar.reopen(Some("max"), false);
    assert_eq!(bar.query(), "max");
}

#[test]
fn the_bar_shrinks_nothing_but_the_top_of_the_text() {
    let text = Rect {
        x: 0,
        y: 40,
        width: 320,
        height: 180,
    };
    for replacing in [false, true] {
        let layout = FindLayout::new(text, 6, 12, replacing);
        assert_eq!(
            (layout.panel.x, layout.panel.y, layout.panel.width),
            (0, 40, 320)
        );
        let mut parts = vec![
            layout.query,
            layout.count,
            layout.match_case,
            layout.whole_word,
            layout.previous,
            layout.next,
            layout.close,
        ];
        if replacing {
            parts.extend([layout.replacement, layout.replace_one, layout.replace_all]);
        }
        for (index, part) in parts.iter().enumerate() {
            assert!(
                part.x >= layout.panel.x && part.right() <= layout.panel.right(),
                "{part:?}"
            );
            assert!(
                part.y >= layout.panel.y && part.bottom() < layout.panel.bottom(),
                "{part:?}"
            );
            for other in &parts[index + 1..] {
                assert!(!part.overlaps(*other), "{part:?} {other:?}");
            }
        }
    }
}
