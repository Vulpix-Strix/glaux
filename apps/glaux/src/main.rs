mod app;
mod app_dirs;
mod cartridge;
mod cartridge_fs;
mod cartridge_template;
mod clock;
mod config_file;
mod desktop;
mod editor_settings_file;
mod licenses;
mod logging;
mod lsp;
mod platform;
mod rom;
#[cfg(test)]
mod scratch_dir;
mod sprite_import;
mod system;
mod tooling;
mod ui_text;

use std::env::consts::{ARCH, OS};

use desktop::single_instance::{self, SecondaryLaunch, Startup};

// The profiling workload test starts its own guard, and a second one would
// install a second global allocator under `hotpath-alloc`.
#[cfg_attr(not(test), hotpath::main)]
fn main() -> anyhow::Result<()> {
    if let Some(command) = tooling::Cli::parse_env().command {
        tooling::run(command)?;
        return Ok(());
    }

    // Decided before logging starts so a forwarded launch does not create a
    // session log of its own and rotate out the primary's logs.
    let startup = single_instance::start(&SecondaryLaunch::current());

    let data_dir = app_dirs::data_dir();
    // Held until `main` returns so the file log is flushed on exit.
    let mut logging = match startup {
        Startup::Primary(_) => logging::init(data_dir.as_deref()),
        Startup::Forwarded | Startup::ForwardFailed(_) => logging::init_stderr(),
    };

    let instance = match startup {
        Startup::Primary(instance) => instance,
        Startup::Forwarded => {
            tracing::info!("launch forwarded to the running Glaux instance");
            return Ok(());
        }
        Startup::ForwardFailed(err) => {
            let err =
                err.context("Glaux is already running but the launch could not be handed to it");
            tracing::error!(error = %format!("{err:#}"), "launch not forwarded");
            return Err(err);
        }
    };
    if let Some(err) = &instance.degraded {
        tracing::warn!(error = %format!("{err:#}"), "single-instance launches unavailable, running without them");
    }

    tracing::info!(
        version = env!("CARGO_PKG_VERSION"),
        os = OS,
        arch = ARCH,
        data_dir = ?data_dir,
        "Glaux starting"
    );
    let session = platform::detect();
    if cfg!(target_os = "linux") {
        tracing::info!(session = %session, "display session detected");
    }

    let config_dir = app_dirs::config_dir();
    if config_dir.is_none() {
        tracing::warn!(
            "no config directory is available, window state and editor settings will not be kept"
        );
    }

    let result = app::run(app::Launch {
        session,
        config_dir: config_dir.as_deref(),
        data_dir: data_dir.as_deref(),
        instance: &instance,
        diagnostics: logging.take_diagnostics(),
    });
    match &result {
        Ok(()) => tracing::info!("clean shutdown"),
        Err(err) => tracing::error!(error = %format!("{err:#}"), "Glaux stopped with an error"),
    }
    result
}
