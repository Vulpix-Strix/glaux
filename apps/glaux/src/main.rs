mod app_dirs;
mod cartridge;
mod cartridge_fs;
mod clock;
mod desktop;
mod logging;
mod platform;
mod rom;
mod system;
mod tooling;

use std::env::consts::{ARCH, OS};
use std::path::Path;
use std::time::Instant;

use cartridge_fs::CartridgeStorage;
use clock::TickClock;
use desktop::single_instance::{self, PrimaryInstance, SecondaryLaunch, Startup};
use desktop::window_state::{self, WindowSnapshot, WindowTracker};
use glaux_host_raylib::{AudioDevice, AudioOutput, Presentation, RaylibHost};
use glaux_machine::machine::Machine;
use glaux_machine::psg::{SAMPLE_RATE, SAMPLES_PER_TICK};
use glaux_video::{HEIGHT, IndexedFrame, WIDTH};
use platform::SessionKind;
use system::{System, SystemInfo};

/// Window size, as a multiple of the logical resolution, when no saved
/// state applies.
const WINDOW_SCALE: i32 = 3;

/// How the host draws frames to the window.
const PRESENTATION: Presentation = Presentation::Crt;

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
    let _logging = match startup {
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
        tracing::warn!("no config directory is available, window state will not be kept");
    }

    let result = run(
        session,
        config_dir.as_deref(),
        data_dir.as_deref(),
        &instance,
    );
    match &result {
        Ok(()) => tracing::info!("clean shutdown"),
        Err(err) => tracing::error!(error = %format!("{err:#}"), "Glaux stopped with an error"),
    }
    result
}

fn run(
    session: SessionKind,
    config_dir: Option<&Path>,
    data_dir: Option<&Path>,
    instance: &PrimaryInstance,
) -> anyhow::Result<()> {
    let cartridges = CartridgeStorage::open(data_dir);

    let mut machine = Machine::new(rom::load()?);
    // The system draws into the machine's framebuffer with data from the
    // machine's ROM; a one-time copy of the ROM avoids borrowing both at once.
    let rom = machine.system_rom().clone();
    let mut system = System::new(
        SystemInfo {
            rom_version: rom.version().clone(),
        },
        rom.sounds().clone(),
        cartridges,
    );

    let saved = config_dir.and_then(window_state::load);
    let size = saved.map_or(
        (WIDTH as i32 * WINDOW_SCALE, HEIGHT as i32 * WINDOW_SCALE),
        |state| (state.width, state.height),
    );
    let mut host = RaylibHost::new("Glaux", size, PRESENTATION)?;
    let placement = window_state::plan(session, saved.as_ref(), &host.monitors());
    host.place(placement);
    if saved.is_some_and(|state| state.maximized) {
        host.maximize();
    }
    let mut tracker = WindowTracker::new(session, saved.as_ref(), placement);

    // Declared before the output that borrows it, so it is dropped after.
    let audio_device = AudioDevice::open()
        .inspect_err(|err| tracing::warn!(error = %err, "audio unavailable, running silent"))
        .ok();
    let mut audio = audio_device.as_ref().and_then(|device| {
        device
            .output(SAMPLE_RATE)
            .inspect_err(|err| tracing::warn!(error = %err, "audio unavailable, running silent"))
            .ok()
    });
    let mut samples = [0; SAMPLES_PER_TICK];

    // Presentation follows the host refresh rate while the system advances
    // at the machine's fixed tick rate. Input polled on frames where no tick
    // is due waits for the next tick instead of being dropped.
    let mut clock = TickClock::new();
    let mut pending = Vec::new();
    let mut last = Instant::now();
    // Render before updating so the initial state is the first frame shown.
    while !host.should_close() {
        tracker.observe(snapshot(&host, &tracker));
        for launch in instance.drain() {
            tracing::info!(args = ?launch.args, cwd = %launch.cwd.display(), "another launch was forwarded here");
            // Wayland compositors do not let clients take focus on their own.
            if session != SessionKind::Wayland {
                host.focus();
            }
        }

        system.render(machine.framebuffer_mut(), &rom);
        host.present(&IndexedFrame::new(machine.framebuffer(), rom.palette()))?;
        pending.extend(host.poll_input(system.input_mode()));

        let now = Instant::now();
        let ticks = clock.advance(now - last);
        last = now;
        for _ in 0..ticks {
            system = system.update(&pending);
            pending.clear();
            system.audio(machine.psg_mut());
            machine.psg_mut().render(&mut samples);
            if let Some(output) = &mut audio {
                output.queue(&samples);
            }
        }
        pump_audio(&mut audio);
    }

    if let (Some(dir), Some(state)) = (config_dir, tracker.finish()) {
        match window_state::save(dir, &state) {
            Ok(()) => tracing::debug!(?state, "window state saved"),
            Err(err) => {
                tracing::warn!(error = %format!("{err:#}"), "failed to save the window state")
            }
        }
    }
    Ok(())
}

/// Feeds the device the samples queued so far. A failed stream is closed
/// and the rest of the session runs silent.
fn pump_audio(audio: &mut Option<AudioOutput<'_>>) {
    if let Some(output) = audio
        && let Err(err) = output.pump()
    {
        tracing::warn!(error = %err, "audio stream failed, running silent");
        *audio = None;
    }
}

/// The window's current state, with its position only where `tracker`
/// follows positions.
fn snapshot(host: &RaylibHost, tracker: &WindowTracker) -> WindowSnapshot {
    WindowSnapshot {
        size: host.window_size(),
        position: tracker.tracks_positions().then(|| host.window_position()),
        maximized: host.is_maximized(),
        minimized: host.is_minimized(),
    }
}
