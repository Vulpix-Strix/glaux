use super::*;
use crate::scratch_dir::ScratchDir;
use pretty_assertions::assert_eq;

fn state(width: i32, height: i32, position: Option<(i32, i32)>) -> WindowState {
    WindowState {
        width,
        height,
        x: position.map(|(x, _)| x),
        y: position.map(|(_, y)| y),
        maximized: false,
    }
}

const FULL_HD: MonitorArea = MonitorArea {
    x: 0,
    y: 0,
    width: 1920,
    height: 1080,
};
const LEFT_OF_PRIMARY: MonitorArea = MonitorArea {
    x: -1280,
    y: -200,
    width: 1280,
    height: 1024,
};

fn snapshot(size: (i32, i32), position: (i32, i32)) -> WindowSnapshot {
    WindowSnapshot {
        size,
        position: Some(position),
        maximized: false,
        minimized: false,
    }
}

#[test]
fn position_and_maximized_are_optional() {
    assert_eq!(
        parse(r#"{"width": 800, "height": 600}"#).unwrap(),
        state(800, 600, None)
    );
    let text = serde_json::to_string(&state(800, 600, None)).unwrap();
    assert!(!text.contains("\"x\""), "{text}");
}

#[test]
fn saved_state_round_trips_through_the_file() {
    let dir = ScratchDir::new("window-state-save");
    let saved = state(1024, 768, Some((10, 20)));

    save(&dir.join("nested"), &saved).unwrap();

    assert_eq!(load(&dir.join("nested")), Some(saved));
    assert!(!dir.join("nested").join("window-state.json.tmp").exists());
}

#[test]
fn missing_file_loads_as_absent() {
    let dir = ScratchDir::new("window-state-missing");
    assert_eq!(load(&dir), None);
}

#[test]
fn malformed_file_loads_as_absent() {
    let dir = ScratchDir::new("window-state-malformed");
    fs::write(path(&dir), "{ \"width\": 960, ").unwrap();
    assert_eq!(load(&dir), None);
    fs::write(path(&dir), r#"{"width": "wide", "height": 720}"#).unwrap();
    assert_eq!(load(&dir), None);
}

#[test]
fn out_of_range_sizes_are_rejected() {
    let dir = ScratchDir::new("window-state-invalid");
    let (w, h) = (WIDTH as i32, HEIGHT as i32);
    for (width, height) in [
        (0, 0),
        (-960, 720),
        (w - 1, h),
        (w, h - 1),
        (MAX_DIMENSION + 1, 720),
    ] {
        save(&dir, &state(width, height, None)).unwrap();
        assert_eq!(load(&dir), None, "{width}x{height}");
    }
}

#[test]
fn valid_sizes_are_restored() {
    let dir = ScratchDir::new("window-state-valid");
    for saved in [
        state(WIDTH as i32, HEIGHT as i32, None),
        state(1234, 777, None),
        state(MAX_DIMENSION, MAX_DIMENSION, None),
    ] {
        save(&dir, &saved).unwrap();
        assert_eq!(load(&dir), Some(saved));
    }
}

#[test]
fn position_on_a_monitor_is_restored() {
    let saved = state(960, 720, Some((100, 50)));
    assert_eq!(
        plan(SessionKind::X11, Some(&saved), &[FULL_HD]),
        WindowPlacement::At { x: 100, y: 50 }
    );
}

#[test]
fn partially_visible_window_is_restored() {
    let saved = state(960, 720, Some((1920 - 100, 1080 - 100)));
    assert_eq!(
        plan(SessionKind::X11, Some(&saved), &[FULL_HD]),
        WindowPlacement::At { x: 1820, y: 980 }
    );
}

#[test]
fn position_on_a_monitor_at_negative_offset_is_restored() {
    let saved = state(960, 720, Some((-1000, -100)));
    assert_eq!(
        plan(
            SessionKind::Unknown,
            Some(&saved),
            &[FULL_HD, LEFT_OF_PRIMARY]
        ),
        WindowPlacement::At { x: -1000, y: -100 }
    );
}

#[test]
fn position_off_every_monitor_is_ignored() {
    // The monitor the window was on has been disconnected.
    let saved = state(960, 720, Some((-1000, -100)));
    assert_eq!(
        plan(SessionKind::X11, Some(&saved), &[FULL_HD]),
        WindowPlacement::CenterOnMonitor
    );
    let far = state(960, 720, Some((i32::MAX, i32::MAX)));
    assert_eq!(
        plan(SessionKind::X11, Some(&far), &[FULL_HD]),
        WindowPlacement::CenterOnMonitor
    );
}

#[test]
fn a_sliver_on_a_monitor_is_not_enough() {
    let saved = state(960, 720, Some((1920 - 10, 0)));
    assert_eq!(
        plan(SessionKind::X11, Some(&saved), &[FULL_HD]),
        WindowPlacement::CenterOnMonitor
    );
}

#[test]
fn no_monitors_means_default_placement() {
    let saved = state(960, 720, Some((100, 50)));
    assert_eq!(
        plan(SessionKind::X11, Some(&saved), &[]),
        WindowPlacement::CenterOnMonitor
    );
}

#[test]
fn missing_state_or_position_means_default_placement() {
    assert_eq!(
        plan(SessionKind::X11, None, &[FULL_HD]),
        WindowPlacement::CenterOnMonitor
    );
    assert_eq!(
        plan(SessionKind::X11, Some(&state(960, 720, None)), &[FULL_HD]),
        WindowPlacement::CenterOnMonitor
    );
}

#[test]
fn wayland_ignores_saved_position() {
    let saved = state(960, 720, Some((100, 50)));
    assert_eq!(
        plan(SessionKind::Wayland, Some(&saved), &[FULL_HD]),
        WindowPlacement::PlatformDefault
    );
}

#[test]
fn wayland_keeps_size_and_carries_the_old_position_over() {
    let saved = state(960, 720, Some((100, 50)));
    let mut tracker = WindowTracker::new(
        SessionKind::Wayland,
        Some(&saved),
        WindowPlacement::PlatformDefault,
    );
    assert!(!tracker.tracks_positions());

    tracker.observe(WindowSnapshot {
        size: (1280, 960),
        position: None,
        maximized: false,
        minimized: false,
    });

    assert_eq!(tracker.finish(), Some(state(1280, 960, Some((100, 50)))));
}

#[test]
fn tracker_follows_normal_geometry() {
    let mut tracker = WindowTracker::new(SessionKind::X11, None, WindowPlacement::CenterOnMonitor);
    assert_eq!(tracker.finish(), None);

    tracker.observe(snapshot((960, 720), (480, 180)));
    tracker.observe(snapshot((1000, 800), (300, 100)));

    assert_eq!(tracker.finish(), Some(state(1000, 800, Some((300, 100)))));
}

#[test]
fn maximizing_preserves_normal_geometry() {
    let mut tracker = WindowTracker::new(SessionKind::X11, None, WindowPlacement::CenterOnMonitor);
    tracker.observe(snapshot((960, 720), (480, 180)));

    tracker.observe(WindowSnapshot {
        size: (1920, 1050),
        position: Some((0, 30)),
        maximized: true,
        minimized: false,
    });

    let mut expected = state(960, 720, Some((480, 180)));
    expected.maximized = true;
    assert_eq!(tracker.finish(), Some(expected));
}

#[test]
fn minimized_and_empty_geometry_is_ignored() {
    let mut tracker = WindowTracker::new(SessionKind::X11, None, WindowPlacement::CenterOnMonitor);
    tracker.observe(snapshot((960, 720), (480, 180)));

    tracker.observe(WindowSnapshot {
        size: (0, 0),
        position: Some((-32000, -32000)),
        maximized: false,
        minimized: true,
    });
    tracker.observe(snapshot((0, 0), (10, 10)));

    assert_eq!(tracker.finish(), Some(state(960, 720, Some((480, 180)))));
}

#[test]
fn restored_maximized_window_keeps_its_saved_normal_geometry() {
    let mut saved = state(960, 720, Some((100, 50)));
    saved.maximized = true;
    let placement = plan(SessionKind::X11, Some(&saved), &[FULL_HD]);
    let mut tracker = WindowTracker::new(SessionKind::X11, Some(&saved), placement);

    tracker.observe(WindowSnapshot {
        size: (1920, 1050),
        position: Some((0, 30)),
        maximized: true,
        minimized: false,
    });

    assert_eq!(tracker.finish(), Some(saved));
}

#[test]
fn deferred_startup_maximize_keeps_saved_geometry() {
    let mut saved = state(960, 720, Some((100, 50)));
    saved.maximized = true;
    let placement = plan(SessionKind::X11, Some(&saved), &[FULL_HD]);
    let mut tracker = WindowTracker::new(SessionKind::X11, Some(&saved), placement);

    // The first frame is presented at the normal size, then maximized.
    tracker.observe(snapshot((960, 720), (100, 50)));
    tracker.observe(WindowSnapshot {
        size: (1920, 1050),
        position: Some((0, 30)),
        maximized: true,
        minimized: false,
    });

    assert_eq!(tracker.finish(), Some(saved));
}

#[test]
fn rejected_position_is_not_saved_back() {
    let mut saved = state(960, 720, Some((-1000, -100)));
    saved.maximized = true;
    let placement = plan(SessionKind::X11, Some(&saved), &[FULL_HD]);
    let tracker = WindowTracker::new(SessionKind::X11, Some(&saved), placement);

    let mut expected = state(960, 720, None);
    expected.maximized = true;
    assert_eq!(tracker.finish(), Some(expected));
}
