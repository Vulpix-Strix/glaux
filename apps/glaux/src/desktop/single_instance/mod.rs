//! Keeps normal Glaux launches to one running instance.
//!
//! The first process becomes the primary and listens for later launches.
//! A later process hands its arguments and working directory to the
//! primary and exits. Ownership is tied to resources the operating system
//! reclaims when a process dies, so a crash never blocks the next launch:
//!
//! - Linux: a well-known name on the session D-Bus, falling back to a Unix
//!   socket in `$XDG_RUNTIME_DIR` when no session bus is reachable.
//! - Windows: a named mutex plus a message-only window receiving
//!   `WM_COPYDATA`.
//! - macOS: a Unix socket in the per-user temporary directory.

#[cfg(target_os = "linux")]
mod dbus;
#[cfg(unix)]
mod socket;
#[cfg(windows)]
mod windows;

use std::any::Any;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};

use serde::{Deserialize, Serialize};

/// Application identity the instance endpoints are named after.
#[cfg_attr(not(any(unix, windows)), allow(dead_code))]
const IDENTIFIER: &str = "dev.cuervolu.Glaux";

/// What a later launch hands to the primary instance.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SecondaryLaunch {
    /// The full command line, program name first.
    pub args: Vec<String>,
    /// The working directory the launch ran in.
    pub cwd: PathBuf,
}

impl SecondaryLaunch {
    /// This process's command line and working directory.
    ///
    /// Arguments and the directory are converted lossily so the payload is
    /// always valid UTF-8 for every transport.
    pub fn current() -> Self {
        let cwd = std::env::current_dir()
            .map(|dir| PathBuf::from(dir.to_string_lossy().into_owned()))
            .unwrap_or_default();
        Self {
            args: std::env::args_os()
                .map(|arg| arg.to_string_lossy().into_owned())
                .collect(),
            cwd,
        }
    }

    #[cfg_attr(not(any(unix, windows)), allow(dead_code))]
    fn to_json(&self) -> serde_json::Result<Vec<u8>> {
        serde_json::to_vec(self)
    }

    #[cfg_attr(not(any(unix, windows)), allow(dead_code))]
    fn from_json(bytes: &[u8]) -> serde_json::Result<Self> {
        serde_json::from_slice(bytes)
    }
}

/// How this process should proceed after looking for a running instance.
pub enum Startup {
    /// No other instance is running; this process continues as Glaux.
    Primary(PrimaryInstance),
    /// The launch was handed to the running instance; exit successfully.
    Forwarded,
    /// An instance is running but the launch could not be handed to it.
    /// Starting a second instance anyway would break single-instance
    /// semantics, so this process exits with the error.
    ForwardFailed(anyhow::Error),
}

/// The running primary instance.
pub struct PrimaryInstance {
    // Held only for its `Drop`, which releases ownership. Declared first so
    // listeners stop before the receiving end of their channel goes away.
    _owner: Option<Box<dyn Any>>,
    launches: Receiver<SecondaryLaunch>,
    /// Why this process runs without exclusivity, if it does.
    pub degraded: Option<anyhow::Error>,
}

impl PrimaryInstance {
    /// The launches received since the previous call, oldest first. Never
    /// blocks.
    pub fn drain(&self) -> impl Iterator<Item = SecondaryLaunch> + '_ {
        self.launches.try_iter()
    }
}

/// The outcome of one backend's attempt to claim the instance.
enum Claim<O> {
    /// This process owns the instance; dropping `O` releases it.
    Owner(O),
    /// The launch was handed to the running owner.
    Forwarded,
    /// An owner is confirmed running but the hand-off failed.
    ForwardFailed(anyhow::Error),
    /// The backend cannot provide exclusivity here.
    Unavailable(anyhow::Error),
}

impl<O: 'static> Claim<O> {
    fn erase(self) -> Claim<Box<dyn Any>> {
        match self {
            Self::Owner(owner) => Claim::Owner(Box::new(owner)),
            Self::Forwarded => Claim::Forwarded,
            Self::ForwardFailed(err) => Claim::ForwardFailed(err),
            Self::Unavailable(err) => Claim::Unavailable(err),
        }
    }
}

/// Becomes the primary instance or hands `launch` to the one running.
///
/// Failing to set up any exclusivity mechanism is not fatal: the process
/// becomes a primary with [`PrimaryInstance::degraded`] set.
pub fn start(launch: &SecondaryLaunch) -> Startup {
    let (sender, launches) = mpsc::channel();
    match claim(launch, sender) {
        Claim::Owner(owner) => Startup::Primary(PrimaryInstance {
            _owner: Some(owner),
            launches,
            degraded: None,
        }),
        Claim::Forwarded => Startup::Forwarded,
        Claim::ForwardFailed(err) => Startup::ForwardFailed(err),
        Claim::Unavailable(err) => Startup::Primary(PrimaryInstance {
            _owner: None,
            launches,
            degraded: Some(err),
        }),
    }
}

#[cfg(target_os = "linux")]
fn claim(launch: &SecondaryLaunch, sender: Sender<SecondaryLaunch>) -> Claim<Box<dyn Any>> {
    let bus_error = match dbus::claim(launch, sender.clone()) {
        Claim::Unavailable(err) => err,
        claim => return claim.erase(),
    };
    let Some(runtime_dir) = crate::app_dirs::runtime_dir() else {
        return Claim::Unavailable(
            bus_error.context("no runtime directory is available for the socket fallback"),
        );
    };
    match socket::claim(&runtime_dir.join(socket::file_name()), launch, sender) {
        Claim::Unavailable(err) => Claim::Unavailable(err.context(format!(
            "session D-Bus unavailable ({bus_error:#}) and the socket fallback failed"
        ))),
        claim => claim.erase(),
    }
}

#[cfg(target_os = "macos")]
fn claim(launch: &SecondaryLaunch, sender: Sender<SecondaryLaunch>) -> Claim<Box<dyn Any>> {
    // `$TMPDIR` is a private per-user directory on macOS.
    let path = std::env::temp_dir().join(socket::file_name());
    socket::claim(&path, launch, sender).erase()
}

#[cfg(windows)]
fn claim(launch: &SecondaryLaunch, sender: Sender<SecondaryLaunch>) -> Claim<Box<dyn Any>> {
    windows::claim(launch, sender).erase()
}

#[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
fn claim(_launch: &SecondaryLaunch, _sender: Sender<SecondaryLaunch>) -> Claim<Box<dyn Any>> {
    Claim::Unavailable(anyhow::anyhow!(
        "single-instance launches are not supported on this platform"
    ))
}
