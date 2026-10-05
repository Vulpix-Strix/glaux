//! The desktop window's size, position and maximized state, kept across
//! launches in Glaux's config directory.

use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use anyhow::Context;
use glaux_host_raylib::{MonitorArea, WindowPlacement};
use glaux_video::{HEIGHT, WIDTH};
use serde::{Deserialize, Serialize};

use crate::platform::{self, SessionKind};

/// File name of the saved state inside the config directory.
const FILE_NAME: &str = "window-state.json";

/// Largest width or height accepted from a saved state.
const MAX_DIMENSION: i32 = 16_384;

/// How much of a restored window, in screen coordinates on each axis, must
/// land on one monitor for its saved position to be used.
const MIN_VISIBLE: i64 = 32;

/// The window state saved between launches.
///
/// `width` and `height` are always the last normal (not maximized, not
/// minimized) size, so un-maximizing a restored window returns to it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct WindowState {
    pub width: i32,
    pub height: i32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub x: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub y: Option<i32>,
    #[serde(default)]
    pub maximized: bool,
}

impl WindowState {
    /// The saved normal position, when both coordinates are present.
    pub fn position(&self) -> Option<(i32, i32)> {
        self.x.zip(self.y)
    }

    /// Whether the saved size is one Glaux can open a window at: at least
    /// the logical resolution and at most [`MAX_DIMENSION`] on each axis.
    fn has_valid_size(&self) -> bool {
        let valid = |value: i32, min: usize| {
            usize::try_from(value).is_ok_and(|value| value >= min) && value <= MAX_DIMENSION
        };
        valid(self.width, WIDTH) && valid(self.height, HEIGHT)
    }
}

/// Path of the saved state inside `config_dir`.
pub fn path(config_dir: &Path) -> PathBuf {
    config_dir.join(FILE_NAME)
}

/// Parses a saved state.
fn parse(text: &str) -> Result<WindowState, serde_json::Error> {
    serde_json::from_str(text)
}

/// Loads the saved state from `config_dir`.
///
/// A missing, unreadable, malformed or out-of-range state is logged and
/// treated as absent so it never stops Glaux from starting.
pub fn load(config_dir: &Path) -> Option<WindowState> {
    let path = path(config_dir);
    let text = match fs::read_to_string(&path) {
        Ok(text) => text,
        Err(err) if err.kind() == ErrorKind::NotFound => {
            tracing::debug!(path = %path.display(), "no saved window state");
            return None;
        }
        Err(err) => {
            tracing::warn!(path = %path.display(), error = %err, "failed to read the saved window state, using defaults");
            return None;
        }
    };
    let state = match parse(&text) {
        Ok(state) => state,
        Err(err) => {
            tracing::warn!(path = %path.display(), error = %err, "saved window state is malformed, using defaults");
            return None;
        }
    };
    if !state.has_valid_size() {
        tracing::warn!(
            path = %path.display(),
            width = state.width,
            height = state.height,
            "saved window size is out of range, using defaults"
        );
        return None;
    }
    tracing::debug!(?state, "loaded saved window state");
    Some(state)
}

/// Writes `state` to `config_dir`, creating the directory if needed.
///
/// The state is written to a temporary file and renamed into place so an
/// interrupted save never leaves a truncated file behind.
pub fn save(config_dir: &Path, state: &WindowState) -> anyhow::Result<()> {
    fs::create_dir_all(config_dir)
        .with_context(|| format!("failed to create {}", config_dir.display()))?;
    let path = path(config_dir);
    let temp = path.with_extension("json.tmp");
    let text = serde_json::to_string_pretty(state).context("failed to encode the window state")?;
    fs::write(&temp, text).with_context(|| format!("failed to write {}", temp.display()))?;
    fs::rename(&temp, &path).with_context(|| format!("failed to replace {}", path.display()))?;
    Ok(())
}

/// Whether window positions in `session` can be read and set.
fn positions_supported(session: SessionKind) -> bool {
    platform::window_placement(session) != WindowPlacement::PlatformDefault
}

/// Where to put a window opened with `saved` state in `session`.
///
/// The saved position is used only where the session supports positions
/// and the window would be reachable on one of `monitors`; otherwise the
/// session's usual placement applies.
pub fn plan(
    session: SessionKind,
    saved: Option<&WindowState>,
    monitors: &[MonitorArea],
) -> WindowPlacement {
    let default = platform::window_placement(session);
    if !positions_supported(session) {
        return default;
    }
    let Some((state, (x, y))) = saved.and_then(|state| Some((state, state.position()?))) else {
        return default;
    };
    if monitors
        .iter()
        .any(|monitor| visible_on(x, y, state.width, state.height, monitor))
    {
        WindowPlacement::At { x, y }
    } else {
        tracing::info!(
            x,
            y,
            width = state.width,
            height = state.height,
            "saved window position is off every monitor, using default placement"
        );
        default
    }
}

/// Whether a `width`×`height` window at (`x`, `y`) overlaps `monitor` by at
/// least [`MIN_VISIBLE`] on both axes.
fn visible_on(x: i32, y: i32, width: i32, height: i32, monitor: &MonitorArea) -> bool {
    // Saved coordinates come from a file, so widen before adding.
    let overlap = |start: i32, len: i32, m_start: i32, m_len: i32| {
        let (start, m_start) = (i64::from(start), i64::from(m_start));
        (start + i64::from(len)).min(m_start + i64::from(m_len)) - start.max(m_start)
    };
    overlap(x, width, monitor.x, monitor.width) >= MIN_VISIBLE
        && overlap(y, height, monitor.y, monitor.height) >= MIN_VISIBLE
}

/// The window's state as observed on one frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WindowSnapshot {
    pub size: (i32, i32),
    /// `None` where the session does not report positions.
    pub position: Option<(i32, i32)>,
    pub maximized: bool,
    pub minimized: bool,
}

/// Follows the window while Glaux runs and produces the state to save.
#[derive(Debug)]
pub struct WindowTracker {
    track_positions: bool,
    size: Option<(i32, i32)>,
    position: Option<(i32, i32)>,
    maximized: bool,
}

impl WindowTracker {
    /// Starts from the `saved` state the window was opened with and the
    /// `placement` it was given.
    ///
    /// Where the session does not support positions, the saved position is
    /// carried over unchanged so it is not lost to sessions that do.
    pub fn new(
        session: SessionKind,
        saved: Option<&WindowState>,
        placement: WindowPlacement,
    ) -> Self {
        let track_positions = positions_supported(session);
        let position = if track_positions {
            match placement {
                WindowPlacement::At { x, y } => Some((x, y)),
                _ => None,
            }
        } else {
            saved.and_then(WindowState::position)
        };
        Self {
            track_positions,
            size: saved.map(|state| (state.width, state.height)),
            position,
            maximized: saved.is_some_and(|state| state.maximized),
        }
    }

    /// Whether snapshots should carry the window position.
    pub fn tracks_positions(&self) -> bool {
        self.track_positions
    }

    /// Records `snapshot`.
    ///
    /// Size and position only change while the window is in its normal
    /// state; a minimized window changes nothing.
    pub fn observe(&mut self, snapshot: WindowSnapshot) {
        if snapshot.minimized {
            return;
        }
        self.maximized = snapshot.maximized;
        let (width, height) = snapshot.size;
        if snapshot.maximized || width <= 0 || height <= 0 {
            return;
        }
        self.size = Some(snapshot.size);
        if self.track_positions && snapshot.position.is_some() {
            self.position = snapshot.position;
        }
    }

    /// The state to save, or `None` if no normal size was ever observed.
    pub fn finish(&self) -> Option<WindowState> {
        let (width, height) = self.size?;
        Some(WindowState {
            width,
            height,
            x: self.position.map(|(x, _)| x),
            y: self.position.map(|(_, y)| y),
            maximized: self.maximized,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    /// A fresh, empty directory under the system temp dir.
    fn scratch_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "glaux-test-{}-window-state-{name}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

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
    fn json_round_trips() {
        let saved = WindowState {
            width: 960,
            height: 720,
            x: Some(-40),
            y: Some(120),
            maximized: true,
        };
        let text = serde_json::to_string(&saved).unwrap();
        assert_eq!(parse(&text).unwrap(), saved);
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
        let dir = scratch_dir("save");
        let saved = state(1024, 768, Some((10, 20)));

        save(&dir.join("nested"), &saved).unwrap();

        assert_eq!(load(&dir.join("nested")), Some(saved));
        assert!(!dir.join("nested").join("window-state.json.tmp").exists());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn missing_file_loads_as_absent() {
        let dir = scratch_dir("missing");
        assert_eq!(load(&dir), None);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn malformed_file_loads_as_absent() {
        let dir = scratch_dir("malformed");
        fs::write(path(&dir), "{ \"width\": 960, ").unwrap();
        assert_eq!(load(&dir), None);
        fs::write(path(&dir), r#"{"width": "wide", "height": 720}"#).unwrap();
        assert_eq!(load(&dir), None);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn out_of_range_sizes_are_rejected() {
        let dir = scratch_dir("invalid");
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
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn valid_sizes_are_restored() {
        let dir = scratch_dir("valid");
        for saved in [
            state(WIDTH as i32, HEIGHT as i32, None),
            state(1234, 777, None),
            state(MAX_DIMENSION, MAX_DIMENSION, None),
        ] {
            save(&dir, &saved).unwrap();
            assert_eq!(load(&dir), Some(saved));
        }
        fs::remove_dir_all(&dir).unwrap();
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
        let mut tracker =
            WindowTracker::new(SessionKind::X11, None, WindowPlacement::CenterOnMonitor);
        assert_eq!(tracker.finish(), None);

        tracker.observe(snapshot((960, 720), (480, 180)));
        tracker.observe(snapshot((1000, 800), (300, 100)));

        assert_eq!(tracker.finish(), Some(state(1000, 800, Some((300, 100)))));
    }

    #[test]
    fn maximizing_preserves_normal_geometry() {
        let mut tracker =
            WindowTracker::new(SessionKind::X11, None, WindowPlacement::CenterOnMonitor);
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
        let mut tracker =
            WindowTracker::new(SessionKind::X11, None, WindowPlacement::CenterOnMonitor);
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
    fn rejected_position_is_not_saved_back() {
        let mut saved = state(960, 720, Some((-1000, -100)));
        saved.maximized = true;
        let placement = plan(SessionKind::X11, Some(&saved), &[FULL_HD]);
        let tracker = WindowTracker::new(SessionKind::X11, Some(&saved), placement);

        let mut expected = state(960, 720, None);
        expected.maximized = true;
        assert_eq!(tracker.finish(), Some(expected));
    }
}
