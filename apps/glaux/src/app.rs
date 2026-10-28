//! The running application. [`run`] builds the machine, the system and the
//! window, and [`App`] moves frames between them until the system quits.
//!
//! Dialogs, links, the menu bar and the saved window state come from
//! [`crate::desktop`], which does not depend on this module.

#![warn(unreachable_pub)]

mod audio;
mod host_work;
mod menu;
mod window;

use std::path::Path;
use std::time::Instant;

use glaux_host_raylib::RaylibHost;
use glaux_machine::framebuffer::Framebuffer;
use glaux_machine::machine::Machine;
use glaux_machine::psg::SAMPLES_PER_TICK;
use glaux_machine::system_rom::SystemRom;
use glaux_video::IndexedFrame;

use crate::cartridge_fs::CartridgeStorage;
use crate::clock::TickClock;
use crate::desktop::file_picker::FilePicker;
use crate::desktop::single_instance::PrimaryInstance;
use crate::logging::Diagnostics;
use crate::platform::SessionKind;
use crate::system::{CartridgeVideoDefaults, System, SystemInfo};
use crate::{editor_settings_file, rom};
use audio::AudioSink;
use host_work::{HostClipboard, serve_host_requests};
use menu::Menu;
use window::WindowLifecycle;

/// What the process learned before the application starts.
pub(crate) struct Launch<'a> {
    pub session: SessionKind,
    pub config_dir: Option<&'a Path>,
    pub data_dir: Option<&'a Path>,
    pub instance: &'a PrimaryInstance,
    pub diagnostics: Option<Diagnostics>,
}

/// Runs Glaux until the system confirms it should quit.
pub(crate) fn run(launch: Launch<'_>) -> anyhow::Result<()> {
    let Launch {
        session,
        config_dir,
        data_dir,
        instance,
        diagnostics,
    } = launch;
    let machine = Machine::new(rom::load()?);
    let system = new_system(&machine, config_dir, data_dir, diagnostics);

    // `App` borrows the window and the audio device instead of owning them.
    // Locals drop in reverse order, so the app (with its audio stream and
    // menu bar) goes first, then the device, then the window.
    let (mut host, window) = WindowLifecycle::open(session, config_dir)?;
    let menu = Menu::install();
    let audio_device = audio::open_device();
    let app = App::new(
        machine,
        Shared {
            session,
            config_dir,
            instance,
        },
        &mut host,
        window,
        menu,
        AudioSink::open(audio_device.as_ref()),
    );
    app.run(system)
}

/// The system, set up from the machine's System ROM and what the process
/// found on disk.
fn new_system(
    machine: &Machine,
    config_dir: Option<&Path>,
    data_dir: Option<&Path>,
    diagnostics: Option<Diagnostics>,
) -> System {
    let rom = machine.system_rom();
    let system = System::new(
        SystemInfo {
            rom_version: rom.version().clone(),
        },
        rom.sounds().clone(),
        CartridgeVideoDefaults::from_rom(rom),
        CartridgeStorage::open(data_dir),
        rom.font(),
        rom.palette().clone(),
    )
    .with_editor_settings(
        config_dir
            .map(editor_settings_file::load)
            .unwrap_or_default(),
    );
    match diagnostics {
        Some(diagnostics) => system.with_diagnostics(diagnostics),
        None => system,
    }
}

/// What the process learned about where it runs.
struct Shared<'a> {
    session: SessionKind,
    config_dir: Option<&'a Path>,
    instance: &'a PrimaryInstance,
}

/// The machine and the host resources a frame needs. The system itself is
/// passed through each frame by value.
struct App<'a> {
    // The menu bar hangs off the window, so it is dropped first.
    menu: Menu,
    host: &'a mut RaylibHost,
    window: WindowLifecycle,
    audio: AudioSink<'a>,
    machine: Machine,
    /// A copy of the System ROM, so the system can draw into the machine's
    /// framebuffer with data from the ROM without borrowing both at once.
    rom: SystemRom,
    picker: FilePicker,
    instance: &'a PrimaryInstance,
    session: SessionKind,
    config_dir: Option<&'a Path>,
    /// Presentation follows the host refresh rate while the system advances
    /// at the machine's fixed tick rate.
    clock: TickClock,
    last: Instant,
    /// Input polled on frames where no tick is due. It waits for the next
    /// tick instead of being dropped.
    pending: Vec<glaux_input::InputEvent>,
    /// The frame as presented: the machine's framebuffer with the pointer
    /// drawn over it, so the pointer follows the host every frame without
    /// ever entering the machine's framebuffer.
    presented: Framebuffer,
    samples: [i16; SAMPLES_PER_TICK],
}

impl<'a> App<'a> {
    fn new(
        machine: Machine,
        shared: Shared<'a>,
        host: &'a mut RaylibHost,
        window: WindowLifecycle,
        menu: Menu,
        audio: AudioSink<'a>,
    ) -> Self {
        let rom = machine.system_rom().clone();
        Self {
            menu,
            host,
            window,
            audio,
            machine,
            rom,
            picker: FilePicker::new(),
            instance: shared.instance,
            session: shared.session,
            config_dir: shared.config_dir,
            clock: TickClock::new(),
            last: Instant::now(),
            pending: Vec::new(),
            presented: Framebuffer::new(),
            samples: [0; SAMPLES_PER_TICK],
        }
    }

    /// Runs frames until the system confirms it should quit, then saves the
    /// window state.
    fn run(mut self, mut system: System) -> anyhow::Result<()> {
        while let Some(next) = self.frame(system)? {
            system = next;
        }
        self.window.finish(self.config_dir);
        Ok(())
    }

    /// One pass of the loop, which shows the current state before it
    /// advances it. Returns the system for the next frame, or `None` once
    /// quitting is confirmed.
    #[hotpath::measure(label = "app::frame", impl_type = "App")]
    fn frame(&mut self, mut system: System) -> anyhow::Result<Option<System>> {
        // Closing the window only asks: the system may first ask what to do
        // with unsaved edits, and quits once they are saved or discarded.
        if self.host.should_close() {
            system.request_exit();
        }
        if system.exit_confirmed() {
            return Ok(None);
        }
        self.window.observe(self.host);
        self.focus_for_forwarded_launches();
        self.present(&system)?;
        self.pending
            .extend(self.host.poll_input(system.input_mode()));
        system = self.menu.run_commands(system, &mut self.pending);
        system = self.advance(system);
        self.audio.pump();
        self.menu.sync(&system);
        if let Some(picked) = self.picker.poll() {
            system.file_picked(picked);
        }
        Ok(Some(system))
    }

    /// Brings the window forward for each launch another process forwarded
    /// here.
    fn focus_for_forwarded_launches(&mut self) {
        for launch in self.instance.drain() {
            tracing::info!(args = ?launch.args, cwd = %launch.cwd.display(), "another launch was forwarded here");
            // Wayland compositors do not let clients take focus on their own.
            if self.session != SessionKind::Wayland {
                self.host.focus();
            }
        }
    }

    /// Draws the system into the machine's framebuffer and shows it with the
    /// pointer on top.
    #[hotpath::measure(label = "app::present", impl_type = "App")]
    fn present(&mut self, system: &System) -> anyhow::Result<()> {
        let pointer = self.host.pointer_position();
        system.render(self.machine.framebuffer_mut(), &self.rom, pointer);
        self.presented
            .as_bytes_mut()
            .copy_from_slice(self.machine.framebuffer().as_bytes());
        system.draw_cursor(&mut self.presented, &self.rom, pointer);
        self.host.present(&IndexedFrame::new(
            &self.presented,
            system.frame_palette(&self.rom),
        ))?;
        self.window.after_present(self.host);
        Ok(())
    }

    /// Runs the ticks due since the last frame. After each tick it serves the
    /// system's host requests and queues the audio the tick produced.
    #[hotpath::measure(label = "app::advance", impl_type = "App")]
    fn advance(&mut self, mut system: System) -> System {
        let now = Instant::now();
        let ticks = self.clock.advance(now - self.last);
        self.last = now;
        for _ in 0..ticks {
            system = system.update_with(&self.pending, &mut HostClipboard(self.host));
            self.pending.clear();
            serve_host_requests(&mut system, &mut self.picker, self.config_dir);
            system.audio(self.machine.psg_mut());
            self.machine.psg_mut().render(&mut self.samples);
            self.audio.queue(&self.samples);
        }
        system
    }
}
