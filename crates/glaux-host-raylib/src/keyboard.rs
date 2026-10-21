//! Translation of raylib keyboard state into Glaux input events.
//!
//! Auto-repeat is timed here instead of taken from the platform. We can't
//! rely on raylib's `IsKeyPressedRepeat` because it never fires under
//! Wayland. GLFW reports each repeat there as another press of a key that
//! is already down, and raylib ignores it as both a press and a repeat (see
//! <https://github.com/raysan5/raylib/issues/3984>). Typed characters don't
//! repeat reliably either. Measuring how long a key has been held works
//! the same on every platform.
//!
//! Game buttons and physical keys are level transitions rather than
//! repeating events, and only reach the system while it is in
//! [`InputMode::Game`].

use std::time::Duration;

use glaux_input::{Button, InputEvent, InputMode, Key};
use raylib::consts::KeyboardKey;

/// How long an editing key must be held before it starts repeating.
const REPEAT_DELAY: Duration = Duration::from_millis(500);
/// Time between repeats once a held key is repeating.
const REPEAT_INTERVAL: Duration = Duration::from_millis(33);

/// Modifier keys held while an editing key fires.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct Modifiers {
    pub(crate) shift: bool,
    pub(crate) ctrl: bool,
    pub(crate) alt: bool,
}

/// Builds the event for a key, given the modifiers held.
type KeyEvent = fn(Modifiers) -> InputEvent;

/// An editing key, the event it produces and whether holding it repeats.
struct EditKey {
    key: KeyboardKey,
    event: KeyEvent,
    repeats: bool,
    /// The key is a [`SHORTCUTS`] key while Ctrl is held, so it produces
    /// nothing here then.
    plain_only: bool,
}

const EDIT_KEYS: [EditKey; 21] = [
    EditKey {
        key: KeyboardKey::KEY_LEFT,
        event: |m| {
            let select = m.shift;
            if m.alt && !m.ctrl {
                InputEvent::Back
            } else if m.ctrl {
                InputEvent::WordLeft { select }
            } else {
                InputEvent::Left { select }
            }
        },
        repeats: true,
        plain_only: false,
    },
    EditKey {
        key: KeyboardKey::KEY_RIGHT,
        event: |m| {
            let select = m.shift;
            if m.alt && !m.ctrl {
                InputEvent::Forward
            } else if m.ctrl {
                InputEvent::WordRight { select }
            } else {
                InputEvent::Right { select }
            }
        },
        repeats: true,
        plain_only: false,
    },
    EditKey {
        key: KeyboardKey::KEY_HOME,
        event: |m| {
            let select = m.shift;
            if m.ctrl {
                InputEvent::DocumentStart { select }
            } else {
                InputEvent::Home { select }
            }
        },
        repeats: true,
        plain_only: false,
    },
    EditKey {
        key: KeyboardKey::KEY_END,
        event: |m| {
            let select = m.shift;
            if m.ctrl {
                InputEvent::DocumentEnd { select }
            } else {
                InputEvent::End { select }
            }
        },
        repeats: true,
        plain_only: false,
    },
    EditKey {
        key: KeyboardKey::KEY_PAGE_UP,
        event: |m| InputEvent::PageUp { select: m.shift },
        repeats: true,
        plain_only: true,
    },
    EditKey {
        key: KeyboardKey::KEY_PAGE_DOWN,
        event: |m| InputEvent::PageDown { select: m.shift },
        repeats: true,
        plain_only: true,
    },
    EditKey {
        key: KeyboardKey::KEY_BACKSPACE,
        event: |_| InputEvent::Backspace,
        repeats: true,
        plain_only: false,
    },
    EditKey {
        key: KeyboardKey::KEY_DELETE,
        event: |_| InputEvent::Delete,
        repeats: true,
        plain_only: false,
    },
    // Enter does not repeat so that holding it submits a line only once.
    EditKey {
        key: KeyboardKey::KEY_ENTER,
        event: |_| InputEvent::Enter,
        repeats: false,
        plain_only: false,
    },
    EditKey {
        key: KeyboardKey::KEY_KP_ENTER,
        event: |_| InputEvent::Enter,
        repeats: false,
        plain_only: false,
    },
    EditKey {
        key: KeyboardKey::KEY_UP,
        event: |m| InputEvent::Up { select: m.shift },
        repeats: true,
        plain_only: false,
    },
    EditKey {
        key: KeyboardKey::KEY_DOWN,
        event: |m| InputEvent::Down { select: m.shift },
        repeats: true,
        plain_only: false,
    },
    // Ctrl+Tab switches views, leaving plain Tab free for completion and
    // indentation.
    EditKey {
        key: KeyboardKey::KEY_TAB,
        event: |m| match (m.ctrl, m.shift) {
            (false, reverse) => InputEvent::Tab { reverse },
            (true, false) => InputEvent::NextView,
            (true, true) => InputEvent::PreviousView,
        },
        repeats: false,
        plain_only: false,
    },
    EditKey {
        key: KeyboardKey::KEY_F1,
        event: |_| InputEvent::Docs,
        repeats: false,
        plain_only: false,
    },
    EditKey {
        key: KeyboardKey::KEY_F2,
        event: |_| InputEvent::Function(2),
        repeats: false,
        plain_only: false,
    },
    EditKey {
        key: KeyboardKey::KEY_F3,
        event: |_| InputEvent::Function(3),
        repeats: false,
        plain_only: false,
    },
    EditKey {
        key: KeyboardKey::KEY_F4,
        event: |_| InputEvent::Function(4),
        repeats: false,
        plain_only: false,
    },
    EditKey {
        key: KeyboardKey::KEY_F5,
        event: |m| {
            if m.shift {
                InputEvent::Stop
            } else {
                InputEvent::Run
            }
        },
        repeats: false,
        plain_only: false,
    },
    EditKey {
        key: KeyboardKey::KEY_F11,
        event: |_| InputEvent::GoToDefinition,
        repeats: false,
        plain_only: false,
    },
    EditKey {
        key: KeyboardKey::KEY_F12,
        event: |_| InputEvent::ToggleLogs,
        repeats: false,
        plain_only: false,
    },
    EditKey {
        key: KeyboardKey::KEY_ESCAPE,
        event: |_| InputEvent::Break,
        repeats: false,
        plain_only: false,
    },
];

/// Ctrl shortcuts for editing and document commands, each fired once per
/// press and never repeated. Raylib key codes name physical positions, so on
/// layouts other than QWERTY the shortcut follows the key where the letter
/// sits on QWERTY.
const SHORTCUTS: [(KeyboardKey, KeyEvent); 20] = [
    (KeyboardKey::KEY_Z, |m| {
        if m.shift {
            InputEvent::Redo
        } else {
            InputEvent::Undo
        }
    }),
    (KeyboardKey::KEY_Y, |_| InputEvent::Redo),
    (KeyboardKey::KEY_S, |_| InputEvent::Save),
    (KeyboardKey::KEY_A, |_| InputEvent::SelectAll),
    (KeyboardKey::KEY_P, |_| InputEvent::OpenFile),
    (KeyboardKey::KEY_W, |_| InputEvent::CloseDocument),
    (KeyboardKey::KEY_PAGE_DOWN, |_| InputEvent::NextDocument),
    (KeyboardKey::KEY_PAGE_UP, |_| InputEvent::PreviousDocument),
    (KeyboardKey::KEY_N, |_| InputEvent::NewFile),
    (KeyboardKey::KEY_F, |_| InputEvent::Find),
    (KeyboardKey::KEY_H, |_| InputEvent::Replace),
    (KeyboardKey::KEY_G, |_| InputEvent::GoToLine),
    (KeyboardKey::KEY_B, |_| InputEvent::ToggleSidebar),
    (KeyboardKey::KEY_O, |_| InputEvent::GoToSymbol),
    (KeyboardKey::KEY_SPACE, |m| {
        if m.shift {
            InputEvent::SignatureHelp
        } else {
            InputEvent::Complete
        }
    }),
    (KeyboardKey::KEY_R, |m| {
        if m.shift {
            InputEvent::Restart
        } else {
            InputEvent::Run
        }
    }),
    (KeyboardKey::KEY_PERIOD, |_| InputEvent::Stop),
    (KeyboardKey::KEY_C, |_| InputEvent::Copy),
    (KeyboardKey::KEY_X, |_| InputEvent::Cut),
    (KeyboardKey::KEY_V, |_| InputEvent::Paste),
];

/// Appends the command events of the shortcuts pressed this frame to `out`.
/// Shortcuts need Ctrl. Without it the same keys only type.
pub(crate) fn shortcut_events(
    pressed: impl Fn(KeyboardKey) -> bool,
    modifiers: Modifiers,
    out: &mut Vec<InputEvent>,
) {
    if !modifiers.ctrl {
        return;
    }
    for (key, event) in SHORTCUTS {
        if pressed(key) {
            out.push(event(modifiers));
        }
    }
}

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
        modifiers: Modifiers,
        elapsed: Duration,
        out: &mut Vec<InputEvent>,
    ) {
        for (edit, held) in EDIT_KEYS.iter().zip(&mut self.held) {
            let key = state(edit.key);
            if edit.plain_only && modifiers.ctrl {
                // A hold that outlasts Ctrl starts over, so the shortcut
                // never turns into a burst of plain repeats.
                *held = (edit.repeats && key.down).then_some(Hold::default());
                continue;
            }
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
                out.push((edit.event)(modifiers));
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
        // systems. It starts a new one only if it typed something.
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
        // the platform's own repeats. Drop them so ours are the only ones.
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

/// The default desktop binding for each virtual game button: the arrow keys
/// drive the D-pad, Z is A and X is B. Cartridges only see the virtual
/// buttons, so player-facing text that names physical keys depends on this
/// table. Escape is the break key and is deliberately not a game button.
const GAME_KEYS: [(KeyboardKey, Button); 6] = [
    (KeyboardKey::KEY_UP, Button::Up),
    (KeyboardKey::KEY_DOWN, Button::Down),
    (KeyboardKey::KEY_LEFT, Button::Left),
    (KeyboardKey::KEY_RIGHT, Button::Right),
    (KeyboardKey::KEY_Z, Button::A),
    (KeyboardKey::KEY_X, Button::B),
];

/// The raylib key behind each Glaux [`Key`], one entry per key. Escape is
/// missing on purpose, and keypad Enter is not [`Key::Enter`]: with one
/// physical key per [`Key`], releasing either Enter cannot release the other.
const KEY_CODES: [(KeyboardKey, Key); Key::ALL.len()] = {
    use KeyboardKey::*;
    [
        (KEY_A, Key::A),
        (KEY_B, Key::B),
        (KEY_C, Key::C),
        (KEY_D, Key::D),
        (KEY_E, Key::E),
        (KEY_F, Key::F),
        (KEY_G, Key::G),
        (KEY_H, Key::H),
        (KEY_I, Key::I),
        (KEY_J, Key::J),
        (KEY_K, Key::K),
        (KEY_L, Key::L),
        (KEY_M, Key::M),
        (KEY_N, Key::N),
        (KEY_O, Key::O),
        (KEY_P, Key::P),
        (KEY_Q, Key::Q),
        (KEY_R, Key::R),
        (KEY_S, Key::S),
        (KEY_T, Key::T),
        (KEY_U, Key::U),
        (KEY_V, Key::V),
        (KEY_W, Key::W),
        (KEY_X, Key::X),
        (KEY_Y, Key::Y),
        (KEY_Z, Key::Z),
        (KEY_ZERO, Key::Num0),
        (KEY_ONE, Key::Num1),
        (KEY_TWO, Key::Num2),
        (KEY_THREE, Key::Num3),
        (KEY_FOUR, Key::Num4),
        (KEY_FIVE, Key::Num5),
        (KEY_SIX, Key::Num6),
        (KEY_SEVEN, Key::Num7),
        (KEY_EIGHT, Key::Num8),
        (KEY_NINE, Key::Num9),
        (KEY_F1, Key::F1),
        (KEY_F2, Key::F2),
        (KEY_F3, Key::F3),
        (KEY_F4, Key::F4),
        (KEY_F5, Key::F5),
        (KEY_F6, Key::F6),
        (KEY_F7, Key::F7),
        (KEY_F8, Key::F8),
        (KEY_F9, Key::F9),
        (KEY_F10, Key::F10),
        (KEY_F11, Key::F11),
        (KEY_F12, Key::F12),
        (KEY_SPACE, Key::Space),
        (KEY_ENTER, Key::Enter),
        (KEY_TAB, Key::Tab),
        (KEY_BACKSPACE, Key::Backspace),
        (KEY_INSERT, Key::Insert),
        (KEY_DELETE, Key::Delete),
        (KEY_HOME, Key::Home),
        (KEY_END, Key::End),
        (KEY_PAGE_UP, Key::PageUp),
        (KEY_PAGE_DOWN, Key::PageDown),
        (KEY_LEFT_SHIFT, Key::LeftShift),
        (KEY_RIGHT_SHIFT, Key::RightShift),
        (KEY_LEFT_CONTROL, Key::LeftControl),
        (KEY_RIGHT_CONTROL, Key::RightControl),
        (KEY_LEFT_ALT, Key::LeftAlt),
        (KEY_RIGHT_ALT, Key::RightAlt),
        (KEY_UP, Key::Up),
        (KEY_DOWN, Key::Down),
        (KEY_LEFT, Key::Left),
        (KEY_RIGHT, Key::Right),
    ]
};

/// Level transitions for one table of raylib keys, tracked by table
/// position so buttons and keys share the logic.
#[derive(Debug, Default)]
struct Transitions {
    /// Bit `i` is set while the key of entry `i` was down at the end of the
    /// previous frame.
    held: u128,
}

impl Transitions {
    /// Appends a `down` event for each entry whose key went down and an
    /// `up` event for each whose key came up. See [`GameKeys::events`].
    fn events<T: Copy>(
        &mut self,
        table: &[(KeyboardKey, T)],
        down: fn(T) -> InputEvent,
        up: fn(T) -> InputEvent,
        queued: &[KeyboardKey],
        is_down: &impl Fn(KeyboardKey) -> bool,
        out: &mut Vec<InputEvent>,
    ) {
        for (index, &(key, value)) in table.iter().enumerate() {
            let bit = 1u128 << index;
            let now = is_down(key);
            if self.held & bit == 0 && (now || queued.contains(&key)) {
                self.held |= bit;
                out.push(down(value));
            }
            if self.held & bit != 0 && !now {
                self.held &= !bit;
                out.push(up(value));
            }
        }
    }
}

/// Turns physical key state into game button and key transitions.
#[derive(Debug, Default)]
pub(crate) struct GameKeys {
    buttons: Transitions,
    keys: Transitions,
}

impl GameKeys {
    /// Appends this frame's button transitions, then its key transitions,
    /// to `out`.
    ///
    /// `queued` are the keys raylib reported going down this frame and
    /// `is_down` reports whether a key is held now. A key that went down and
    /// up within one frame appears only in `queued`, so it yields a down
    /// event immediately followed by an up event. A held key never produces
    /// another down event, even if the platform reports repeats.
    pub(crate) fn events(
        &mut self,
        queued: &[KeyboardKey],
        is_down: impl Fn(KeyboardKey) -> bool,
        out: &mut Vec<InputEvent>,
    ) {
        self.buttons.events(
            &GAME_KEYS,
            InputEvent::ButtonDown,
            InputEvent::ButtonUp,
            queued,
            &is_down,
            out,
        );
        self.keys.events(
            &KEY_CODES,
            InputEvent::KeyDown,
            InputEvent::KeyUp,
            queued,
            &is_down,
            out,
        );
    }
}

/// Drops the events `mode` does not consume. [`InputEvent::Break`] is kept
/// in every mode, and [`InputEvent::ToggleLogs`] while a cartridge runs or
/// the workspace is open. The F12 key never reaches a cartridge. View switching, editing and document commands and pointer
/// events are kept only in [`InputMode::Workspace`].
pub(crate) fn retain_for_mode(mode: InputMode, events: &mut Vec<InputEvent>) {
    events.retain(|event| {
        let game = matches!(
            event,
            InputEvent::ButtonDown(_)
                | InputEvent::ButtonUp(_)
                | InputEvent::KeyDown(_)
                | InputEvent::KeyUp(_)
        );
        let workspace = event.is_pointer()
            || matches!(
                event,
                InputEvent::NextView
                    | InputEvent::PreviousView
                    | InputEvent::Undo
                    | InputEvent::Redo
                    | InputEvent::Save
                    | InputEvent::SelectAll
                    | InputEvent::OpenFile
                    | InputEvent::CloseDocument
                    | InputEvent::NextDocument
                    | InputEvent::PreviousDocument
                    | InputEvent::NewFile
                    | InputEvent::Find
                    | InputEvent::Replace
                    | InputEvent::GoToLine
                    | InputEvent::Complete
                    | InputEvent::SignatureHelp
                    | InputEvent::Docs
                    | InputEvent::Run
                    | InputEvent::Stop
                    | InputEvent::Restart
                    | InputEvent::Function(_)
                    | InputEvent::Back
                    | InputEvent::Forward
                    | InputEvent::GoToDefinition
                    | InputEvent::GoToSymbol
                    | InputEvent::ToggleSidebar
                    | InputEvent::Copy
                    | InputEvent::Cut
                    | InputEvent::Paste
            );
        // F12 belongs to the system's log panel. A running cartridge never
        // sees it as a key, so it cannot use it as a game control.
        let reserved = matches!(
            event,
            InputEvent::KeyDown(Key::F12) | InputEvent::KeyUp(Key::F12)
        );
        let logs = *event == InputEvent::ToggleLogs;
        match mode {
            InputMode::Text => !game && !workspace && !logs,
            InputMode::Game => (game && !reserved) || matches!(event, InputEvent::Break) || logs,
            InputMode::Workspace => !game,
        }
    });
}

/// Number of repeats that are due after a key has been held for `held`.
fn repeats_due(held: Duration) -> u128 {
    match held.checked_sub(REPEAT_DELAY) {
        Some(past_delay) => past_delay.as_nanos() / REPEAT_INTERVAL.as_nanos() + 1,
        None => 0,
    }
}

#[cfg(test)]
mod tests;
