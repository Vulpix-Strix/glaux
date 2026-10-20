use pretty_assertions::assert_eq;

use super::*;

fn long_roll() -> Roll {
    Roll::new((0..40).map(|n| RollLine::Text(n.to_string())).collect())
}

fn skip_intro(roll: &mut Roll) {
    for _ in 0..INTRO_TICKS {
        roll.tick();
    }
}

fn space() -> InputEvent {
    InputEvent::Text(' ')
}

#[test]
fn the_title_card_ends_by_itself_or_with_space() {
    let mut roll = long_roll();
    skip_intro(&mut roll);
    assert_eq!(roll.intro, 0);
    let mut roll = long_roll();
    assert!(roll.handle_input(space()));
    assert_eq!(roll.intro, 0);
    assert!(!roll.is_paused());
}

#[test]
fn the_roll_moves_one_line_a_second() {
    let mut roll = long_roll();
    skip_intro(&mut roll);
    for _ in 0..LINE_TICKS * 3 {
        roll.tick();
    }
    assert_eq!(roll.offset, 3);
}

#[test]
fn space_pauses_and_resumes_the_roll() {
    let mut roll = long_roll();
    skip_intro(&mut roll);
    roll.handle_input(space());
    assert!(roll.is_paused());
    for _ in 0..LINE_TICKS * 2 {
        roll.tick();
    }
    assert_eq!(roll.offset, 0);
    roll.handle_input(space());
    for _ in 0..LINE_TICKS {
        roll.tick();
    }
    assert_eq!(roll.offset, 1);
}

#[test]
fn scrolling_by_hand_holds_the_roll_for_five_seconds() {
    let mut roll = long_roll();
    skip_intro(&mut roll);
    roll.handle_input(InputEvent::Down { select: false });
    assert_eq!(roll.offset, 1);
    for _ in 0..HOLD_TICKS {
        roll.tick();
    }
    assert_eq!(roll.offset, 1);
    for _ in 0..LINE_TICKS {
        roll.tick();
    }
    assert_eq!(roll.offset, 2);
}

#[test]
fn scrolling_by_hand_during_the_title_card_skips_it() {
    let mut roll = long_roll();
    roll.handle_input(InputEvent::Down { select: false });
    assert_eq!((roll.intro, roll.offset), (0, 1));
}

#[test]
fn scrolling_stops_at_both_ends() {
    let mut roll = long_roll();
    roll.handle_input(InputEvent::Up { select: false });
    assert_eq!(roll.offset, 0);
    roll.handle_input(InputEvent::End { select: false });
    assert_eq!(roll.offset, 40 - VISIBLE);
    roll.handle_input(InputEvent::PageDown { select: false });
    assert_eq!(roll.offset, 40 - VISIBLE);
    assert_eq!(roll.percent(), 100);
}

#[test]
fn a_roll_shorter_than_the_screen_does_not_move() {
    let mut roll = Roll::new(vec![RollLine::Blank; 3]);
    skip_intro(&mut roll);
    for _ in 0..LINE_TICKS * 2 {
        roll.tick();
    }
    assert_eq!(roll.offset, 0);
}
