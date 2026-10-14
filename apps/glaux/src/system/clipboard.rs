//! The host's clipboard, as the system reads and writes it.

/// Text shared with other programs through the host.
///
/// Reads and writes happen while the input that asks for them is handled,
/// so a paste lands exactly where it was asked for among the other input.
pub trait Clipboard {
    /// The clipboard's text, `None` when it holds no text.
    fn text(&mut self) -> Option<String>;
    /// Puts `text` on the clipboard.
    fn set_text(&mut self, text: &str);
}

/// A clipboard kept in memory, for running the system without a host.
#[cfg(test)]
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MemoryClipboard {
    pub text: Option<String>,
}

#[cfg(test)]
impl Clipboard for MemoryClipboard {
    fn text(&mut self) -> Option<String> {
        self.text.clone()
    }

    fn set_text(&mut self, text: &str) {
        self.text = Some(text.to_owned());
    }
}
