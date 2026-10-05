//! Host-independent input events for Glaux system software.
//!
//! Hosts translate their native keyboard handling into these events; the
//! system never sees host key codes or keyboard state.

/// A semantic input event, either for text editing or for a game button.
///
/// `select` is set when the movement should extend the current selection
/// (typically Shift held) instead of collapsing it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputEvent {
    /// A character typed by the user.
    Text(char),
    Left {
        select: bool,
    },
    Right {
        select: bool,
    },
    /// Move to the start of the line.
    Home {
        select: bool,
    },
    /// Move to the end of the line.
    End {
        select: bool,
    },
    Backspace,
    Delete,
    Enter,
    /// Step toward older entries, such as earlier commands in a history.
    Up,
    /// Step toward newer entries.
    Down,
    /// Request completion of the word being typed.
    Tab,
    /// Interrupt the running program and return to the system.
    Break,
    /// A game button went from released to pressed. Never repeated while
    /// the button stays held.
    ButtonDown(GameButton),
    /// A game button went from pressed to released.
    ButtonUp(GameButton),
}

/// Which kind of input the active system context consumes.
///
/// Hosts emit only the events of the current mode, plus
/// [`InputEvent::Break`] in both, so one physical key never acts as a game
/// button and an editing key at the same time.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum InputMode {
    /// Text and editing events, for the shell.
    #[default]
    Text,
    /// [`InputEvent::ButtonDown`] and [`InputEvent::ButtonUp`], for a
    /// running cartridge.
    Game,
}

/// A virtual Glaux game button. Cartridges see only these, never host keys.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GameButton {
    Up,
    Down,
    Left,
    Right,
    A,
    B,
}

impl GameButton {
    pub const ALL: [Self; 6] = [
        Self::Up,
        Self::Down,
        Self::Left,
        Self::Right,
        Self::A,
        Self::B,
    ];

    fn bit(self) -> u8 {
        1 << self as u8
    }
}

/// A set of [`GameButton`]s, one bit per button.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ButtonSet(u8);

impl ButtonSet {
    pub const EMPTY: Self = Self(0);

    pub fn contains(self, button: GameButton) -> bool {
        self.0 & button.bit() != 0
    }

    pub fn insert(&mut self, button: GameButton) {
        self.0 |= button.bit();
    }

    pub fn remove(&mut self, button: GameButton) {
        self.0 &= !button.bit();
    }
}

impl FromIterator<GameButton> for ButtonSet {
    fn from_iter<I: IntoIterator<Item = GameButton>>(iter: I) -> Self {
        let mut set = Self::EMPTY;
        for button in iter {
            set.insert(button);
        }
        set
    }
}

/// The game buttons as seen by one logical tick.
///
/// `held` and `pressed` are independent: a button tapped and released
/// between two ticks is pressed but not held.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ButtonState {
    /// Buttons down at the tick boundary.
    pub held: ButtonSet,
    /// Buttons that went from released to pressed since the previous tick.
    pub pressed: ButtonSet,
}

/// Folds button events into one [`ButtonState`] per logical tick.
///
/// A new latch has every button released.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ButtonLatch {
    held: ButtonSet,
}

impl ButtonLatch {
    pub fn new() -> Self {
        Self::default()
    }

    /// Applies the button events received since the previous tick, in
    /// order, and returns the state for this tick. Other events are
    /// ignored.
    ///
    /// A `ButtonDown` for a button that is already held is not a new press,
    /// and a `ButtonUp` for one that is not held changes nothing.
    pub fn advance(&mut self, events: &[InputEvent]) -> ButtonState {
        let mut pressed = ButtonSet::EMPTY;
        for event in events {
            match *event {
                InputEvent::ButtonDown(button) if !self.held.contains(button) => {
                    self.held.insert(button);
                    pressed.insert(button);
                }
                InputEvent::ButtonUp(button) => self.held.remove(button),
                _ => {}
            }
        }
        ButtonState {
            held: self.held,
            pressed,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use GameButton::*;
    use InputEvent::{ButtonDown, ButtonUp};

    fn set(buttons: &[GameButton]) -> ButtonSet {
        buttons.iter().copied().collect()
    }

    fn state(held: &[GameButton], pressed: &[GameButton]) -> ButtonState {
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
        for button in GameButton::ALL {
            assert_eq!(buttons.contains(button), matches!(button, Left | B));
        }
        buttons.remove(Left);
        assert_eq!(buttons, set(&[B]));
    }

    #[test]
    fn press_hold_release_and_press_again() {
        let mut latch = ButtonLatch::new();
        assert_eq!(latch.advance(&[ButtonDown(Left)]), state(&[Left], &[Left]));
        assert_eq!(latch.advance(&[]), state(&[Left], &[]));
        assert_eq!(latch.advance(&[ButtonUp(Left)]), state(&[], &[]));
        assert_eq!(latch.advance(&[]), state(&[], &[]));
        assert_eq!(latch.advance(&[ButtonDown(Left)]), state(&[Left], &[Left]));
    }

    #[test]
    fn unrelated_buttons_stay_released() {
        let mut latch = ButtonLatch::new();
        let tick = latch.advance(&[ButtonDown(A)]);
        for button in GameButton::ALL.into_iter().filter(|&b| b != A) {
            assert!(!tick.held.contains(button), "{button:?}");
            assert!(!tick.pressed.contains(button), "{button:?}");
        }
    }

    #[test]
    fn a_tap_between_ticks_is_pressed_but_not_held() {
        let mut latch = ButtonLatch::new();
        assert_eq!(
            latch.advance(&[ButtonDown(Up), ButtonUp(Up)]),
            state(&[], &[Up])
        );
        assert_eq!(latch.advance(&[]), state(&[], &[]));
    }

    #[test]
    fn several_host_frames_before_a_tick_press_once() {
        // A press in the first of three host frames, delivered to one tick.
        let mut latch = ButtonLatch::new();
        let frames = [ButtonDown(Down), ButtonDown(Down), ButtonDown(Down)];
        assert_eq!(latch.advance(&frames), state(&[Down], &[Down]));
    }

    #[test]
    fn catch_up_ticks_do_not_repeat_the_press() {
        let mut latch = ButtonLatch::new();
        assert_eq!(latch.advance(&[ButtonDown(B)]), state(&[B], &[B]));
        for _ in 0..3 {
            assert_eq!(latch.advance(&[]), state(&[B], &[]));
        }
    }

    #[test]
    fn repeated_down_while_held_is_not_a_press() {
        let mut latch = ButtonLatch::new();
        latch.advance(&[ButtonDown(Right)]);
        assert_eq!(latch.advance(&[ButtonDown(Right)]), state(&[Right], &[]));
    }

    #[test]
    fn release_and_press_within_a_tick_is_a_new_press() {
        let mut latch = ButtonLatch::new();
        latch.advance(&[ButtonDown(A)]);
        assert_eq!(
            latch.advance(&[ButtonUp(A), ButtonDown(A)]),
            state(&[A], &[A])
        );
    }

    #[test]
    fn up_without_down_and_other_events_are_ignored() {
        let mut latch = ButtonLatch::new();
        assert_eq!(
            latch.advance(&[ButtonUp(A), InputEvent::Up, InputEvent::Break]),
            state(&[], &[])
        );
    }
}
