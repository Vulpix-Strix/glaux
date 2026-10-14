//! The desktop clipboard, as text.

use std::ffi::CStr;

use raylib::prelude::*;

/// The clipboard's text, `None` when it holds no text.
///
/// Invalid UTF-8 is replaced rather than refused, so a paste from a
/// program with another encoding still inserts something readable.
pub(crate) fn text(_rl: &RaylibHandle) -> Option<String> {
    // raylib-rs's `get_clipboard_text` builds a `CStr` from the returned
    // pointer without checking it, but GLFW returns NULL when the clipboard
    // is empty or holds no text, so the call is made here instead.
    //
    // ## Safety
    // the `RaylibHandle` borrow proves the window, and with it
    // GLFW, is initialized, and this runs on the thread that owns it.
    let ptr = unsafe { ffi::GetClipboardText() };
    if ptr.is_null() {
        return None;
    }
    // SAFETY: GLFW returns a NUL-terminated string it owns, valid until the
    // next clipboard call; it is copied out before anything else runs.
    let bytes = unsafe { CStr::from_ptr(ptr) }.to_bytes();
    Some(String::from_utf8_lossy(bytes).into_owned())
}

/// Puts `text` on the clipboard. Text holding a NUL character cannot be
/// handed to the platform, so the clipboard is then left as it was.
pub(crate) fn set_text(rl: &mut RaylibHandle, text: &str) {
    if let Err(err) = rl.set_clipboard_text(text) {
        tracing::warn!(error = %err, "text not copied: it holds a NUL character");
    }
}
