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
    /// Move to the start of the previous word (Ctrl+Left on desktop).
    WordLeft {
        select: bool,
    },
    /// Move past the end of the next word (Ctrl+Right on desktop).
    WordRight {
        select: bool,
    },
    /// Move to the start of the whole text (Ctrl+Home on desktop).
    DocumentStart {
        select: bool,
    },
    /// Move to the end of the whole text (Ctrl+End on desktop).
    DocumentEnd {
        select: bool,
    },
    /// Move up by one screenful.
    PageUp {
        select: bool,
    },
    /// Move down by one screenful.
    PageDown {
        select: bool,
    },
    Backspace,
    Delete,
    Enter,
    /// Step toward older entries, such as earlier commands in a history,
    /// or move up a row.
    Up {
        select: bool,
    },
    /// Step toward newer entries, or move down a row.
    Down {
        select: bool,
    },
    /// The Tab key. What it does is up to the context: complete the word
    /// being typed, indent, or move focus to the next control.
    ///
    /// `reverse` is set when the key should act backward (typically Shift
    /// held): outdent, or move focus to the previous control.
    Tab {
        reverse: bool,
    },
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
    /// Reverse the last edit (Ctrl+Z on desktop).
    Undo,
    /// Reapply the last reversed edit (Ctrl+Y or Ctrl+Shift+Z on desktop).
    Redo,
    /// Write the open document to the cartridge (Ctrl+S on desktop).
    Save,
    /// Select the whole text (Ctrl+A on desktop).
    SelectAll,
    /// Ask to choose a file to open (Ctrl+P on desktop).
    OpenFile,
    /// Close the active document (Ctrl+W on desktop).
    CloseDocument,
    /// Switch to the next open document (Ctrl+PageDown on desktop).
    NextDocument,
    /// Switch to the previous open document (Ctrl+PageUp on desktop).
    PreviousDocument,
    /// Ask to create a new file (Ctrl+N on desktop).
    NewFile,
    /// Search the open document (Ctrl+F on desktop).
    Find,
    /// Search the open document and replace matches (Ctrl+H on desktop).
    Replace,
    /// Ask for a line of the open document to move to (Ctrl+G on desktop).
    GoToLine,
    /// Put the selection on the clipboard (Ctrl+C on desktop).
    Copy,
    /// Put the selection on the clipboard and delete it (Ctrl+X on
    /// desktop).
    Cut,
    /// Insert the clipboard's text (Ctrl+V on desktop).
    Paste,
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
    /// [`PointerUp`](Self::PointerUp), delivered at the clamped position if
    /// the button is released outside the viewport. The one exception is a
    /// capture the host loses, such as when the window loses focus: then
    /// [`PointerCancel`](Self::PointerCancel) ends every capture instead,
    /// and no `PointerUp` follows for them.
    /// Presses that start outside the viewport, such as on the bars around
    /// it, are not reported, and neither are their releases.
    ///
    /// `select` is set when the press should extend the current selection
    /// (typically Shift held) instead of starting a new one.
    PointerDown {
        button: PointerButton,
        at: Point,
        select: bool,
    },
    /// A captured pointer button was released.
    PointerUp {
        button: PointerButton,
        at: Point,
    },
    /// Every captured pointer button lost its capture without a release,
    /// for example because the window lost focus. Abandon any gesture the
    /// captured presses started. Buttons still held are not reported again
    /// until they are released and pressed anew.
    PointerCancel,
    /// The scroll wheel moved by whole notches; positive is away from the
    /// user. Hosts report at most one wheel event per host frame.
    Wheel {
        delta: i8,
    },
    /// Sideways scrolling by whole notches, from a horizontal wheel or the
    /// wheel turned while Shift is held; positive is toward the left, as
    /// Shift with the wheel turned away from the user. Hosts report at most
    /// one horizontal wheel event per host frame.
    WheelHorizontal {
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
                | Self::PointerCancel
                | Self::Wheel { .. }
                | Self::WheelHorizontal { .. }
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
    /// Text and editing events, view switching, editing and document
    /// commands such as [`InputEvent::Save`] and [`InputEvent::OpenFile`],
    /// and pointer events, for the system's editor workspace.
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
mod tests;
