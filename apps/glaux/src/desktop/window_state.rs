//! The desktop window's size, position and maximized state, kept across
//! launches in Glaux's config directory.

use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use anyhow::Context;
use glaux_host_raylib::{MonitorArea, WindowPlacement};
use glaux_video::{HEIGHT, WIDTH};
use serde::{Deserialize, Serialize};

use crate::config_file;
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
pub(crate) struct WindowState {
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
    pub(crate) fn position(&self) -> Option<(i32, i32)> {
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
pub(crate) fn path(config_dir: &Path) -> PathBuf {
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
pub(crate) fn load(config_dir: &Path) -> Option<WindowState> {
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
/// An interrupted save never leaves a truncated file behind.
pub(crate) fn save(config_dir: &Path, state: &WindowState) -> anyhow::Result<()> {
    let text = serde_json::to_string_pretty(state).context("failed to encode the window state")?;
    config_file::write_atomically(&path(config_dir), &text)
}

/// Whether window positions in `session` can be read and set.
fn positions_supported(session: SessionKind) -> bool {
    platform::window_placement(session) != WindowPlacement::PlatformDefault
}

/// Where to put a window opened with `saved` state in `session`.
///
/// The saved position is used only where the session supports positions
/// and the window would be reachable on one of `monitors`. Otherwise the
/// session's usual placement applies.
pub(crate) fn plan(
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
pub(crate) struct WindowSnapshot {
    pub size: (i32, i32),
    /// `None` where the session does not report positions.
    pub position: Option<(i32, i32)>,
    pub maximized: bool,
    pub minimized: bool,
}

/// Follows the window while Glaux runs and produces the state to save.
#[derive(Debug)]
pub(crate) struct WindowTracker {
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
    pub(crate) fn new(
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
    pub(crate) fn tracks_positions(&self) -> bool {
        self.track_positions
    }

    /// Records `snapshot`.
    ///
    /// Size and position only change while the window is in its normal
    /// state. A minimized window changes nothing.
    pub(crate) fn observe(&mut self, snapshot: WindowSnapshot) {
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
    pub(crate) fn finish(&self) -> Option<WindowState> {
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
mod tests;
