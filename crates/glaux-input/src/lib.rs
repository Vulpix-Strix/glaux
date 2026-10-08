//! Host-independent input events for Glaux system software.
//!
//! Hosts translate their native keyboard handling into these events; the
//! system never sees host key codes or keyboard state.
//!
//! Four kinds of input stay separate: typed text and editing events for the
//! shell, virtual [`Button`]s for portable game controls, physical [`Key`]s
//! for cartridges that need a specific key, and pointer events for the
//! system's workspace. Pointer input never reaches cartridges.

/// A semantic input event: text editing, a game button or a physical key.
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
    /// Pause the running program and return to the system, or leave the
    /// pause screen.
    Break,
    /// A game button went from released to pressed. Never repeated while
    /// the button stays held.
    ButtonDown(Button),
    /// A game button went from pressed to released.
    ButtonUp(Button),
    /// A physical key went from released to pressed. Never repeated while
    /// the key stays held, even if the platform auto-repeats it.
    KeyDown(Key),
    /// A physical key went from pressed to released.
    KeyUp(Key),
    /// Switch to the next view of the active context (Ctrl+Tab on desktop).
    NextView,
    /// Switch to the previous view of the active context (Ctrl+Shift+Tab on
    /// desktop).
    PreviousView,
    /// The logical pixel under the pointer changed. `None` means the pointer
    /// is outside the logical viewport and no button is captured.
    ///
    /// While a button is captured (see [`PointerDown`](Self::PointerDown)),
    /// the position is clamped to the viewport edge instead of becoming
    /// `None`. Hosts report at most one move per host frame, and only when
    /// the logical pixel changes.
    PointerMove(Option<Point>),
    /// A pointer button was pressed inside the logical viewport.
    ///
    /// The press captures the pointer until the matching
    /// [`PointerUp`](Self::PointerUp), which is always delivered, at the
    /// clamped position if the button is released outside the viewport.
    /// Presses that start outside the viewport, such as on the bars around
    /// it, are not reported, and neither are their releases.
    PointerDown {
        button: PointerButton,
        at: Point,
    },
    /// A captured pointer button was released.
    PointerUp {
        button: PointerButton,
        at: Point,
    },
    /// The scroll wheel moved by whole notches; positive is away from the
    /// user. Hosts report at most one wheel event per host frame.
    Wheel {
        delta: i8,
    },
}

impl InputEvent {
    /// Whether this is a pointer event, which only the workspace consumes.
    pub fn is_pointer(&self) -> bool {
        matches!(
            self,
            Self::PointerMove(_)
                | Self::PointerDown { .. }
                | Self::PointerUp { .. }
                | Self::Wheel { .. }
        )
    }
}

/// A pixel of the logical framebuffer, always inside it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct Point {
    pub x: u16,
    pub y: u16,
}

impl Point {
    pub const fn new(x: u16, y: u16) -> Self {
        Self { x, y }
    }
}

/// A pointer button. Which physical button drives each is up to the host.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PointerButton {
    Primary,
    Secondary,
    Middle,
}

impl PointerButton {
    pub const ALL: [Self; 3] = [Self::Primary, Self::Secondary, Self::Middle];
}

/// Which kind of input the active system context consumes.
///
/// Hosts emit only the events of the current mode, plus
/// [`InputEvent::Break`] in every mode, so one physical key never acts as a
/// game input and an editing key at the same time, and pointer input exists
/// only in [`Workspace`](Self::Workspace).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum InputMode {
    /// Text and editing events, for the shell.
    #[default]
    Text,
    /// Button and key transitions, for a running cartridge.
    Game,
    /// Text and editing events, view switching and pointer events, for the
    /// system's editor workspace.
    Workspace,
}

/// A virtual Glaux G1 controller button. Which physical input drives each
/// button is up to the host.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Button {
    Up,
    Down,
    Left,
    Right,
    A,
    B,
}

impl Button {
    pub const ALL: [Self; 6] = [
        Self::Up,
        Self::Down,
        Self::Left,
        Self::Right,
        Self::A,
        Self::B,
    ];

    /// The button's name as cartridges spell it, as in `Button.Up`.
    pub fn name(self) -> &'static str {
        match self {
            Self::Up => "Up",
            Self::Down => "Down",
            Self::Left => "Left",
            Self::Right => "Right",
            Self::A => "A",
            Self::B => "B",
        }
    }

    fn bit(self) -> u8 {
        1 << self as u8
    }
}

/// Declares [`Key`] together with its `ALL` list and cartridge names, so the
/// three cannot drift apart.
macro_rules! keys {
    ($($key:ident),+ $(,)?) => {
        /// A physical keyboard key, independent of any host's key codes.
        ///
        /// Escape is not a key: hosts reserve it for [`InputEvent::Break`].
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        #[repr(u8)]
        pub enum Key {
            $($key),+
        }

        impl Key {
            pub const ALL: &[Self] = &[$(Self::$key),+];

            /// The key's name as cartridges spell it, as in `Key.PageUp`.
            pub fn name(self) -> &'static str {
                match self {
                    $(Self::$key => stringify!($key)),+
                }
            }
        }
    };
}

keys! {
    A, B, C, D, E, F, G, H, I, J, K, L, M,
    N, O, P, Q, R, S, T, U, V, W, X, Y, Z,
    Num0, Num1, Num2, Num3, Num4, Num5, Num6, Num7, Num8, Num9,
    F1, F2, F3, F4, F5, F6, F7, F8, F9, F10, F11, F12,
    Space, Enter, Tab, Backspace, Insert, Delete, Home, End, PageUp, PageDown,
    LeftShift, RightShift, LeftControl, RightControl, LeftAlt, RightAlt,
    Up, Down, Left, Right,
}

impl Key {
    fn bit(self) -> u128 {
        1 << self as u8
    }
}

/// A set of [`Button`]s, one bit per button.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ButtonSet(u8);

impl ButtonSet {
    pub const EMPTY: Self = Self(0);

    pub fn contains(self, button: Button) -> bool {
        self.0 & button.bit() != 0
    }

    pub fn insert(&mut self, button: Button) {
        self.0 |= button.bit();
    }

    pub fn remove(&mut self, button: Button) {
        self.0 &= !button.bit();
    }
}

impl FromIterator<Button> for ButtonSet {
    fn from_iter<I: IntoIterator<Item = Button>>(iter: I) -> Self {
        let mut set = Self::EMPTY;
        for button in iter {
            set.insert(button);
        }
        set
    }
}

/// A set of [`Key`]s, one bit per key.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct KeySet(u128);

impl KeySet {
    pub const EMPTY: Self = Self(0);

    pub fn contains(self, key: Key) -> bool {
        self.0 & key.bit() != 0
    }

    pub fn insert(&mut self, key: Key) {
        self.0 |= key.bit();
    }

    pub fn remove(&mut self, key: Key) {
        self.0 &= !key.bit();
    }
}

impl FromIterator<Key> for KeySet {
    fn from_iter<I: IntoIterator<Item = Key>>(iter: I) -> Self {
        let mut set = Self::EMPTY;
        for key in iter {
            set.insert(key);
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

/// The physical keys as seen by one logical tick, with the same meaning of
/// `held` and `pressed` as [`ButtonState`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct KeyState {
    /// Keys down at the tick boundary.
    pub held: KeySet,
    /// Keys that went from released to pressed since the previous tick.
    pub pressed: KeySet,
}

/// Everything a cartridge can read about input during one logical tick.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct InputState {
    pub buttons: ButtonState,
    pub keys: KeyState,
}

/// Folds button and key events into one [`InputState`] per logical tick.
///
/// A new latch has every button and key released.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct InputLatch {
    buttons: ButtonSet,
    keys: KeySet,
}

impl InputLatch {
    pub fn new() -> Self {
        Self::default()
    }

    /// Applies the events received since the previous tick, in order, and
    /// returns the state for this tick. Text and editing events are
    /// ignored.
    ///
    /// A down event for a button or key that is already held is not a new
    /// press, and an up event for one that is not held changes nothing.
    pub fn advance(&mut self, events: &[InputEvent]) -> InputState {
        let mut buttons = ButtonSet::EMPTY;
        let mut keys = KeySet::EMPTY;
        for event in events {
            match *event {
                InputEvent::ButtonDown(button) if !self.buttons.contains(button) => {
                    self.buttons.insert(button);
                    buttons.insert(button);
                }
                InputEvent::ButtonUp(button) => self.buttons.remove(button),
                InputEvent::KeyDown(key) if !self.keys.contains(key) => {
                    self.keys.insert(key);
                    keys.insert(key);
                }
                InputEvent::KeyUp(key) => self.keys.remove(key),
                _ => {}
            }
        }
        InputState {
            buttons: ButtonState {
                held: self.buttons,
                pressed: buttons,
            },
            keys: KeyState {
                held: self.keys,
                pressed: keys,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use Button::*;
    use InputEvent::{ButtonDown, ButtonUp, KeyDown, KeyUp};

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
    fn unrelated_buttons_stay_released() {
        let mut latch = InputLatch::new();
        let tick = latch.advance(&[ButtonDown(A)]).buttons;
        for button in Button::ALL.into_iter().filter(|&b| b != A) {
            assert!(!tick.held.contains(button), "{button:?}");
            assert!(!tick.pressed.contains(button), "{button:?}");
        }
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
    fn catch_up_ticks_do_not_repeat_the_press() {
        let mut latch = InputLatch::new();
        assert_eq!(latch.advance(&[ButtonDown(B)]).buttons, state(&[B], &[B]));
        for _ in 0..3 {
            assert_eq!(latch.advance(&[]).buttons, state(&[B], &[]));
        }
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
    fn up_without_down_and_other_events_are_ignored() {
        let mut latch = InputLatch::new();
        assert_eq!(
            latch
                .advance(&[ButtonUp(A), InputEvent::Up, InputEvent::Break])
                .buttons,
            state(&[], &[])
        );
    }

    fn keys(held: &[Key], pressed: &[Key]) -> KeyState {
        KeyState {
            held: held.iter().copied().collect(),
            pressed: pressed.iter().copied().collect(),
        }
    }

    #[test]
    fn key_names_are_unique_and_escape_is_not_a_key() {
        let mut names: Vec<&str> = Key::ALL.iter().map(|key| key.name()).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), Key::ALL.len());
        assert!(!names.contains(&"Escape"));
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
    fn pointer_and_view_events_never_reach_the_latch() {
        let mut latch = InputLatch::new();
        let at = Point::new(10, 20);
        let tick = latch.advance(&[
            InputEvent::PointerMove(Some(at)),
            InputEvent::PointerDown {
                button: PointerButton::Primary,
                at,
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

    #[test]
    fn button_and_key_events_do_not_cross() {
        // `Button::Up` and `Key::Up` share a name but not a state.
        let mut latch = InputLatch::new();
        let tick = latch.advance(&[ButtonDown(Up)]);
        assert_eq!(tick.keys, KeyState::default());
        let tick = latch.advance(&[KeyUp(Key::Up)]);
        assert_eq!(tick.buttons, state(&[Up], &[]));
    }
}
