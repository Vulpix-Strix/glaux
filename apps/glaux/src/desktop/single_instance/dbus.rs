//! Instance ownership through a well-known name on the session D-Bus.
//!
//! The bus releases the name when the owning connection closes, including
//! when the process dies.

use std::path::PathBuf;
use std::sync::mpsc::Sender;

use anyhow::Context;
use zbus::blocking::Connection;
use zbus::blocking::connection::Builder;
use zbus::fdo::RequestNameFlags;

use super::{Claim, SecondaryLaunch};

const BUS_NAME: &str = "dev.cuervolu.Glaux.SingleInstance";
const OBJECT_PATH: &str = "/dev/cuervolu/Glaux/SingleInstance";
const INTERFACE: &str = "dev.cuervolu.Glaux.SingleInstance";
const METHOD: &str = "SecondaryLaunch";

/// Receives launches forwarded by later processes.
struct Listener {
    launches: Sender<SecondaryLaunch>,
}

#[zbus::interface(name = "dev.cuervolu.Glaux.SingleInstance")]
impl Listener {
    fn secondary_launch(&self, args: Vec<String>, cwd: String) {
        let launch = SecondaryLaunch {
            args,
            cwd: PathBuf::from(cwd),
        };
        // The receiver is gone only while Glaux is shutting down.
        let _ = self.launches.send(launch);
    }
}

/// Claims the bus name, or forwards `launch` to its owner.
pub(super) fn claim(
    launch: &SecondaryLaunch,
    launches: Sender<SecondaryLaunch>,
) -> Claim<Connection> {
    let connection = match Builder::session()
        .and_then(|builder| builder.serve_at(OBJECT_PATH, Listener { launches }))
        .and_then(Builder::build)
    {
        Ok(connection) => connection,
        Err(err) => {
            return Claim::Unavailable(
                anyhow::Error::new(err).context("failed to connect to the session D-Bus"),
            );
        }
    };
    // The name is requested with exactly `DoNotQueue` rather than through
    // `Builder::name`. zbus's default flags are `AllowReplacement |
    // ReplaceExisting | DoNotQueue`, so with the defaults a later launch
    // silently takes the name from the running instance instead of failing
    // with `NameTaken`, and both processes start as owners. See
    // https://github.com/z-galaxy/zbus/pull/1431 and
    // https://github.com/z-galaxy/zbus/issues/1413.
    //
    // After updating zbus, check that requesting an owned name with these
    // flags still fails with `NameTaken` (e.g. launch Glaux twice).
    match connection.request_name_with_flags(BUS_NAME, RequestNameFlags::DoNotQueue.into()) {
        Ok(_) => Claim::Owner(connection),
        Err(zbus::Error::NameTaken) => match forward(launch) {
            Ok(()) => Claim::Forwarded,
            Err(err) => Claim::ForwardFailed(err),
        },
        Err(err) => Claim::Unavailable(
            anyhow::Error::new(err).context("failed to claim the single-instance bus name"),
        ),
    }
}

/// Calls the running owner with `launch`.
fn forward(launch: &SecondaryLaunch) -> anyhow::Result<()> {
    let connection = Connection::session().context("failed to connect to the session D-Bus")?;
    let cwd = launch.cwd.to_string_lossy();
    connection
        .call_method(
            Some(BUS_NAME),
            OBJECT_PATH,
            Some(INTERFACE),
            METHOD,
            &(&launch.args, cwd.as_ref()),
        )
        .with_context(|| format!("failed to call {METHOD} on {BUS_NAME}"))?;
    Ok(())
}
