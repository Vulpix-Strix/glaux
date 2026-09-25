//! Classification of the desktop session Glaux runs in.

use std::fmt;

use glaux_host_raylib::WindowPlacement;

/// Windowing system of a Linux desktop session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionKind {
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
pub fn detect() -> SessionKind {
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

/// Initial window placement for `session`.
///
/// Wayland has no global window positions, so the compositor chooses.
pub fn window_placement(session: SessionKind) -> WindowPlacement {
    match session {
        SessionKind::Wayland => WindowPlacement::PlatformDefault,
        SessionKind::X11 | SessionKind::Unknown => WindowPlacement::CenterOnMonitor,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    fn classify_with(vars: &[(&str, &str)]) -> SessionKind {
        classify(|name| {
            vars.iter()
                .find(|(key, _)| *key == name)
                .map(|(_, value)| value.to_string())
        })
    }

    #[test]
    fn session_type_decides_when_set() {
        assert_eq!(
            classify_with(&[("XDG_SESSION_TYPE", "wayland")]),
            SessionKind::Wayland
        );
        assert_eq!(
            classify_with(&[
                ("XDG_SESSION_TYPE", "x11"),
                ("WAYLAND_DISPLAY", "wayland-0")
            ]),
            SessionKind::X11
        );
    }

    #[test]
    fn falls_back_to_display_variables() {
        assert_eq!(
            classify_with(&[("WAYLAND_DISPLAY", "wayland-0"), ("DISPLAY", ":0")]),
            SessionKind::Wayland
        );
        assert_eq!(classify_with(&[("DISPLAY", ":0")]), SessionKind::X11);
        assert_eq!(
            classify_with(&[("XDG_SESSION_TYPE", "tty"), ("DISPLAY", ":1")]),
            SessionKind::X11
        );
    }

    #[test]
    fn unknown_without_display_hints() {
        assert_eq!(classify_with(&[]), SessionKind::Unknown);
        assert_eq!(
            classify_with(&[("XDG_SESSION_TYPE", "tty")]),
            SessionKind::Unknown
        );
    }

    #[test]
    fn empty_values_count_as_unset() {
        assert_eq!(
            classify_with(&[
                ("XDG_SESSION_TYPE", ""),
                ("WAYLAND_DISPLAY", ""),
                ("DISPLAY", ":0")
            ]),
            SessionKind::X11
        );
        assert_eq!(
            classify_with(&[("WAYLAND_DISPLAY", ""), ("DISPLAY", "")]),
            SessionKind::Unknown
        );
    }

    #[test]
    fn only_wayland_leaves_placement_to_the_compositor() {
        assert_eq!(
            window_placement(SessionKind::Wayland),
            WindowPlacement::PlatformDefault
        );
        assert_eq!(
            window_placement(SessionKind::X11),
            WindowPlacement::CenterOnMonitor
        );
        assert_eq!(
            window_placement(SessionKind::Unknown),
            WindowPlacement::CenterOnMonitor
        );
    }
}
