//! Classification of the desktop session Glaux runs in.

#![warn(unreachable_pub)]

use std::fmt;

use glaux_host_raylib::WindowPlacement;

/// Windowing system of a Linux desktop session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SessionKind {
    Wayland,
    X11,
    Unknown,
}

impl fmt::Display for SessionKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Wayland => "wayland",
            Self::X11 => "x11",
            Self::Unknown => "unknown",
        })
    }
}

/// Classifies the session from the process environment.
pub(crate) fn detect() -> SessionKind {
    classify(|name| std::env::var(name).ok())
}

/// Classifies the session from environment variables read through `var`.
///
/// An explicit `XDG_SESSION_TYPE` of `wayland` or `x11` wins. Otherwise the
/// presence of a Wayland display, then of an X display, decides. Empty values
/// count as unset.
fn classify(var: impl Fn(&str) -> Option<String>) -> SessionKind {
    let var = |name| var(name).filter(|value: &String| !value.is_empty());

    match var("XDG_SESSION_TYPE").as_deref() {
        Some("wayland") => return SessionKind::Wayland,
        Some("x11") => return SessionKind::X11,
        _ => {}
    }
    if var("WAYLAND_DISPLAY").is_some() {
        SessionKind::Wayland
    } else if var("DISPLAY").is_some() {
        SessionKind::X11
    } else {
        SessionKind::Unknown
    }
}

/// Whether the window icon can be set at runtime in `session`.
///
/// macOS takes the icon from the app bundle, and Wayland from the desktop
/// entry matching the application id. GLFW ignores a runtime icon in both.
pub(crate) fn sets_window_icon(session: SessionKind) -> bool {
    !cfg!(target_os = "macos") && session != SessionKind::Wayland
}

/// Initial window placement for `session`.
///
/// Wayland has no global window positions, so the compositor chooses.
pub(crate) fn window_placement(session: SessionKind) -> WindowPlacement {
    match session {
        SessionKind::Wayland => WindowPlacement::PlatformDefault,
        SessionKind::X11 | SessionKind::Unknown => WindowPlacement::CenterOnMonitor,
    }
}

#[cfg(test)]
mod tests;
