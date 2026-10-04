//! Translation of raylib keyboard state into Glaux input events.
//!
//! Auto-repeat is timed here instead of taken from the platform. We can't
//! rely on raylib's `IsKeyPressedRepeat` because it never fires under
//! Wayland. GLFW reports each repeat there as another press of a key that
//! is already down, and raylib ignores it as both a press and a repeat (see
//! <https://github.com/raysan5/raylib/issues/3984>). Typed characters don't
//! repeat reliably either. Measuring how long a key has been held works
//! the same on every platform.

use std::time::Duration;

use glaux_input::InputEvent;
use raylib::consts::KeyboardKey;

/// How long an editing key must be held before it starts repeating.
const REPEAT_DELAY: Duration = Duration::from_millis(500);
/// Time between repeats once a held key is repeating.
const REPEAT_INTERVAL: Duration = Duration::from_millis(33);

/// Builds the event for a key, given whether Shift is held.
type KeyEvent = fn(bool) -> InputEvent;

/// An editing key, the event it produces and whether holding it repeats.
struct EditKey {
    key: KeyboardKey,
    event: KeyEvent,
    repeats: bool,
}

const EDIT_KEYS: [EditKey; 8] = [
    EditKey {
        key: KeyboardKey::KEY_LEFT,
        event: |select| InputEvent::Left { select },
        repeats: true,
    },
    EditKey {
        key: KeyboardKey::KEY_RIGHT,
        event: |select| InputEvent::Right { select },
        repeats: true,
    },
    EditKey {
        key: KeyboardKey::KEY_HOME,
        event: |select| InputEvent::Home { select },
        repeats: true,
    },
    EditKey {
        key: KeyboardKey::KEY_END,
        event: |select| InputEvent::End { select },
        repeats: true,
    },
    EditKey {
        key: KeyboardKey::KEY_BACKSPACE,
        event: |_| InputEvent::Backspace,
        repeats: true,
    },
    EditKey {
        key: KeyboardKey::KEY_DELETE,
        event: |_| InputEvent::Delete,
        repeats: true,
    },
    // Enter does not repeat so that holding it submits a line only once.
    EditKey {
        key: KeyboardKey::KEY_ENTER,
        event: |_| InputEvent::Enter,
        repeats: false,
    },
    EditKey {
        key: KeyboardKey::KEY_KP_ENTER,
        event: |_| InputEvent::Enter,
        repeats: false,
    },
];

/// One key's state for the current frame.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct KeyState {
    /// The key went down this frame.
    pub(crate) pressed: bool,
    /// The key is currently held.
    pub(crate) down: bool,
}

/// Modifier keys, which never produce text on their own.
const MODIFIER_KEYS: [KeyboardKey; 8] = [
    KeyboardKey::KEY_LEFT_SHIFT,
    KeyboardKey::KEY_RIGHT_SHIFT,
    KeyboardKey::KEY_LEFT_CONTROL,
    KeyboardKey::KEY_RIGHT_CONTROL,
    KeyboardKey::KEY_LEFT_ALT,
    KeyboardKey::KEY_RIGHT_ALT,
    KeyboardKey::KEY_LEFT_SUPER,
    KeyboardKey::KEY_RIGHT_SUPER,
];

/// How long a key has been held, for timing its repeats.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct Hold(Duration);

impl Hold {
    /// Adds `elapsed` to the hold time and returns whether a repeat became
    /// due. At most one repeat is reported per call, so a stalled frame
    /// does not release a burst of edits.
    fn advance(&mut self, elapsed: Duration) -> bool {
        let before = self.0;
        self.0 = before.saturating_add(elapsed);
        repeats_due(self.0) > repeats_due(before)
    }
}

/// Turns per-frame key state into editing events, repeating held keys.
#[derive(Debug, Default)]
pub(crate) struct EditKeys {
    /// The hold of each repeating key in [`EDIT_KEYS`], or `None` when it
    /// is up.
    held: [Option<Hold>; EDIT_KEYS.len()],
}

impl EditKeys {
    /// Appends the events for this frame to `out`.
    ///
    /// `elapsed` is the time since the previous call. A key emits an event
    /// when pressed, then, while held, once [`REPEAT_DELAY`] has passed and
    /// every [`REPEAT_INTERVAL`] after that.
    pub(crate) fn events(
        &mut self,
        state: impl Fn(KeyboardKey) -> KeyState,
        shift: bool,
        elapsed: Duration,
        out: &mut Vec<InputEvent>,
    ) {
        for (edit, held) in EDIT_KEYS.iter().zip(&mut self.held) {
            let key = state(edit.key);
            let fire = if key.pressed {
                *held = edit.repeats.then_some(Hold::default());
                true
            } else if let Some(hold) = held.as_mut().filter(|_| key.down) {
                hold.advance(elapsed)
            } else {
                *held = None;
                false
            };
            if fire {
                out.push((edit.event)(shift));
            }
        }
    }
}

/// A typed character and the key whose hold repeats it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct HeldText {
    key: KeyboardKey,
    ch: char,
    hold: Hold,
}

/// Turns typed characters into text events, repeating the last one while
/// the key that typed it stays down.
///
/// Raylib reports characters and keys in separate queues, so a character
/// is tied to the non-modifier key that went down in the same frame. The
/// repeated character is the one first typed, so releasing Shift mid-hold
/// keeps repeating the uppercase letter. Keys raylib can't identify still
/// type, but don't repeat.
#[derive(Debug, Default)]
pub(crate) struct TextKeys {
    held: Option<HeldText>,
}

impl TextKeys {
    /// Appends text events for this frame to `out`.
    ///
    /// `chars` are the characters typed this frame, `pressed` the keys that
    /// went down this frame, `is_down` reports whether a key is held, and
    /// `elapsed` is the time since the previous call.
    pub(crate) fn events(
        &mut self,
        chars: &[char],
        pressed: &[KeyboardKey],
        is_down: impl Fn(KeyboardKey) -> bool,
        elapsed: Duration,
        out: &mut Vec<InputEvent>,
    ) {
        // Any new non-modifier key ends the current repeat, as on desktop
        // systems; it starts a new one only if it typed something.
        let new_key = pressed
            .iter()
            .rev()
            .copied()
            .find(|key| !MODIFIER_KEYS.contains(key));
        if let Some(key) = new_key {
            out.extend(chars.iter().copied().map(InputEvent::Text));
            let types = !EDIT_KEYS.iter().any(|edit| edit.key == key);
            self.held = chars.last().filter(|_| types).map(|&ch| HeldText {
                key,
                ch,
                hold: Hold::default(),
            });
            return;
        }

        let Some(held) = self.held.as_mut().filter(|held| is_down(held.key)) else {
            self.held = None;
            out.extend(chars.iter().copied().map(InputEvent::Text));
            return;
        };
        // No key went down this frame, so copies of the held character are
        // the platform's own repeats; drop them so ours are the only ones.
        let ch = held.ch;
        out.extend(
            chars
                .iter()
                .copied()
                .filter(|&c| c != ch)
                .map(InputEvent::Text),
        );
        if held.hold.advance(elapsed) {
            out.push(InputEvent::Text(ch));
        }
    }
}

/// Number of repeats that are due after a key has been held for `held`.
fn repeats_due(held: Duration) -> u128 {
    match held.checked_sub(REPEAT_DELAY) {
        Some(past_delay) => past_delay.as_nanos() / REPEAT_INTERVAL.as_nanos() + 1,
        None => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use KeyboardKey::*;
    use pretty_assertions::assert_eq;

    const FRAME: Duration = Duration::from_millis(16);
    const PRESSED: KeyState = KeyState {
        pressed: true,
        down: true,
    };
    const HELD: KeyState = KeyState {
        pressed: false,
        down: true,
    };

    /// Events for one frame in which every key in `keys` has `state` and
    /// every other key is up.
    fn frame(
        edit: &mut EditKeys,
        keys: &[KeyboardKey],
        state: KeyState,
        shift: bool,
        elapsed: Duration,
    ) -> Vec<InputEvent> {
        let mut out = Vec::new();
        edit.events(
            |key| {
                if keys.contains(&key) {
                    state
                } else {
                    KeyState::default()
                }
            },
            shift,
            elapsed,
            &mut out,
        );
        out
    }

    fn press(keys: &[KeyboardKey], shift: bool) -> Vec<InputEvent> {
        frame(&mut EditKeys::default(), keys, PRESSED, shift, FRAME)
    }

    /// Presses `key`, holds it for `frames` frames of `FRAME` each, and
    /// returns how many events each frame produced, press frame first.
    fn hold(key: KeyboardKey, frames: usize) -> Vec<usize> {
        let mut edit = EditKeys::default();
        let mut counts = vec![frame(&mut edit, &[key], PRESSED, false, FRAME).len()];
        for _ in 0..frames {
            counts.push(frame(&mut edit, &[key], HELD, false, FRAME).len());
        }
        counts
    }

    #[test]
    fn no_keys_produce_no_events() {
        assert_eq!(press(&[], false), []);
        assert_eq!(press(&[], true), []);
    }

    #[test]
    fn movement_keys_select_only_with_shift() {
        let keys = [KEY_LEFT, KEY_RIGHT, KEY_HOME, KEY_END];
        assert_eq!(
            press(&keys, false),
            [
                InputEvent::Left { select: false },
                InputEvent::Right { select: false },
                InputEvent::Home { select: false },
                InputEvent::End { select: false },
            ]
        );
        assert_eq!(
            press(&keys, true),
            [
                InputEvent::Left { select: true },
                InputEvent::Right { select: true },
                InputEvent::Home { select: true },
                InputEvent::End { select: true },
            ]
        );
    }

    #[test]
    fn editing_keys_ignore_shift() {
        let keys = [KEY_BACKSPACE, KEY_DELETE, KEY_ENTER];
        let expected = [InputEvent::Backspace, InputEvent::Delete, InputEvent::Enter];
        assert_eq!(press(&keys, false), expected);
        assert_eq!(press(&keys, true), expected);
    }

    #[test]
    fn keypad_enter_is_enter() {
        assert_eq!(press(&[KEY_KP_ENTER], false), [InputEvent::Enter]);
    }

    #[test]
    fn unrelated_keys_are_ignored() {
        assert_eq!(press(&[KEY_A, KEY_LEFT_SHIFT], true), []);
    }

    #[test]
    fn held_key_without_press_does_not_fire() {
        let mut edit = EditKeys::default();
        let long = REPEAT_DELAY * 2;
        assert_eq!(frame(&mut edit, &[KEY_BACKSPACE], HELD, false, long), []);
    }

    #[test]
    fn held_key_waits_for_delay_then_repeats_at_interval() {
        let mut edit = EditKeys::default();
        let key = &[KEY_BACKSPACE];
        assert_eq!(frame(&mut edit, key, PRESSED, false, FRAME).len(), 1);

        let just_before = REPEAT_DELAY - Duration::from_millis(1);
        assert_eq!(frame(&mut edit, key, HELD, false, just_before), []);
        assert_eq!(
            frame(&mut edit, key, HELD, false, Duration::from_millis(1)),
            [InputEvent::Backspace]
        );

        let half = REPEAT_INTERVAL / 2;
        assert_eq!(frame(&mut edit, key, HELD, false, half), []);
        assert_eq!(
            frame(&mut edit, key, HELD, false, REPEAT_INTERVAL - half),
            [InputEvent::Backspace]
        );
    }

    #[test]
    fn repeating_keys_repeat_while_held() {
        // With a 0.5 s delay, a 33 ms interval and 16 ms frames, the first
        // repeat lands on frame 32 and then roughly every other frame.
        for key in [
            KEY_LEFT,
            KEY_RIGHT,
            KEY_HOME,
            KEY_END,
            KEY_BACKSPACE,
            KEY_DELETE,
        ] {
            let counts = hold(key, 40);
            assert_eq!(counts[0], 1, "{key:?} press");
            assert!(counts[1..32].iter().all(|&n| n == 0), "{key:?} delay");
            assert_eq!(counts[32..].iter().sum::<usize>(), 5, "{key:?} repeats");
        }
    }

    #[test]
    fn enter_does_not_repeat() {
        for key in [KEY_ENTER, KEY_KP_ENTER] {
            assert_eq!(hold(key, 120).iter().sum::<usize>(), 1, "{key:?}");
        }
    }

    #[test]
    fn repeats_use_current_shift_state() {
        let mut edit = EditKeys::default();
        frame(&mut edit, &[KEY_LEFT], PRESSED, false, FRAME);
        assert_eq!(
            frame(&mut edit, &[KEY_LEFT], HELD, true, REPEAT_DELAY),
            [InputEvent::Left { select: true }]
        );
    }

    #[test]
    fn stalled_frame_emits_a_single_repeat() {
        let mut edit = EditKeys::default();
        frame(&mut edit, &[KEY_DELETE], PRESSED, false, FRAME);
        let stall = REPEAT_DELAY + REPEAT_INTERVAL * 10;
        assert_eq!(
            frame(&mut edit, &[KEY_DELETE], HELD, false, stall),
            [InputEvent::Delete]
        );
    }

    /// Text events for one frame. Keys in `down` are held; `pressed` keys
    /// went down this frame (and are held too).
    fn text_frame(
        text: &mut TextKeys,
        chars: &str,
        pressed: &[KeyboardKey],
        down: &[KeyboardKey],
        elapsed: Duration,
    ) -> Vec<InputEvent> {
        let chars: Vec<char> = chars.chars().collect();
        let mut out = Vec::new();
        text.events(
            &chars,
            pressed,
            |key| pressed.contains(&key) || down.contains(&key),
            elapsed,
            &mut out,
        );
        out
    }

    fn texts(s: &str) -> Vec<InputEvent> {
        s.chars().map(InputEvent::Text).collect()
    }

    #[test]
    fn typed_characters_become_text_events() {
        let mut text = TextKeys::default();
        assert_eq!(
            text_frame(&mut text, "hé", &[KEY_H, KEY_E], &[], FRAME),
            texts("hé")
        );
    }

    #[test]
    fn held_character_repeats_after_delay() {
        let mut text = TextKeys::default();
        assert_eq!(text_frame(&mut text, "o", &[KEY_O], &[], FRAME), texts("o"));
        let just_before = REPEAT_DELAY - Duration::from_millis(1);
        assert_eq!(text_frame(&mut text, "", &[], &[KEY_O], just_before), []);
        assert_eq!(
            text_frame(&mut text, "", &[], &[KEY_O], Duration::from_millis(1)),
            texts("o")
        );
        assert_eq!(
            text_frame(&mut text, "", &[], &[KEY_O], REPEAT_INTERVAL),
            texts("o")
        );
    }

    #[test]
    fn shifted_character_repeats_as_typed() {
        let mut text = TextKeys::default();
        text_frame(&mut text, "O", &[KEY_LEFT_SHIFT, KEY_O], &[], FRAME);
        assert_eq!(
            text_frame(&mut text, "", &[], &[KEY_O], REPEAT_DELAY),
            texts("O")
        );
    }

    #[test]
    fn releasing_the_key_stops_the_repeat() {
        let mut text = TextKeys::default();
        text_frame(&mut text, "h", &[KEY_H], &[], FRAME);
        assert_eq!(text_frame(&mut text, "", &[], &[], REPEAT_DELAY), []);
        assert_eq!(text_frame(&mut text, "", &[], &[KEY_H], REPEAT_DELAY), []);
    }

    #[test]
    fn new_key_replaces_the_repeating_character() {
        let mut text = TextKeys::default();
        text_frame(&mut text, "a", &[KEY_A], &[], FRAME);
        text_frame(&mut text, "b", &[KEY_B], &[KEY_A], FRAME);
        assert_eq!(
            text_frame(&mut text, "", &[], &[KEY_A, KEY_B], REPEAT_DELAY),
            texts("b")
        );
    }

    #[test]
    fn platform_repeats_of_held_character_are_dropped() {
        let mut text = TextKeys::default();
        text_frame(&mut text, "o", &[KEY_O], &[], FRAME);
        assert_eq!(text_frame(&mut text, "o", &[], &[KEY_O], FRAME), []);
        assert_eq!(
            text_frame(&mut text, "oo", &[], &[KEY_O], REPEAT_DELAY),
            texts("o")
        );
    }

    #[test]
    fn characters_without_a_key_press_still_type() {
        let mut text = TextKeys::default();
        assert_eq!(text_frame(&mut text, "ñ", &[], &[], FRAME), texts("ñ"));
        assert_eq!(text_frame(&mut text, "", &[], &[], REPEAT_DELAY * 2), []);
    }

    #[test]
    fn key_press_without_text_does_not_repeat() {
        let mut text = TextKeys::default();
        text_frame(&mut text, "x", &[KEY_X], &[], FRAME);
        // Ctrl+O produces no character, and ends the previous repeat.
        assert_eq!(
            text_frame(&mut text, "", &[KEY_LEFT_CONTROL, KEY_O], &[KEY_X], FRAME),
            []
        );
        assert_eq!(
            text_frame(&mut text, "", &[], &[KEY_X, KEY_O], REPEAT_DELAY),
            []
        );
    }

    #[test]
    fn editing_key_press_ends_text_repeat() {
        let mut text = TextKeys::default();
        text_frame(&mut text, "a", &[KEY_A], &[], FRAME);
        assert_eq!(
            text_frame(&mut text, "", &[KEY_BACKSPACE], &[KEY_A], FRAME),
            []
        );
        assert_eq!(
            text_frame(&mut text, "", &[], &[KEY_A, KEY_BACKSPACE], REPEAT_DELAY),
            []
        );
    }

    #[test]
    fn modifier_press_keeps_text_repeat() {
        let mut text = TextKeys::default();
        text_frame(&mut text, "a", &[KEY_A], &[], FRAME);
        text_frame(&mut text, "", &[KEY_LEFT_SHIFT], &[KEY_A], FRAME);
        assert_eq!(
            text_frame(&mut text, "", &[], &[KEY_A, KEY_LEFT_SHIFT], REPEAT_DELAY),
            texts("a")
        );
    }

    #[test]
    fn release_resets_repeat_timer() {
        let mut edit = EditKeys::default();
        let key = &[KEY_RIGHT];
        frame(&mut edit, key, PRESSED, false, FRAME);
        frame(&mut edit, key, HELD, false, REPEAT_DELAY / 2);
        frame(&mut edit, &[], PRESSED, false, FRAME);

        // Holding the key again without a new press does nothing, because
        // the release cleared the timer.
        assert_eq!(frame(&mut edit, key, HELD, false, REPEAT_DELAY), []);

        frame(&mut edit, key, PRESSED, false, FRAME);
        assert_eq!(
            frame(&mut edit, key, HELD, false, REPEAT_DELAY / 2),
            [],
            "a new press restarts the delay"
        );
    }
}
