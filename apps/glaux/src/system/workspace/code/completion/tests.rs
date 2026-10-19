use glaux_input::{InputEvent, Point, PointerButton};
use glaux_sense::{Candidate, CandidateKind, Doc, Origin};
use pretty_assertions::assert_eq;

use super::*;
use crate::system::code_document::DocumentId;

pub(super) fn candidate(label: &str) -> Candidate {
    Candidate {
        label: label.to_owned(),
        kind: CandidateKind::Function,
        origin: Origin::Api,
        insertion: Insertion::Name(label.to_owned()),
        detail: "gfx".to_owned(),
        doc: Doc::None,
    }
}

pub(super) fn answer(count: usize, anchor: std::ops::Range<usize>) -> Completions {
    Completions {
        revision: 0,
        anchor,
        prefix: String::new(),
        items: (0..count).map(|n| candidate(&format!("item{n}"))).collect(),
    }
}

fn popup(count: usize) -> CompletionPopup {
    CompletionPopup::new(DocumentId::from_raw(0), answer(count, 0..0), Vec::new()).unwrap()
}

fn handle(popup: &mut CompletionPopup, event: InputEvent) -> CompletionAction {
    popup.handle_input(event, None, None)
}

const DOWN: InputEvent = InputEvent::Down { select: false };
const UP: InputEvent = InputEvent::Up { select: false };

#[test]
fn nothing_to_list_makes_no_popup() {
    assert_eq!(
        CompletionPopup::new(DocumentId::from_raw(0), answer(0, 0..0), Vec::new()),
        None
    );
}

#[test]
fn up_and_down_move_the_pick_and_stop_at_the_ends() {
    let mut popup = popup(3);
    assert_eq!(popup.selected(), 0);
    assert_eq!(handle(&mut popup, UP), CompletionAction::Stay);
    assert_eq!(popup.selected(), 0);
    for expected in [1, 2, 2] {
        assert_eq!(handle(&mut popup, DOWN), CompletionAction::Stay);
        assert_eq!(popup.selected(), expected);
    }
    assert_eq!(handle(&mut popup, UP), CompletionAction::Stay);
    assert_eq!(popup.selected(), 1);
}

#[test]
fn the_list_scrolls_just_far_enough_to_show_the_pick() {
    let mut popup = popup(20);
    for _ in 0..VISIBLE_ROWS - 1 {
        handle(&mut popup, DOWN);
    }
    assert_eq!((popup.selected(), popup.first()), (7, 0));
    handle(&mut popup, DOWN);
    assert_eq!((popup.selected(), popup.first()), (8, 1));
    for _ in 0..12 {
        handle(&mut popup, DOWN);
    }
    assert_eq!((popup.selected(), popup.first()), (19, 12));
    for _ in 0..7 {
        handle(&mut popup, UP);
    }
    assert_eq!((popup.selected(), popup.first()), (12, 12));
    handle(&mut popup, UP);
    assert_eq!((popup.selected(), popup.first()), (11, 11));
}

#[test]
fn page_keys_move_the_pick_a_screenful() {
    let mut popup = popup(30);
    handle(&mut popup, InputEvent::PageDown { select: false });
    assert_eq!(popup.selected(), VISIBLE_ROWS);
    handle(&mut popup, InputEvent::PageUp { select: false });
    assert_eq!((popup.selected(), popup.first()), (0, 0));
}

#[test]
fn tab_accepts_the_pick_but_shift_tab_does_not() {
    let mut popup = popup(3);
    assert_eq!(
        handle(&mut popup, InputEvent::Tab { reverse: false }),
        CompletionAction::Accept
    );
    assert_eq!(
        handle(&mut popup, InputEvent::Tab { reverse: true }),
        CompletionAction::Dismiss
    );
}

#[test]
fn enter_accepts_only_after_the_user_moved_the_pick() {
    let mut popup = popup(3);
    assert!(!popup.navigated());
    assert_eq!(
        handle(&mut popup, InputEvent::Enter),
        CompletionAction::Dismiss
    );
    handle(&mut popup, DOWN);
    assert!(popup.navigated());
    assert_eq!(
        handle(&mut popup, InputEvent::Enter),
        CompletionAction::Accept
    );
}

#[test]
fn pressing_down_at_the_last_item_still_counts_as_choosing() {
    let mut popup = popup(1);
    handle(&mut popup, DOWN);
    assert_eq!(
        handle(&mut popup, InputEvent::Enter),
        CompletionAction::Accept
    );
}

#[test]
fn escape_closes_and_is_used_up() {
    assert_eq!(
        handle(&mut popup(2), InputEvent::Break),
        CompletionAction::Close
    );
}

#[test]
fn typing_and_backspace_go_on_to_the_text() {
    let mut popup = popup(2);
    assert_eq!(
        handle(&mut popup, InputEvent::Text('a')),
        CompletionAction::Pass
    );
    assert_eq!(
        handle(&mut popup, InputEvent::Backspace),
        CompletionAction::Pass
    );
}

#[test]
fn moving_the_caret_or_leaving_the_word_dismisses_it() {
    for event in [
        InputEvent::Left { select: false },
        InputEvent::Right { select: false },
        InputEvent::Home { select: false },
        InputEvent::End { select: false },
        InputEvent::WordLeft { select: false },
        InputEvent::DocumentEnd { select: false },
        InputEvent::Up { select: true },
        InputEvent::Find,
        InputEvent::Undo,
        InputEvent::Delete,
        InputEvent::Complete,
    ] {
        assert_eq!(
            handle(&mut popup(2), event),
            CompletionAction::Dismiss,
            "{event:?}"
        );
    }
}

#[test]
fn accepting_a_name_replaces_the_word_with_it() {
    let completions = Completions {
        items: vec![candidate("rectfill")],
        ..answer(0, 4..6)
    };
    let popup = CompletionPopup::new(DocumentId::from_raw(0), completions, Vec::new()).unwrap();
    assert_eq!(
        popup.accepted(),
        Some(Accepted {
            edit: Edit {
                range: 4..6,
                text: "rectfill".to_owned()
            },
            caret: None,
        })
    );
}

#[test]
fn accepting_a_declaration_puts_the_caret_inside_its_body() {
    let mut tick = candidate("tick");
    tick.insertion = Insertion::Declaration {
        text: "function tick()\n  \nend".to_owned(),
        caret_in_text: 18,
    };
    let completions = Completions {
        items: vec![tick],
        ..answer(0, 10..12)
    };
    let popup = CompletionPopup::new(DocumentId::from_raw(0), completions, Vec::new()).unwrap();
    let accepted = popup.accepted().unwrap();
    assert_eq!(accepted.edit.range, 10..12);
    assert_eq!(accepted.caret, Some(28));
}

#[test]
fn clicking_a_row_picks_and_accepts_it_and_clicking_elsewhere_dismisses() {
    let layout = CompletionLayout::new(
        Place {
            font: &crate::system::test_font::LAYOUT_FONT,
            area: layout_area(),
            cell_w: 6,
            cell_h: 12,
        },
        &answer(5, 0..0).items,
        0,
        0,
        None,
        (40, 40),
    );
    let row = layout.rows[3].1;
    let press = |at| InputEvent::PointerDown {
        button: PointerButton::Primary,
        at,
        select: false,
    };
    let mut popup = popup(5);
    let on_row = Point::new(row.x as u16 + 2, row.y as u16 + 2);
    assert_eq!(
        popup.handle_input(press(on_row), Some(&layout), None),
        CompletionAction::Accept
    );
    assert_eq!(popup.selected(), 3);
    assert_eq!(
        popup.handle_input(press(Point::new(2, 2)), Some(&layout), None),
        CompletionAction::Dismiss
    );
}

#[test]
fn the_wheel_scrolls_the_list_under_the_pointer_and_dismisses_elsewhere() {
    let layout = CompletionLayout::new(
        Place {
            font: &crate::system::test_font::LAYOUT_FONT,
            area: layout_area(),
            cell_w: 6,
            cell_h: 12,
        },
        &answer(20, 0..0).items,
        0,
        0,
        None,
        (40, 40),
    );
    let over = Point::new(layout.list.x as u16 + 3, layout.list.y as u16 + 5);
    let mut popup = popup(20);
    // Turning away from the user scrolls toward the first item, so the
    // other way moves down.
    assert_eq!(
        popup.handle_input(InputEvent::Wheel { delta: -2 }, Some(&layout), Some(over)),
        CompletionAction::Stay
    );
    assert_eq!((popup.first(), popup.selected()), (2, 2));
    assert!(!popup.navigated());
    assert_eq!(
        popup.handle_input(
            InputEvent::Wheel { delta: -1 },
            Some(&layout),
            Some(Point::new(1, 1))
        ),
        CompletionAction::Dismiss
    );
}

fn layout_area() -> crate::system::workspace::Rect {
    crate::system::workspace::Rect {
        x: 0,
        y: 28,
        width: 320,
        height: 200,
    }
}

#[test]
fn a_popup_only_fits_the_text_and_caret_it_was_made_for() {
    let fixture = crate::cartridge_fs::fixture::Fixture::new("completion-current");
    let (_, session) = super::super::tests::session_with_text(&fixture, &["a.luau"], "au");
    let document = session.active_document().unwrap();
    let completions = Completions {
        revision: document.revision(),
        ..answer(1, 0..2)
    };
    let popup = CompletionPopup::new(document.id(), completions, Vec::new()).unwrap();
    assert!(popup.is_current(document, 2));
    assert!(!popup.is_current(document, 1));
    let elsewhere = CompletionPopup {
        document: DocumentId::from_raw(u64::MAX),
        ..popup
    };
    assert!(!elsewhere.is_current(document, 2));
}
