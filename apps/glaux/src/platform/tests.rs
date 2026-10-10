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
