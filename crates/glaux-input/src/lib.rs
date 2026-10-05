//! Host-independent input events for Glaux system software.
//!
//! Hosts translate their native keyboard handling into these events; the
//! system never sees host key codes or keyboard state.

/// A semantic input event for text editing.
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
}
