//! Instance ownership through a Unix domain socket.
//!
//! A socket file outlives a crashed owner, so a file nobody accepts
//! connections on is treated as stale and replaced.

use std::fs;
use std::io::{self, ErrorKind, Read, Write};
use std::net::Shutdown;
use std::os::unix::fs::MetadataExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::thread::{self, JoinHandle};
use std::time::Duration;

use anyhow::Context;

use super::{Claim, IDENTIFIER, SecondaryLaunch};

/// How long the listener sleeps when no connection is pending. Bounds how
/// long dropping the owner waits for the listener to stop.
const POLL_INTERVAL: Duration = Duration::from_millis(50);

/// How long the listener waits for a connected peer to send its payload.
const READ_TIMEOUT: Duration = Duration::from_secs(1);

/// Largest payload accepted from a peer.
const MAX_PAYLOAD: u64 = 64 * 1024;

/// File name of the socket inside its directory.
pub(super) fn file_name() -> String {
    format!("{IDENTIFIER}.sock")
}

/// Owns the socket at `path` and runs the listener thread.
///
/// Dropping it stops and joins the listener, then removes the socket file if
/// it is still the one this owner bound.
pub(super) struct Owner {
    path: PathBuf,
    /// Device and inode of the socket file this owner bound.
    identity: (u64, u64),
    stop: Arc<AtomicBool>,
    listener: Option<JoinHandle<()>>,
}

impl Drop for Owner {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(listener) = self.listener.take()
            && listener.join().is_err()
        {
            tracing::warn!("single-instance listener panicked");
        }
        // Another process may have replaced a socket it believed stale.
        // Only remove the file this owner created.
        if file_identity(&self.path).is_ok_and(|identity| identity == self.identity)
            && let Err(err) = fs::remove_file(&self.path)
        {
            tracing::warn!(path = %self.path.display(), error = %err, "failed to remove the single-instance socket");
        }
    }
}

/// Binds the socket at `path`, or forwards `launch` to the process
/// listening on it.
pub(super) fn claim(
    path: &Path,
    launch: &SecondaryLaunch,
    launches: Sender<SecondaryLaunch>,
) -> Claim<Owner> {
    match UnixStream::connect(path) {
        Ok(stream) => {
            return match send(stream, launch) {
                Ok(()) => Claim::Forwarded,
                Err(err) => Claim::ForwardFailed(
                    err.context(format!("failed to send the launch to {}", path.display())),
                ),
            };
        }
        Err(err)
            if matches!(
                err.kind(),
                ErrorKind::ConnectionRefused | ErrorKind::NotFound
            ) => {}
        Err(err) => {
            return Claim::Unavailable(
                anyhow::Error::new(err).context(format!("failed to connect to {}", path.display())),
            );
        }
    }

    match bind(path, launches) {
        Ok(owner) => Claim::Owner(owner),
        Err(err) => Claim::Unavailable(err),
    }
}

/// Writes `launch` to a connected owner.
fn send(mut stream: UnixStream, launch: &SecondaryLaunch) -> anyhow::Result<()> {
    let payload = launch.to_json().context("failed to encode the launch")?;
    stream.write_all(&payload)?;
    stream.shutdown(Shutdown::Write)?;
    Ok(())
}

/// Replaces any stale socket at `path`, binds it and starts the listener.
fn bind(path: &Path, launches: Sender<SecondaryLaunch>) -> anyhow::Result<Owner> {
    match fs::remove_file(path) {
        Ok(()) => tracing::info!(path = %path.display(), "removed a stale single-instance socket"),
        Err(err) if err.kind() == ErrorKind::NotFound => {}
        Err(err) => {
            return Err(err).with_context(|| format!("failed to remove stale {}", path.display()));
        }
    }
    let listener =
        UnixListener::bind(path).with_context(|| format!("failed to bind {}", path.display()))?;
    let identity =
        file_identity(path).with_context(|| format!("failed to inspect {}", path.display()))?;
    listener
        .set_nonblocking(true)
        .context("failed to make the single-instance socket non-blocking")?;

    let stop = Arc::new(AtomicBool::new(false));
    let thread = thread::Builder::new()
        .name("glaux-single-instance".into())
        .spawn({
            let stop = Arc::clone(&stop);
            move || listen(&listener, &stop, &launches)
        })
        .context("failed to start the single-instance listener")?;
    tracing::debug!(path = %path.display(), "owning the single-instance socket");

    Ok(Owner {
        path: path.to_path_buf(),
        identity,
        stop,
        listener: Some(thread),
    })
}

/// Accepts launches until `stop` is set.
fn listen(listener: &UnixListener, stop: &AtomicBool, launches: &Sender<SecondaryLaunch>) {
    while !stop.load(Ordering::Relaxed) {
        match listener.accept() {
            Ok((stream, _)) => match receive(stream) {
                Ok(launch) => {
                    // The receiver is gone only while Glaux is shutting down.
                    let _ = launches.send(launch);
                }
                Err(err) => {
                    tracing::warn!(error = %format!("{err:#}"), "ignoring an unreadable launch");
                }
            },
            Err(err) if err.kind() == ErrorKind::WouldBlock => thread::sleep(POLL_INTERVAL),
            Err(err) if err.kind() == ErrorKind::Interrupted => {}
            Err(err) => {
                tracing::warn!(error = %err, "failed to accept a launch");
                thread::sleep(POLL_INTERVAL);
            }
        }
    }
}

/// Reads one launch from a connected peer.
fn receive(stream: UnixStream) -> anyhow::Result<SecondaryLaunch> {
    // Accepted streams can inherit the listener's non-blocking mode.
    stream.set_nonblocking(false)?;
    stream.set_read_timeout(Some(READ_TIMEOUT))?;
    let mut payload = Vec::new();
    stream.take(MAX_PAYLOAD + 1).read_to_end(&mut payload)?;
    anyhow::ensure!(
        payload.len() as u64 <= MAX_PAYLOAD,
        "launch payload exceeds {MAX_PAYLOAD} bytes"
    );
    SecondaryLaunch::from_json(&payload).context("malformed launch payload")
}

/// Device and inode of the file at `path`.
fn file_identity(path: &Path) -> io::Result<(u64, u64)> {
    let metadata = fs::symlink_metadata(path)?;
    Ok((metadata.dev(), metadata.ino()))
}

#[cfg(test)]
mod tests;
