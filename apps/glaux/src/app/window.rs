//! Opening the Glaux window where it was last left, and saving that state
//! when the application ends.

use std::path::Path;

use glaux_host_raylib::{Presentation, RaylibHost};
use glaux_video::{HEIGHT, WIDTH};

use crate::desktop::window_state::{self, WindowSnapshot, WindowTracker};
use crate::platform::{SessionKind, sets_window_icon};

/// Window size, as a multiple of the logical resolution, when no saved
/// state applies.
const WINDOW_SCALE: i32 = 3;

/// How the host draws frames to the window.
const PRESENTATION: Presentation = Presentation::Crt;

/// The window icon at the sizes desktops ask for, smallest first.
const WINDOW_ICONS: [&[u8]; 4] = [
    include_bytes!("../../../../rom/app-icon/glaux-16.png"),
    include_bytes!("../../../../rom/app-icon/glaux-32.png"),
    include_bytes!("../../../../rom/app-icon/glaux-48.png"),
    include_bytes!("../../../../rom/app-icon/glaux-64.png"),
];

/// What Glaux does to the window on top of what the host does by itself.
#[derive(Debug)]
pub(super) struct WindowLifecycle {
    tracker: WindowTracker,
    /// Set when the saved state was maximized, until the first frame has
    /// been shown.
    maximize_pending: bool,
}

impl WindowLifecycle {
    /// Opens the window where the saved state in `config_dir` puts it.
    pub(super) fn open(
        session: SessionKind,
        config_dir: Option<&Path>,
    ) -> anyhow::Result<(RaylibHost, Self)> {
        let saved = config_dir.and_then(window_state::load);
        let size = saved.map_or(
            (WIDTH as i32 * WINDOW_SCALE, HEIGHT as i32 * WINDOW_SCALE),
            |state| (state.width, state.height),
        );
        let mut host = RaylibHost::new("Glaux", size, PRESENTATION)?;
        if sets_window_icon(session) {
            host.set_window_icons(&WINDOW_ICONS);
        }
        let placement = window_state::plan(session, saved.as_ref(), &host.monitors());
        host.place(placement);
        let lifecycle = Self {
            tracker: WindowTracker::new(session, saved.as_ref(), placement),
            maximize_pending: saved.is_some_and(|state| state.maximized),
        };
        Ok((host, lifecycle))
    }

    /// Records the window as it is now.
    pub(super) fn observe(&mut self, host: &RaylibHost) {
        let snapshot = WindowSnapshot {
            size: host.window_size(),
            position: self
                .tracker
                .tracks_positions()
                .then(|| host.window_position()),
            maximized: host.is_maximized(),
            minimized: host.is_minimized(),
        };
        self.tracker.observe(snapshot);
    }

    /// Maximizes the window once a frame has mapped it at its normal size. A
    /// Wayland window maximized before it is mapped has no normal geometry,
    /// so un-maximizing it would keep the maximized size.
    pub(super) fn after_present(&mut self, host: &mut RaylibHost) {
        if std::mem::take(&mut self.maximize_pending) {
            host.maximize();
        }
    }

    /// Saves the window state to `config_dir`, if the window was ever seen
    /// at a normal size.
    pub(super) fn finish(self, config_dir: Option<&Path>) {
        let (Some(dir), Some(state)) = (config_dir, self.tracker.finish()) else {
            return;
        };
        match window_state::save(dir, &state) {
            Ok(()) => tracing::debug!(?state, "window state saved"),
            Err(err) => {
                tracing::warn!(error = %format!("{err:#}"), "failed to save the window state")
            }
        }
    }
}
