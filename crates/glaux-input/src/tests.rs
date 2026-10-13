use super::*;
use Button::*;
use InputEvent::{ButtonDown, ButtonUp, KeyDown, KeyUp};
use proptest::prelude::*;

fn set(buttons: &[Button]) -> ButtonSet {
    buttons.iter().copied().collect()
}

fn state(held: &[Button], pressed: &[Button]) -> ButtonState {
    ButtonState {
        held: set(held),
        pressed: set(pressed),
    }
}

#[test]
fn button_set_tracks_each_button_separately() {
    let mut buttons = ButtonSet::EMPTY;
    buttons.insert(Left);
    buttons.insert(B);
    for button in Button::ALL {
        assert_eq!(buttons.contains(button), matches!(button, Left | B));
    }
    buttons.remove(Left);
    assert_eq!(buttons, set(&[B]));
}

#[test]
fn press_hold_release_and_press_again() {
    let mut latch = InputLatch::new();
    assert_eq!(
        latch.advance(&[ButtonDown(Left)]).buttons,
        state(&[Left], &[Left])
    );
    assert_eq!(latch.advance(&[]).buttons, state(&[Left], &[]));
    assert_eq!(latch.advance(&[ButtonUp(Left)]).buttons, state(&[], &[]));
    assert_eq!(latch.advance(&[]).buttons, state(&[], &[]));
    assert_eq!(
        latch.advance(&[ButtonDown(Left)]).buttons,
        state(&[Left], &[Left])
    );
}

#[test]
fn a_tap_between_ticks_is_pressed_but_not_held() {
    let mut latch = InputLatch::new();
    assert_eq!(
        latch.advance(&[ButtonDown(Up), ButtonUp(Up)]).buttons,
        state(&[], &[Up])
    );
    assert_eq!(latch.advance(&[]).buttons, state(&[], &[]));
}

#[test]
fn several_host_frames_before_a_tick_press_once() {
    // A press in the first of three host frames, delivered to one tick.
    let mut latch = InputLatch::new();
    let frames = [ButtonDown(Down), ButtonDown(Down), ButtonDown(Down)];
    assert_eq!(latch.advance(&frames).buttons, state(&[Down], &[Down]));
}

#[test]
fn repeated_down_while_held_is_not_a_press() {
    let mut latch = InputLatch::new();
    latch.advance(&[ButtonDown(Right)]);
    assert_eq!(
        latch.advance(&[ButtonDown(Right)]).buttons,
        state(&[Right], &[])
    );
}

#[test]
fn release_and_press_within_a_tick_is_a_new_press() {
    let mut latch = InputLatch::new();
    latch.advance(&[ButtonDown(A)]);
    assert_eq!(
        latch.advance(&[ButtonUp(A), ButtonDown(A)]).buttons,
        state(&[A], &[A])
    );
}

#[test]
fn up_without_down_and_non_game_events_never_reach_the_latch() {
    let mut latch = InputLatch::new();
    let at = Point::new(10, 20);
    let tick = latch.advance(&[
        ButtonUp(A),
        KeyUp(Key::Z),
        InputEvent::Up { select: false },
        InputEvent::Break,
        InputEvent::PointerMove(Some(at)),
        InputEvent::PointerDown {
            button: PointerButton::Primary,
            at,
            select: false,
        },
        InputEvent::Wheel { delta: 1 },
        InputEvent::NextView,
        InputEvent::PointerUp {
            button: PointerButton::Primary,
            at,
        },
    ]);
    assert_eq!(tick, InputState::default());
}

fn keys(held: &[Key], pressed: &[Key]) -> KeyState {
    KeyState {
        held: held.iter().copied().collect(),
        pressed: pressed.iter().copied().collect(),
    }
}

#[test]
fn key_set_tracks_every_key_separately() {
    // The last keys sit in the high bits of the 128-bit set.
    for &key in Key::ALL {
        let set: KeySet = [key].into_iter().collect();
        for &other in Key::ALL {
            assert_eq!(set.contains(other), other == key, "{key:?} {other:?}");
        }
    }
}

#[test]
fn key_press_hold_release_and_press_again() {
    let mut latch = InputLatch::new();
    use Key::F2;
    assert_eq!(latch.advance(&[KeyDown(F2)]).keys, keys(&[F2], &[F2]));
    assert_eq!(latch.advance(&[]).keys, keys(&[F2], &[]));
    assert_eq!(latch.advance(&[KeyUp(F2)]).keys, keys(&[], &[]));
    assert_eq!(latch.advance(&[KeyDown(F2)]).keys, keys(&[F2], &[F2]));
}

#[test]
fn repeated_key_down_while_held_is_not_a_press() {
    let mut latch = InputLatch::new();
    latch.advance(&[KeyDown(Key::Insert)]);
    let repeats = [KeyDown(Key::Insert), KeyDown(Key::Insert)];
    assert_eq!(latch.advance(&repeats).keys, keys(&[Key::Insert], &[]));
}

#[test]
fn a_key_tap_between_ticks_is_pressed_but_not_held() {
    let mut latch = InputLatch::new();
    let tap = [KeyDown(Key::Space), KeyUp(Key::Space)];
    assert_eq!(latch.advance(&tap).keys, keys(&[], &[Key::Space]));
    assert_eq!(latch.advance(&[]).keys, keys(&[], &[]));
}

#[test]
fn simultaneous_buttons_and_keys_are_tracked_independently() {
    let mut latch = InputLatch::new();
    let tick = latch.advance(&[
        ButtonDown(A),
        KeyDown(Key::Z),
        ButtonDown(Left),
        KeyDown(Key::LeftShift),
    ]);
    assert_eq!(tick.buttons, state(&[A, Left], &[A, Left]));
    assert_eq!(
        tick.keys,
        keys(&[Key::Z, Key::LeftShift], &[Key::Z, Key::LeftShift])
    );

    let tick = latch.advance(&[ButtonUp(A), KeyUp(Key::LeftShift)]);
    assert_eq!(tick.buttons, state(&[Left], &[]));
    assert_eq!(tick.keys, keys(&[Key::Z], &[]));
}

#[test]
fn button_and_key_events_do_not_cross() {
    // `Button::Up` and `Key::Up` share a name but not a state.
    let mut latch = InputLatch::new();
    let tick = latch.advance(&[ButtonDown(Up)]);
    assert_eq!(tick.keys, KeyState::default());
    let tick = latch.advance(&[KeyUp(Key::Up)]);
    assert_eq!(tick.buttons, state(&[Up], &[]));
}

/// A button or key, by position in its `ALL` list, going down or up.
#[derive(Debug, Clone, Copy)]
enum Edge {
    Button(usize, bool),
    Key(usize, bool),
}

impl Edge {
    fn event(self) -> InputEvent {
        match self {
            Edge::Button(i, true) => ButtonDown(Button::ALL[i]),
            Edge::Button(i, false) => ButtonUp(Button::ALL[i]),
            Edge::Key(i, true) => KeyDown(Key::ALL[i]),
            Edge::Key(i, false) => KeyUp(Key::ALL[i]),
        }
    }
}

/// Edges on every button and on a few keys, so keys repeat often enough to
/// be held and pressed again.
fn edge() -> impl Strategy<Value = Edge> {
    prop_oneof![
        (0..Button::ALL.len(), any::<bool>()).prop_map(|(i, down)| Edge::Button(i, down)),
        (0..4usize, any::<bool>()).prop_map(|(i, down)| Edge::Key(i, down)),
    ]
}

/// Per-control `(held, pressed)` after one tick of `edges` on that control:
/// held when its last edge went down, pressed when it went down from
/// released at least once.
fn model_tick(held: &mut [bool], edges: impl Iterator<Item = (usize, bool)>) -> Vec<bool> {
    let mut pressed = vec![false; held.len()];
    for (i, down) in edges {
        pressed[i] |= down && !held[i];
        held[i] = down;
    }
    pressed
}

proptest! {
    #[test]
    fn the_latch_matches_a_model_of_holds_and_presses(
        ticks in proptest::collection::vec(proptest::collection::vec(edge(), 0..6), 1..20),
    ) {
        let mut latch = InputLatch::new();
        let mut held_buttons = [false; Button::ALL.len()];
        let mut held_keys = vec![false; Key::ALL.len()];
        for edges in ticks {
            let events: Vec<InputEvent> = edges.iter().map(|edge| edge.event()).collect();
            let tick = latch.advance(&events);

            let pressed = model_tick(
                &mut held_buttons,
                edges.iter().filter_map(|&edge| match edge {
                    Edge::Button(i, down) => Some((i, down)),
                    Edge::Key(..) => None,
                }),
            );
            let buttons = |flags: &[bool]| {
                Button::ALL.into_iter().zip(flags).filter(|(_, on)| **on).map(|(b, _)| b).collect()
            };
            prop_assert_eq!(
                tick.buttons,
                ButtonState { held: buttons(&held_buttons), pressed: buttons(&pressed) }
            );

            let pressed = model_tick(
                &mut held_keys,
                edges.iter().filter_map(|&edge| match edge {
                    Edge::Key(i, down) => Some((i, down)),
                    Edge::Button(..) => None,
                }),
            );
            let keys = |flags: &[bool]| {
                Key::ALL.iter().zip(flags).filter(|(_, on)| **on).map(|(k, _)| *k).collect()
            };
            prop_assert_eq!(
                tick.keys,
                KeyState { held: keys(&held_keys), pressed: keys(&pressed) }
            );
        }
    }
}
