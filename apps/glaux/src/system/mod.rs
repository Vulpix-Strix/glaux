//! System software: the boot sequence, the shell that follows it, and the
//! cartridge runtime the shell starts.
//!
//! The system advances one step per logical tick and redraws the whole
//! framebuffer from its current state on every frame.

mod audio;
mod boot;
mod line_editor;
mod pause;
mod running;
mod sfx;
mod shell;

pub use boot::Boot;
pub use running::CartridgeVideoDefaults;
pub use shell::Shell;

use glaux_input::{InputEvent, InputMode};
use glaux_luau::RuntimeError;
use glaux_machine::framebuffer::{Framebuffer, HEIGHT, WIDTH};
use glaux_machine::machine::{GENERATION, NAME};
use glaux_machine::palette::Palette;
use glaux_machine::psg::Psg;
use glaux_machine::system_rom::{SystemRom, SystemSounds};
use glaux_video::draw::{clear, draw_text, fill_rect};
use pause::PauseOverlay;
use running::{FailedStart, RunningCartridge, StartError};
use semver::Version;
use sfx::SystemSound;
use shell::{CartridgeSlot, ShellRequest, cartridge_identity};

use crate::cartridge::LoadedCartridge;
use crate::cartridge_fs::CartridgeStorage;

// System UI colors, as indices into the system palette (Zughy 32).
const BACKGROUND: u8 = 11;
const TEXT: u8 = 16;
const DIM: u8 = 14;
const PALE: u8 = 15;
const OK: u8 = 8;
const ACCENT: u8 = 18;
const ERROR: u8 = 28;
const TRACE: u8 = 13;
const WARN: u8 = 26;

/// Where the system is in its own navigation. Each variant owns exactly the
/// state that phase needs, and transitions consume one variant to build the
/// next.
#[derive(Debug)]
pub enum SystemState {
    /// The boot sequence. The cartridge storage waits here for the shell.
    Boot {
        boot: Boot,
        cartridges: CartridgeStorage,
    },
    /// The shell is the active UI, with or without a loaded cartridge.
    Shell {
        shell: Box<Shell>,
        loaded: Option<LoadedCartridge>,
    },
    /// A cartridge runs and receives the logical ticks. The shell keeps its
    /// state, collecting logged output, but takes no input. The break key
    /// pauses the cartridge.
    Running {
        shell: Box<Shell>,
        running: RunningCartridge,
    },
    /// A cartridge whose VM is kept alive but receives no ticks, so its
    /// state, last frame and logical time stay as they were.
    Paused {
        shell: Box<Shell>,
        running: RunningCartridge,
        view: PausedView,
    },
}

/// What the system shows while a cartridge is paused.
#[derive(Debug)]
pub enum PausedView {
    /// The cartridge's last frame under the system's pause overlay. Break
    /// resumes the cartridge and Enter opens the shell.
    Game(PauseOverlay),
    /// The shell, which can resume or stop the cartridge.
    Shell,
}

/// Shown when the shell opens over a paused cartridge.
const PAUSED_SHELL_HINT: &str = "Type 'resume' to continue or 'stop' to end.";

/// Facts about the running system that the shell reports.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SystemInfo {
    /// Version of the System ROM the system runs from.
    pub rom_version: Version,
}

/// `GLAUX G1  SYSTEM ROM x.y.z`, shared by the boot header and `VERSION`.
fn identity_line(rom_version: &Version) -> String {
    format!("{NAME} {GENERATION}  SYSTEM ROM {rom_version}")
}

/// Why a cartridge's VM was disposed of.
enum StopReason {
    /// The user stopped the paused cartridge.
    Stopped,
    Failed(RuntimeError),
}

/// What drives the PSG.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AudioSource {
    Silent,
    /// System ROM sound effects.
    System,
    /// The shell's diagnostic voices.
    Shell,
    /// The running cartridge's audio.
    Cartridge,
}

/// The system UI state machine.
#[derive(Debug)]
pub struct System {
    state: SystemState,
    info: SystemInfo,
    /// The source that last wrote the PSG.
    audio_source: AudioSource,
    sound: SystemSound,
    /// Video state for the assets a started cartridge does not supply.
    video_defaults: CartridgeVideoDefaults,
}

impl System {
    /// Starts at frame 0 of the boot sequence, which plays the System ROM's
    /// `sounds`. The shell that follows navigates `cartridges`, and
    /// cartridges it starts fall back to `video_defaults`.
    pub fn new(
        info: SystemInfo,
        sounds: SystemSounds,
        video_defaults: CartridgeVideoDefaults,
        cartridges: CartridgeStorage,
    ) -> Self {
        Self {
            state: SystemState::Boot {
                boot: Boot::new(),
                cartridges,
            },
            info,
            audio_source: AudioSource::Silent,
            sound: SystemSound::new(sounds),
            video_defaults,
        }
    }

    #[cfg(test)]
    pub fn state(&self) -> &SystemState {
        &self.state
    }

    /// The kind of input the current state consumes: game buttons while a
    /// cartridge runs, text everywhere else, including while one is paused.
    pub fn input_mode(&self) -> InputMode {
        match self.state {
            SystemState::Running { .. } => InputMode::Game,
            SystemState::Boot { .. } | SystemState::Shell { .. } | SystemState::Paused { .. } => {
                InputMode::Text
            }
        }
    }

    /// Advances the system by one logical tick, applying the tick's `input`.
    ///
    /// Input is ignored during the boot sequence. While a cartridge runs,
    /// [`InputEvent::Break`] pauses it and button events reach its `btn` and
    /// `btnp`; anything else is ignored. On the pause screen, Break resumes
    /// the cartridge and Enter opens the shell with it still paused.
    pub fn update(self, input: &[InputEvent]) -> Self {
        let Self {
            state,
            info,
            audio_source,
            mut sound,
            video_defaults,
        } = self;
        let state = match state {
            SystemState::Boot {
                mut boot,
                cartridges,
            } => {
                if let Some(sfx) = boot.tick() {
                    sound.play(sfx);
                }
                if boot.is_finished() {
                    tracing::info!("boot finished, entering shell");
                    sound.stop();
                    SystemState::Shell {
                        shell: Box::new(Shell::new(info.clone(), cartridges)),
                        loaded: None,
                    }
                } else {
                    SystemState::Boot { boot, cartridges }
                }
            }
            SystemState::Shell { shell, loaded } => {
                shell_tick(shell, loaded, input, &video_defaults)
            }
            SystemState::Running { shell, running } => running_tick(shell, running, input),
            SystemState::Paused {
                shell,
                running,
                view: PausedView::Game(overlay),
            } => paused_game_tick(shell, running, overlay, input),
            SystemState::Paused {
                shell,
                running,
                view: PausedView::Shell,
            } => paused_shell_tick(shell, running, input),
        };
        sound.tick();
        Self {
            state,
            info,
            audio_source,
            sound,
            video_defaults,
        }
    }

    /// Writes the system's sound for the current state into `psg`.
    ///
    /// The boot sequence plays System ROM sound effects, the shell's
    /// diagnostic voices play while the shell is the active view, and a
    /// running cartridge plays its own audio. The pause screen is silent.
    /// Whenever the source changes, `psg` is reset first, so the System ROM,
    /// the shell and a cartridge never share oscillator or envelope state,
    /// every run starts from the same PSG state, and pausing or stopping a
    /// cartridge silences it at once.
    ///
    /// For the shell, each channel plays the TONE oscillator, the ENV
    /// envelope and the GATE state. A gate change reaches the PSG only when
    /// the PSG's gate differs, so writing the same state every tick neither
    /// retriggers nor restarts an envelope.
    ///
    /// Call once after each [`update`](Self::update): a cartridge's envelope
    /// and gate writes reach the PSG once each.
    pub fn audio(&mut self, psg: &mut Psg) {
        let source = match &self.state {
            SystemState::Shell { .. }
            | SystemState::Paused {
                view: PausedView::Shell,
                ..
            } => AudioSource::Shell,
            SystemState::Running { .. } => AudioSource::Cartridge,
            SystemState::Boot { .. } => AudioSource::System,
            SystemState::Paused {
                view: PausedView::Game(_),
                ..
            } => AudioSource::Silent,
        };
        let fresh = source != self.audio_source;
        if fresh {
            *psg = Psg::new();
            self.audio_source = source;
        }
        match &mut self.state {
            SystemState::Shell { shell, .. }
            | SystemState::Paused {
                shell,
                view: PausedView::Shell,
                ..
            } => shell_audio(shell, psg),
            SystemState::Running { running, .. } => running.apply_audio(psg, fresh),
            SystemState::Boot { .. } => self.sound.apply(psg, fresh),
            SystemState::Paused {
                view: PausedView::Game(_),
                ..
            } => {}
        }
    }

    /// Redraws the whole framebuffer for the current state using the ROM's
    /// font.
    /// The master palette the frame drawn by [`render`](Self::render)
    /// resolves to: the cartridge's own while its frame is on screen, the
    /// System ROM palette otherwise.
    pub fn frame_palette<'a>(&'a self, rom: &'a SystemRom) -> &'a Palette {
        match &self.state {
            SystemState::Running { running, .. }
            | SystemState::Paused {
                running,
                view: PausedView::Game(_),
                ..
            } => running.palette(),
            SystemState::Boot { .. }
            | SystemState::Shell { .. }
            | SystemState::Paused {
                view: PausedView::Shell,
                ..
            } => rom.palette(),
        }
    }

    pub fn render(&self, framebuffer: &mut Framebuffer, rom: &SystemRom) {
        let font = rom.font();
        let metrics = font.metrics();
        let (cell_w, cell_h) = (
            i32::from(metrics.cell_width()),
            i32::from(metrics.cell_height()),
        );

        let lines = match &self.state {
            SystemState::Boot { boot, .. } if boot.stage() == boot::BootStage::Intro => {
                boot::render_intro(framebuffer, font, rom.version(), boot.frame());
                return;
            }
            SystemState::Boot { boot, .. } => boot::lines(boot.stage(), rom),
            SystemState::Shell { shell, .. }
            | SystemState::Paused {
                shell,
                view: PausedView::Shell,
                ..
            } => shell.lines(text_columns(cell_w), text_rows(cell_h)),
            SystemState::Running { running, .. } => {
                running.render(framebuffer, font);
                return;
            }
            SystemState::Paused {
                running,
                view: PausedView::Game(overlay),
                ..
            } => {
                running.render(framebuffer, font);
                overlay.render(framebuffer, running.palette(), rom.palette());
                return;
            }
        };

        clear(framebuffer, BACKGROUND);
        for line in &lines {
            let (x, y) = line.origin(cell_w, cell_h);
            if let Some(background) = line.background {
                let cells = i32::try_from(line.text.chars().count()).unwrap_or(i32::MAX);
                fill_rect(
                    framebuffer,
                    x,
                    y,
                    cells.saturating_mul(cell_w),
                    cell_h,
                    background,
                );
            }
            draw_text(framebuffer, font, &line.text, x, y, line.color);
        }
    }
}

/// Writes the shell's diagnostic voices into `psg`.
fn shell_audio(shell: &Shell, psg: &mut Psg) {
    for (index, voice) in shell.voices().into_iter().enumerate() {
        let Some(channel) = psg.channel_mut(index) else {
            unreachable!("the shell holds one voice per PSG channel");
        };
        channel.set_adsr(voice.adsr);
        let Some(tone) = voice.tone else {
            channel.set_volume(0);
            channel.gate_off();
            continue;
        };
        channel.set_waveform(tone.waveform);
        channel.set_frequency(tone.frequency);
        channel.set_volume(tone.volume);
        match (voice.gate, channel.gate()) {
            (true, false) => channel.gate_on(),
            (false, true) => channel.gate_off(),
            _ => {}
        }
    }
}

/// Feeds `input` to the shell and carries out the cartridge requests its
/// commands make.
///
/// A started cartridge does not take input, so events that follow a
/// successful `run` in the same tick are dropped.
fn shell_tick(
    mut shell: Box<Shell>,
    mut loaded: Option<LoadedCartridge>,
    input: &[InputEvent],
    video_defaults: &CartridgeVideoDefaults,
) -> SystemState {
    // Ticking before the input lets this tick's input restart the blink.
    shell.tick();
    // Input is deliberately not logged: user input stays out of
    // application logs.
    for (index, &event) in input.iter().enumerate() {
        match shell.handle_input(event, CartridgeSlot::loaded(loaded.as_ref())) {
            None => {}
            Some(ShellRequest::Load { cartridge, origin }) => {
                let cartridge = loaded.insert(*cartridge);
                shell.announce_loaded(cartridge.manifest(), origin);
            }
            Some(ShellRequest::Unload) => {
                let Some(cartridge) = loaded.take() else {
                    unreachable!("UNLOAD was requested without a loaded cartridge");
                };
                let identity = cartridge_identity(cartridge.manifest());
                tracing::info!(cartridge = %identity, "cartridge unloaded");
                shell.report(format!("Unloaded {identity}"));
            }
            Some(ShellRequest::Run) => {
                // `run` only asks to start when a cartridge is loaded.
                let Some(cartridge) = loaded else {
                    unreachable!("RUN was requested without a loaded cartridge");
                };
                return match start(shell, cartridge, video_defaults) {
                    SystemState::Shell { shell, loaded } => {
                        shell_tick(shell, loaded, &input[index + 1..], video_defaults)
                    }
                    state => state,
                };
            }
            Some(_request @ (ShellRequest::Resume | ShellRequest::Stop)) => {
                unreachable!("{_request:?} was requested without a paused cartridge")
            }
        }
    }
    SystemState::Shell { shell, loaded }
}

/// Starts `cartridge`, reporting its start or failure and what it logged.
/// A cartridge that fails to start stays loaded in the shell.
fn start(
    mut shell: Box<Shell>,
    cartridge: LoadedCartridge,
    video_defaults: &CartridgeVideoDefaults,
) -> SystemState {
    let identity = cartridge_identity(cartridge.manifest());
    match RunningCartridge::start(cartridge, video_defaults) {
        Ok(mut running) => {
            tracing::info!(cartridge = %identity, "cartridge started");
            shell.report(format!("Running {identity}"));
            shell.log(running.take_output());
            SystemState::Running { shell, running }
        }
        Err(failed) => {
            let FailedStart { cartridge, error } = *failed;
            match error {
                StartError::Cartridge(err) => shell.report_error(err.to_string()),
                StartError::Video(err) => shell.report_error(err.to_string()),
                StartError::Script(failure) => {
                    tracing::info!(
                        cartridge = %identity,
                        error = %failure.error,
                        "cartridge failed to start"
                    );
                    shell.log(failure.output);
                    shell.report_error(failure.error.to_string());
                }
            }
            SystemState::Shell {
                shell,
                loaded: Some(cartridge),
            }
        }
    }
}

/// Advances the running cartridge by one tick, or pauses it on a break. A
/// break wins over any button events in the same tick, which are dropped.
fn running_tick(
    mut shell: Box<Shell>,
    mut running: RunningCartridge,
    input: &[InputEvent],
) -> SystemState {
    if input.contains(&InputEvent::Break) {
        return pause(shell, running);
    }
    match running.tick(input) {
        Ok(()) => {
            shell.log(running.take_output());
            SystemState::Running { shell, running }
        }
        Err(err) => stop(shell, running, StopReason::Failed(err)),
    }
}

/// Keeps the cartridge's VM without ticking it and shows the pause screen.
fn pause(mut shell: Box<Shell>, mut running: RunningCartridge) -> SystemState {
    shell.log(running.take_output());
    let identity = cartridge_identity(running.cartridge().manifest());
    tracing::info!(cartridge = %identity, "cartridge paused");
    shell.report(format!("Paused {identity}"));
    SystemState::Paused {
        shell,
        running,
        view: PausedView::Game(PauseOverlay::new()),
    }
}

/// Handles the pause screen: Break resumes the cartridge and Enter opens the
/// shell, which takes the events after it. Other events are ignored, and
/// only the overlay's own animation advances.
fn paused_game_tick(
    mut shell: Box<Shell>,
    running: RunningCartridge,
    mut overlay: PauseOverlay,
    input: &[InputEvent],
) -> SystemState {
    for (index, &event) in input.iter().enumerate() {
        match event {
            InputEvent::Break => return resume(shell, running),
            InputEvent::Enter => {
                let identity = cartridge_identity(running.cartridge().manifest());
                tracing::info!(cartridge = %identity, "shell opened with the cartridge paused");
                shell.report(PAUSED_SHELL_HINT);
                return paused_shell_tick(shell, running, &input[index + 1..]);
            }
            _ => {}
        }
    }
    overlay.tick();
    SystemState::Paused {
        shell,
        running,
        view: PausedView::Game(overlay),
    }
}

/// Feeds `input` to the shell while the cartridge stays paused, and carries
/// out a `resume` or `stop`. Events after either in the same tick are
/// dropped.
fn paused_shell_tick(
    mut shell: Box<Shell>,
    running: RunningCartridge,
    input: &[InputEvent],
) -> SystemState {
    shell.tick();
    for &event in input {
        match shell.handle_input(event, CartridgeSlot::Paused(running.cartridge())) {
            None => {}
            Some(ShellRequest::Resume) => return resume(shell, running),
            Some(ShellRequest::Stop) => return stop(shell, running, StopReason::Stopped),
            // `load`, `unload` and `run` refuse to replace a paused cartridge.
            Some(
                _request @ (ShellRequest::Load { .. } | ShellRequest::Unload | ShellRequest::Run),
            ) => {
                unreachable!("{_request:?} was requested while a cartridge is paused")
            }
        }
    }
    SystemState::Paused {
        shell,
        running,
        view: PausedView::Shell,
    }
}

/// Continues the paused cartridge with its VM as it was. Its buttons start
/// released, so nothing pressed or held around the pause reaches it as a
/// press; it is first ticked on the next update.
fn resume(mut shell: Box<Shell>, mut running: RunningCartridge) -> SystemState {
    running.reset_input();
    let identity = cartridge_identity(running.cartridge().manifest());
    tracing::info!(cartridge = %identity, "cartridge resumed");
    shell.report(format!("Resumed {identity}"));
    SystemState::Running { shell, running }
}

/// Disposes of the cartridge's VM and returns to the shell with the
/// cartridge still loaded. Every way a VM ends goes through here.
fn stop(mut shell: Box<Shell>, mut running: RunningCartridge, reason: StopReason) -> SystemState {
    shell.log(running.take_output());
    let identity = cartridge_identity(running.cartridge().manifest());
    let cartridge = running.stop();
    match reason {
        StopReason::Stopped => {
            tracing::info!(cartridge = %identity, "cartridge stopped");
            shell.report(format!("Stopped {identity}"));
        }
        StopReason::Failed(err) => {
            tracing::info!(cartridge = %identity, error = %err, "cartridge failed");
            shell.report_error(err.to_string());
        }
    }
    SystemState::Shell {
        shell,
        loaded: Some(cartridge),
    }
}

/// Character cells that fit across the screen inside the margin left by
/// [`Line::origin`].
fn text_columns(cell_w: i32) -> usize {
    let cells = WIDTH as i32 / cell_w.max(1);
    usize::try_from(cells - 2).unwrap_or(0)
}

/// Character rows that fit down the screen inside the margin left by
/// [`Line::origin`].
fn text_rows(cell_h: i32) -> usize {
    let cells = HEIGHT as i32 / cell_h.max(1);
    usize::try_from(cells - 2).unwrap_or(0)
}

/// One line of system text, positioned in character cells.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Line {
    col: i32,
    row: i32,
    text: String,
    color: u8,
    /// Fills the cells covered by the text before drawing it.
    background: Option<u8>,
}

impl Line {
    fn new(col: i32, row: i32, text: impl Into<String>, color: u8) -> Self {
        Self {
            col,
            row,
            text: text.into(),
            color,
            background: None,
        }
    }

    fn with_background(mut self, color: u8) -> Self {
        self.background = Some(color);
        self
    }

    /// Top-left pixel of the line, leaving a one-cell margin around the
    /// screen.
    fn origin(&self, cell_w: i32, cell_h: i32) -> (i32, i32) {
        ((self.col + 1) * cell_w, (self.row + 1) * cell_h)
    }
}

#[cfg(test)]
mod tests {
    use super::boot::BootStage;
    use super::*;
    use glaux_machine::font::{BitmapFont, FIRST_CHAR, FontMetrics, LAST_CHAR};
    use glaux_machine::palette::Palette;
    use glaux_machine::psg::{
        Adsr, CHANNELS, ENVELOPE_FULL, EnvelopeStage, SAMPLES_PER_TICK, Waveform,
    };
    use glaux_machine::system_rom::SystemSfx;
    use pretty_assertions::assert_eq;
    use semver::Version;

    const CELL_W: i32 = 4;
    const CELL_H: i32 = 5;

    /// A ROM whose font draws every printable character as a 3×3 block in a
    /// 4×5 cell.
    fn fake_rom() -> SystemRom {
        let mut font = BitmapFont::new(FontMetrics::new(3, 3, 4, 5).unwrap());
        for ch in FIRST_CHAR..=LAST_CHAR {
            font.insert(ch, &[0b111; 3]).unwrap();
        }
        SystemRom::new(
            Version::new(0, 1, 0),
            "(C) 2026 TEST".into(),
            Palette::provisional(),
            font,
        )
    }

    fn video_defaults() -> CartridgeVideoDefaults {
        CartridgeVideoDefaults::from_rom(&fake_rom())
    }

    fn info() -> SystemInfo {
        SystemInfo {
            rom_version: Version::new(0, 1, 0),
        }
    }

    fn cartridges() -> CartridgeStorage {
        CartridgeStorage::Unavailable
    }

    fn sounds() -> SystemSounds {
        crate::rom::sounds().unwrap()
    }

    fn new_system() -> System {
        System::new(info(), sounds(), video_defaults(), cartridges())
    }

    fn render(system: &System) -> Framebuffer {
        let mut fb = Framebuffer::new();
        system.render(&mut fb, &fake_rom());
        fb
    }

    fn boot_at(stage: BootStage) -> System {
        let mut system = new_system();
        while !matches!(system.state(), SystemState::Boot { boot, .. } if boot.stage() == stage) {
            system = system.update(&[]);
        }
        system
    }

    fn shell() -> System {
        let mut system = new_system();
        while matches!(system.state(), SystemState::Boot { .. }) {
            system = system.update(&[]);
        }
        system
    }

    fn assert_boot_frame(system: &System, frame: u32) {
        match system.state() {
            SystemState::Boot { boot, .. } => assert_eq!(boot, &Boot::at_frame(frame)),
            other => panic!("expected boot, got {other:?}"),
        }
    }

    /// The shell of a system whose shell is the active UI.
    fn shell_of(system: &System) -> &Shell {
        match system.state() {
            SystemState::Shell { shell, .. } => shell,
            other => panic!("expected the shell, got {other:?}"),
        }
    }

    /// Whether any pixel in text row `row` (in cells) differs from the
    /// background.
    fn row_has_ink(fb: &Framebuffer, row: i32) -> bool {
        let top = (row + 1) * CELL_H;
        (top..top + CELL_H).any(|y| (0..320).any(|x| fb.pixel(x, y) != Some(BACKGROUND)))
    }

    #[test]
    fn starts_at_boot_frame_zero() {
        match new_system().state() {
            SystemState::Boot { boot, .. } => {
                assert_eq!(boot.frame(), 0);
                assert_eq!(boot.stage(), BootStage::Intro);
            }
            other => panic!("expected boot, got {other:?}"),
        }
    }

    #[test]
    fn frame_n_is_reached_after_n_updates() {
        let mut system = new_system();
        for n in 1..=10 {
            system = system.update(&[]);
            assert_boot_frame(&system, n);
        }
    }

    #[test]
    fn boot_stages_advance_in_order_at_their_frames() {
        let mut system = new_system();
        let mut seen = vec![(0, BootStage::Intro)];
        let mut frame = 0;

        while let SystemState::Boot { boot, .. } = system.state() {
            let stage = boot.stage();
            if seen.last().map(|&(_, s)| s) != Some(stage) {
                seen.push((frame, stage));
            }
            system = system.update(&[]);
            frame += 1;
        }

        assert_eq!(
            seen,
            [
                (0, BootStage::Intro),
                (boot::HEADER_FRAME, BootStage::Header),
                (boot::VIDEO_FRAME, BootStage::Video),
                (boot::OBJ_FRAME, BootStage::Obj),
                (boot::AUDIO_FRAME, BootStage::Audio),
                (boot::READY_FRAME, BootStage::Ready),
            ]
        );
        assert_eq!(frame, boot::FINISH_FRAME);
    }

    #[test]
    fn switches_to_shell_exactly_at_finish_frame() {
        let mut system = new_system();
        for _ in 0..boot::FINISH_FRAME - 1 {
            system = system.update(&[]);
        }
        assert!(matches!(system.state(), SystemState::Boot { .. }));

        system = system.update(&[]);
        assert_eq!(shell_of(&system), &Shell::new(info(), cartridges()));

        let mut expected = Shell::new(info(), cartridges());
        for _ in 0..100 {
            system = system.update(&[]);
            expected.tick();
        }
        assert_eq!(shell_of(&system), &expected);
    }

    #[test]
    fn boot_header_renders_at_its_position() {
        let rom = fake_rom();
        let fb = render(&boot_at(BootStage::Header));

        let mut expected = Framebuffer::new();
        clear(&mut expected, BACKGROUND);
        for line in boot::lines(BootStage::Header, &rom) {
            draw_text(
                &mut expected,
                rom.font(),
                &line.text,
                (line.col + 1) * CELL_W,
                (line.row + 1) * CELL_H,
                line.color,
            );
        }
        assert_eq!(fb, expected);
        assert!(row_has_ink(&fb, 0));
    }

    #[test]
    fn intro_renders_through_the_intro_renderer() {
        let rom = fake_rom();
        for frame in [0, boot::INTRO_FRAMES / 2, boot::INTRO_FRAMES - 1] {
            let mut system = new_system();
            for _ in 0..frame {
                system = system.update(&[]);
            }
            let mut expected = Framebuffer::new();
            boot::render_intro(&mut expected, rom.font(), rom.version(), frame);
            assert_eq!(render(&system), expected, "frame {frame}");
        }
    }

    #[test]
    fn intro_hands_over_to_the_header_on_a_clean_screen() {
        let last_intro = render(&{
            let mut system = new_system();
            for _ in 0..boot::INTRO_FRAMES - 1 {
                system = system.update(&[]);
            }
            system
        });
        let mut blank = Framebuffer::new();
        clear(&mut blank, BACKGROUND);
        assert_eq!(last_intro, blank);
        assert!(row_has_ink(&render(&boot_at(BootStage::Header)), 0));
    }

    #[test]
    fn video_line_appears_at_video_stage() {
        let video_row = boot::lines(BootStage::Video, &fake_rom())
            .into_iter()
            .find(|line| line.text.starts_with("VIDEO"))
            .unwrap()
            .row;

        assert!(!row_has_ink(
            &render(&boot_at(BootStage::Header)),
            video_row
        ));
        assert!(row_has_ink(&render(&boot_at(BootStage::Video)), video_row));
    }

    #[test]
    fn shell_renders_prompt() {
        let prompt_row = Shell::new(info(), cartridges())
            .lines(text_columns(CELL_W), text_rows(CELL_H))
            .into_iter()
            .find(|line| line.text == "> ")
            .unwrap()
            .row;
        assert!(row_has_ink(&render(&shell()), prompt_row));
    }

    #[test]
    fn input_during_boot_is_ignored() {
        let mut system = new_system();
        for n in 1..=10 {
            system = system.update(&[InputEvent::Text('A'), InputEvent::Enter]);
            assert_boot_frame(&system, n);
        }
    }

    #[test]
    fn shell_works_without_cartridge_storage() {
        let mut system = shell();
        for line in ["pwd", "version"] {
            let mut events: Vec<InputEvent> = line.chars().map(InputEvent::Text).collect();
            events.push(InputEvent::Enter);
            system = system.update(&events);
        }

        // Banner, `> pwd`, its error, `> version`, its output.
        let fb = render(&system);
        let glyph = |row: i32| fb.pixel(CELL_W, (row + 1) * CELL_H);
        assert_eq!(glyph(2), Some(ERROR));
        assert_eq!(glyph(4), Some(TEXT));
    }

    #[test]
    fn typed_text_reaches_shell() {
        let mut system = shell();
        system = system.update(&[InputEvent::Text('H'), InputEvent::Text('I')]);
        system = system.update(&[InputEvent::Left { select: false }]);

        let mut expected = Shell::new(info(), cartridges());
        for event in [
            InputEvent::Text('H'),
            InputEvent::Text('I'),
            InputEvent::Left { select: false },
        ] {
            expected.handle_input(event, CartridgeSlot::Empty);
        }
        assert_eq!(shell_of(&system), &expected);
    }

    #[test]
    fn typed_text_renders_after_prompt() {
        let mut system = shell();
        let empty = render(&system);
        system = system.update(&[InputEvent::Text('A')]);
        let typed = render(&system);

        // Cell columns 2 and 3 follow the two-char prompt. The cursor starts
        // in column 2; typing puts 'A' there and moves the cursor to 3. Fake
        // glyphs leave column 3 of each cell empty, so only the cursor block
        // fills it.
        let y = 2 * CELL_H;
        let glyph = |fb: &Framebuffer, col: i32| fb.pixel((col + 1) * CELL_W, y);
        let block = |fb: &Framebuffer, col: i32| fb.pixel((col + 1) * CELL_W + 3, y);
        assert_eq!(
            (block(&empty, 2), block(&empty, 3)),
            (Some(TEXT), Some(BACKGROUND))
        );
        assert_eq!(glyph(&typed, 2), Some(TEXT));
        assert_eq!(
            (block(&typed, 2), block(&typed, 3)),
            (Some(BACKGROUND), Some(TEXT))
        );
    }

    #[test]
    fn cursor_cell_blinks_until_input_restarts_it() {
        // The cursor cell after the prompt on row 1, outside the 3×3 area
        // the fake font inks for every character, space included, so only
        // the block's fill shows there.
        let cell = |fb: &Framebuffer| {
            let (x, y) = (3 * CELL_W, 2 * CELL_H);
            (0..CELL_H)
                .flat_map(|dy| (0..CELL_W).map(move |dx| (dx, dy)))
                .filter(|&(dx, dy)| dx >= 3 || dy >= 3)
                .map(|(dx, dy)| fb.pixel(x + dx, y + dy))
                .collect::<Vec<_>>()
        };
        let all = |color| vec![Some(color); (CELL_W * CELL_H - 9) as usize];

        let mut system = shell();
        assert_eq!(cell(&render(&system)), all(TEXT));
        for _ in 0..shell::BLINK_TICKS {
            system = system.update(&[]);
        }
        assert_eq!(cell(&render(&system)), all(BACKGROUND));

        system = system.update(&[InputEvent::Text('A'), InputEvent::Backspace]);
        assert_eq!(cell(&render(&system)), all(TEXT));
    }

    #[test]
    fn submitted_line_stays_on_screen_above_a_new_prompt() {
        let mut system = shell();
        system = system.update(&[InputEvent::Text('A'), InputEvent::Enter]);

        let fb = render(&system);
        // Row 1 keeps "> A"; the error follows on row 2 and the new, empty
        // prompt on row 3 with its cursor block in column 2.
        let glyph = |col: i32, row: i32| fb.pixel((col + 1) * CELL_W, (row + 1) * CELL_H);
        assert_eq!(glyph(2, 1), Some(TEXT));
        assert_eq!(glyph(0, 2), Some(ERROR));
        assert_eq!(glyph(0, 3), Some(TEXT));
        assert_eq!(fb.pixel(3 * CELL_W + 3, 4 * CELL_H), Some(TEXT));
        assert!(!row_has_ink(&fb, 4));
    }

    #[test]
    fn selected_cells_render_inverse() {
        let mut system = shell();
        system = system.update(&[
            InputEvent::Text('A'),
            InputEvent::Text('B'),
            InputEvent::Left { select: false },
            InputEvent::Right { select: true },
        ]);
        let fb = render(&system);

        // Fake glyphs leave column 3 of each 4-pixel cell empty.
        let y = 2 * CELL_H;
        let gap = |cell: i32| fb.pixel((cell + 1) * CELL_W + 3, y);
        assert_eq!(gap(2), Some(BACKGROUND), "unselected 'A'");
        assert_eq!(gap(3), Some(TEXT), "selected 'B'");
        assert_eq!(fb.pixel(4 * CELL_W, y), Some(BACKGROUND), "glyph of 'B'");
    }

    #[test]
    fn text_rows_leave_a_cell_of_margin_top_and_bottom() {
        assert_eq!(text_rows(CELL_H), 240 / 5 - 2);
        assert_eq!(text_rows(10), 22);
        assert_eq!(text_rows(400), 0);
    }

    #[test]
    fn text_columns_leave_a_cell_of_margin_on_each_side() {
        assert_eq!(text_columns(CELL_W), 320 / 4 - 2);
        assert_eq!(text_columns(5), 62);
        assert_eq!(text_columns(400), 0);
    }

    #[test]
    fn long_command_line_wraps_onto_next_row() {
        let mut system = shell();
        let fits = text_columns(CELL_W) - 2;
        let mut input = vec![InputEvent::Text('o'); fits];
        system = system.update(&input);
        // The cursor sits in the first cell of row 2 once row 1 is full.
        assert!(row_has_ink(&render(&system), 2));

        // Glyph pixels of the first cell on row 2 (margin + row offset).
        let first_cell_of_row_2 = |fb: &Framebuffer| fb.pixel(CELL_W, 3 * CELL_H);
        input.truncate(1);
        system = system.update(&input);
        let fb = render(&system);
        assert_eq!(first_cell_of_row_2(&fb), Some(TEXT));
        assert!(row_has_ink(&fb, 2));
        // Nothing is drawn past the right margin.
        let right_margin = 320 - CELL_W;
        assert!((0..240).all(|y| (right_margin..320).all(|x| fb.pixel(x, y) == Some(BACKGROUND))));
    }

    #[test]
    fn render_redraws_from_scratch() {
        let rom = fake_rom();
        for system in [boot_at(BootStage::Audio), shell()] {
            let mut first = Framebuffer::new();
            let mut second = Framebuffer::new();
            second.clear(200);
            second.set_pixel(300, 230, 3);

            system.render(&mut first, &rom);
            system.render(&mut second, &rom);
            assert_eq!(first, second);
        }
    }

    // Cartridge lifecycle.

    use crate::cartridge::fixture::write_cartridge;
    use crate::cartridge_fs::fixture::Fixture;

    /// A system at the shell, navigating a cartridge root that holds a
    /// `Pong` cartridge at `/pong` whose entry script is `script`.
    fn shell_with_cartridge(fixture: &Fixture, script: &str) -> System {
        let dir = fixture.root().join("pong");
        write_cartridge(&dir, "Pong");
        std::fs::write(dir.join("code/main.luau"), script).unwrap();
        let mut system = System::new(info(), sounds(), video_defaults(), fixture.storage());
        while matches!(system.state(), SystemState::Boot { .. }) {
            system = system.update(&[]);
        }
        system
    }

    /// Types `line` and presses Enter within one tick.
    fn enter(system: System, line: &str) -> System {
        let mut events: Vec<InputEvent> = line.chars().map(InputEvent::Text).collect();
        events.push(InputEvent::Enter);
        system.update(&events)
    }

    /// Loads and runs the `Pong` cartridge with `script` as its entry.
    fn started(fixture: &Fixture, script: &str) -> System {
        let system = shell_with_cartridge(fixture, script);
        enter(enter(system, "load pong"), "run")
    }

    fn ticks(mut system: System, n: usize) -> System {
        for _ in 0..n {
            system = system.update(&[]);
        }
        system
    }

    fn press_break(system: System) -> System {
        system.update(&[InputEvent::Break])
    }

    /// Opens the shell from the pause screen.
    fn open_shell(system: System) -> System {
        system.update(&[InputEvent::Enter])
    }

    /// Pauses the running cartridge and stops it from the shell.
    fn stop_cartridge(system: System) -> System {
        enter(open_shell(press_break(system)), "stop")
    }

    fn is_running(system: &System) -> bool {
        matches!(system.state(), SystemState::Running { .. })
    }

    fn is_paused_game(system: &System) -> bool {
        matches!(
            system.state(),
            SystemState::Paused {
                view: PausedView::Game(_),
                ..
            }
        )
    }

    fn is_paused_shell(system: &System) -> bool {
        matches!(
            system.state(),
            SystemState::Paused {
                view: PausedView::Shell,
                ..
            }
        )
    }

    fn loaded_name(system: &System) -> Option<&str> {
        match system.state() {
            SystemState::Shell { loaded, .. } => loaded
                .as_ref()
                .map(|c| c.manifest().cartridge.name.as_str()),
            other => panic!("expected the shell, got {other:?}"),
        }
    }

    fn transcript(system: &System) -> Vec<String> {
        match system.state() {
            SystemState::Shell { shell, .. }
            | SystemState::Running { shell, .. }
            | SystemState::Paused { shell, .. } => shell.transcript(),
            other => panic!("expected a shell, got {other:?}"),
        }
    }

    /// What was added to the transcript after the line `from`.
    fn after(system: &System, from: &str) -> Vec<String> {
        let lines = transcript(system);
        let start = lines.iter().rposition(|l| l == from).unwrap() + 1;
        lines[start..].to_vec()
    }

    const LIFECYCLE: &str = r#"
        log.info("entry")
        function boot() log.info("boot") end
        local n = 0
        function tick() n += 1 log.info("tick", n) end
        function draw() log.info("draw", n) end
    "#;

    #[test]
    fn run_starts_the_cartridge_and_ticks_drive_it() {
        let fixture = Fixture::new("sys-run");
        let system = started(&fixture, LIFECYCLE);
        assert!(is_running(&system));
        assert_eq!(
            after(&system, "> run"),
            ["Running Pong 0.1.0", "entry", "boot"]
        );

        let system = stop_cartridge(ticks(system, 3));
        assert_eq!(
            after(&system, "> run"),
            [
                "Running Pong 0.1.0",
                "entry",
                "boot",
                "tick    1",
                "draw    1",
                "tick    2",
                "draw    2",
                "tick    3",
                "draw    3",
                "Paused Pong 0.1.0",
                "Type 'resume' to continue or 'stop' to end.",
                "> stop",
                "Stopped Pong 0.1.0",
            ]
        );
        assert_eq!(loaded_name(&system), Some("Pong"));
    }

    #[test]
    fn the_shell_takes_no_input_while_running() {
        let fixture = Fixture::new("sys-run-input");
        let system = enter(started(&fixture, ""), "help");
        assert!(is_running(&system));
        let system = open_shell(press_break(system));
        assert_eq!(
            after(&system, "> run"),
            [
                "Running Pong 0.1.0",
                "Paused Pong 0.1.0",
                "Type 'resume' to continue or 'stop' to end."
            ]
        );
        // Nothing typed while running reached the command line.
        let system = system.update(&[InputEvent::Enter]);
        assert_eq!(
            transcript(&system).last().unwrap(),
            "Type 'resume' to continue or 'stop' to end."
        );
    }

    #[test]
    fn break_in_the_shell_is_ignored() {
        let fixture = Fixture::new("sys-shell-break");
        let system = enter(shell_with_cartridge(&fixture, ""), "load pong");
        let before = transcript(&system);
        let system = press_break(system);
        assert_eq!(transcript(&system), before);
        assert_eq!(loaded_name(&system), Some("Pong"));
    }

    #[test]
    fn running_again_starts_a_fresh_vm() {
        let fixture = Fixture::new("sys-rerun");
        let script = "function tick() count = (count or 0) + 1 log.info(count) end";
        let system = stop_cartridge(ticks(started(&fixture, script), 2));
        let system = stop_cartridge(ticks(enter(system, "run"), 1));
        assert_eq!(
            after(&system, "> run"),
            [
                "Running Pong 0.1.0",
                "1",
                "Paused Pong 0.1.0",
                "Type 'resume' to continue or 'stop' to end.",
                "> stop",
                "Stopped Pong 0.1.0"
            ]
        );
        assert!(transcript(&system).contains(&"2".to_owned()));
    }

    #[test]
    fn start_failures_keep_the_cartridge_loaded() {
        let fixture = Fixture::new("sys-start-fail");
        for (script, expected) in [
            (
                r#"log.info("x") error("boom")"#,
                vec!["x", "! Runtime error: code/main.luau:1: boom"],
            ),
            (
                r#"log.info("e") function boot() log.info("b") error("no") end"#,
                vec!["e", "b", "! Runtime error in boot: code/main.luau:1: no"],
            ),
            (
                "function boot() while true do end end",
                vec!["! Execution limit exceeded in boot"],
            ),
            ("while true do end", vec!["! Execution limit exceeded"]),
        ] {
            let system = started(&fixture, script);
            assert_eq!(after(&system, "> run"), expected, "{script}");
            assert_eq!(loaded_name(&system), Some("Pong"), "{script}");
        }

        let system = started(&fixture, "local = 1");
        let lines = after(&system, "> run");
        assert!(
            matches!(lines.as_slice(), [only] if only.starts_with("! Syntax error: code/main.luau:1:")),
            "{lines:?}"
        );
        assert_eq!(loaded_name(&system), Some("Pong"));
    }

    #[test]
    fn callback_failures_stop_the_cartridge() {
        let fixture = Fixture::new("sys-tick-fail");
        for (script, error) in [
            (
                r#"function tick() log.info("t") error("boom") end"#,
                "! Runtime error in tick: code/main.luau:1: boom",
            ),
            (
                r#"function tick() log.info("t") end function draw() error("bad") end"#,
                "! Runtime error in draw: code/main.luau:1: bad",
            ),
            (
                r#"function tick() log.info("t") while true do end end"#,
                "! Execution limit exceeded in tick",
            ),
            (
                r#"function tick() log.info("t") end
                   function draw() pcall(function() while true do end end) end"#,
                "! Execution limit exceeded in draw",
            ),
        ] {
            let system = ticks(started(&fixture, script), 1);
            assert_eq!(
                after(&system, "> run"),
                ["Running Pong 0.1.0", "t", error],
                "{script}"
            );
            assert_eq!(loaded_name(&system), Some("Pong"), "{script}");
        }
    }

    #[test]
    fn info_reflects_the_cartridge_state() {
        let fixture = Fixture::new("sys-info");
        let state = |system: System| {
            let system = enter(system, "info");
            let line = transcript(&system).last().unwrap().clone();
            (system, line)
        };
        let (system, line) = state(shell_with_cartridge(&fixture, ""));
        assert_eq!(line, "STATE       IDLE");
        let (system, line) = state(enter(system, "load pong"));
        assert_eq!(line, "STATE       LOADED");
        let (system, line) = state(open_shell(press_break(enter(system, "run"))));
        assert_eq!(line, "STATE       PAUSED");
        assert!(is_paused_shell(&system));
        let (system, line) = state(enter(system, "stop"));
        assert_eq!(line, "STATE       LOADED");
        assert!(transcript(&system).contains(&"CARTRIDGE   Pong 0.1.0".to_owned()));
    }

    #[test]
    fn unload_returns_a_loaded_cartridge_to_idle() {
        let fixture = Fixture::new("sys-unload");
        let system = enter(shell_with_cartridge(&fixture, ""), "load pong");
        let system = enter(system, "unload");
        assert_eq!(loaded_name(&system), None);
        assert_eq!(after(&system, "> unload"), ["Unloaded Pong 0.1.0"]);

        let system = enter(system, "info");
        let lines = after(&system, "> info");
        assert_eq!(
            &lines[4..],
            ["RUNTIME     NONE", "CARTRIDGE   NONE", "STATE       IDLE"]
        );
    }

    #[test]
    fn unload_preserves_cartridge_files_and_the_shell_directory() {
        let fixture = Fixture::new("sys-unload-storage");
        let dir = fixture.root().join("games/pong");
        write_cartridge(&dir, "Pong");
        std::fs::write(dir.join("save.dat"), b"kept").unwrap();
        let manifest = std::fs::read(dir.join("cartridge.ron")).unwrap();

        let mut system = System::new(info(), sounds(), video_defaults(), fixture.storage());
        while matches!(system.state(), SystemState::Boot { .. }) {
            system = system.update(&[]);
        }
        let system = enter(enter(enter(system, "cd games"), "load pong"), "unload");
        let system = enter(system, "pwd");

        assert_eq!(transcript(&system).last().unwrap(), "/games");
        assert_eq!(std::fs::read(dir.join("cartridge.ron")).unwrap(), manifest);
        assert_eq!(std::fs::read(dir.join("save.dat")).unwrap(), b"kept");
    }

    #[test]
    fn unload_without_a_cartridge_reports_an_error() {
        let fixture = Fixture::new("sys-unload-empty");
        let system = enter(shell_with_cartridge(&fixture, ""), "unload");
        assert_eq!(after(&system, "> unload"), ["! No cartridge loaded"]);
        assert_eq!(loaded_name(&system), None);
    }

    #[test]
    fn missing_entry_is_reported_at_run() {
        let fixture = Fixture::new("sys-run-missing");
        let system = enter(shell_with_cartridge(&fixture, ""), "load pong");
        std::fs::remove_file(fixture.root().join("pong/code/main.luau")).unwrap();
        let system = enter(system, "run");
        assert_eq!(
            after(&system, "> run"),
            ["! Entry not found: code/main.luau"]
        );
        assert_eq!(loaded_name(&system), Some("Pong"));
    }

    #[test]
    fn malformed_object_tiles_prevent_startup() {
        let cases = [
            (vec![0; 31], "length must be a multiple of 32 bytes, got 31"),
            (
                vec![0; glaux_machine::sprite::OBJ_TILE_BANK_BYTES + 32],
                "got 32800 bytes, maximum is 32768",
            ),
        ];
        for (n, (bytes, expected)) in cases.into_iter().enumerate() {
            let fixture = Fixture::new(&format!("sys-run-bad-sprites-{n}"));
            let system = shell_with_cartridge(&fixture, "");
            std::fs::write(fixture.root().join("pong/sprites.bin"), bytes).unwrap();
            let system = enter(enter(system, "load pong"), "run");
            let lines = after(&system, "> run");
            assert!(
                matches!(lines.as_slice(), [only] if only.starts_with("! Invalid sprites.bin:") && only.contains(expected)),
                "{lines:?}"
            );
            assert_eq!(loaded_name(&system), Some("Pong"));
        }
    }

    #[test]
    fn malformed_palette_assets_prevent_startup() {
        let cases = [
            (
                "obp.bin",
                vec![0; 5],
                "! Invalid obp.bin: object palette data must be exactly 128 bytes, got 5",
            ),
            (
                "palette.bin",
                vec![0; 3],
                "! Invalid palette.bin: palette data must be exactly 768 bytes, got 3",
            ),
            (
                "objattr.bin",
                vec![0, 0x80],
                "! Invalid objattr.bin: object tile 1 attribute 0x80 sets reserved bits; only bits 0-2 are defined",
            ),
        ];
        for (name, bytes, expected) in cases {
            let fixture = Fixture::new(&format!("sys-run-bad-{name}"));
            let system = shell_with_cartridge(&fixture, "");
            std::fs::write(fixture.root().join("pong").join(name), bytes).unwrap();
            let system = enter(enter(system, "load pong"), "run");
            assert_eq!(after(&system, "> run"), [expected], "{name}");
            assert_eq!(loaded_name(&system), Some("Pong"), "{name}");
        }
    }

    /// The fake ROM's palette in reverse order, so every system color sits
    /// at a different index of the cartridge palette.
    fn reversed_rom_palette() -> Palette {
        let mut entries = *fake_rom().palette().entries();
        entries.reverse();
        Palette::new(entries)
    }

    fn started_with_palette(fixture: &Fixture, script: &str, palette: &Palette) -> System {
        let system = shell_with_cartridge(fixture, script);
        std::fs::write(fixture.root().join("pong/palette.bin"), palette.to_rgb888()).unwrap();
        enter(enter(system, "load pong"), "run")
    }

    #[test]
    fn the_cartridge_palette_is_presented_only_with_its_frame() {
        let fixture = Fixture::new("sys-frame-palette");
        let rom = fake_rom();
        let cartridge = reversed_rom_palette();
        assert_eq!(new_system().frame_palette(&rom), rom.palette());

        let system = ticks(started_with_palette(&fixture, COUNTER, &cartridge), 1);
        assert_eq!(system.frame_palette(&rom), &cartridge);
        let system = press_break(system);
        assert_eq!(system.frame_palette(&rom), &cartridge);
        let system = open_shell(system);
        assert_eq!(system.frame_palette(&rom), rom.palette());
    }

    #[test]
    fn the_pause_indicator_keeps_its_colors_under_a_cartridge_palette() {
        let fixture = Fixture::new("sys-pause-palette");
        let script = "function draw() cls(1) end";
        let system = ticks(
            started_with_palette(&fixture, script, &reversed_rom_palette()),
            1,
        );
        let fb = render(&press_break(system));
        let (x, y) = pause::PLATE_ORIGIN;
        assert_eq!(fb.pixel(x + 1, y + 1), Some(255 - BACKGROUND));
        assert_eq!(fb.pixel(x + 5, y + 5), Some(255 - TEXT));
    }

    #[test]
    fn script_errors_do_not_reveal_host_paths() {
        let fixture = Fixture::new("sys-run-paths");
        let host_paths = [
            fixture.base().display().to_string(),
            fixture.root().display().to_string(),
            fixture.outside().display().to_string(),
        ];
        for script in [
            "local = 1",
            r#"error("boom")"#,
            "local t = nil\nlog.info(t.x)",
            r#"require("../../outside/secret")"#,
            r#"log.info(debug.info(1, "s"))"#,
            r#"function tick() log.info(debug.info(1, "s")) error("x") end"#,
        ] {
            let system = ticks(started(&fixture, script), 1);
            for line in after(&system, "> run") {
                for path in &host_paths {
                    assert!(!line.contains(path.as_str()), "{script}: {line}");
                }
            }
        }
    }

    // Cartridge drawing.

    /// A framebuffer filled with `color`.
    fn filled(color: u8) -> Framebuffer {
        let mut fb = Framebuffer::new();
        clear(&mut fb, color);
        fb
    }

    #[test]
    fn a_cartridge_that_has_not_drawn_shows_index_zero() {
        let fixture = Fixture::new("sys-draw-blank");
        let system = started(&fixture, r#"function tick() log.info("x") end"#);
        assert_eq!(render(&system), filled(0));
        let system = ticks(system, 2);
        assert!(is_running(&system));
        assert_eq!(render(&system), filled(0));
    }

    #[test]
    fn cls_fills_the_screen() {
        let fixture = Fixture::new("sys-draw-cls");
        let system = ticks(started(&fixture, "function draw() cls(1) end"), 1);
        assert_eq!(render(&system), filled(1));
    }

    #[test]
    fn a_missing_sprite_asset_starts_with_an_empty_bank() {
        let fixture = Fixture::new("sys-draw-no-sprites");
        let system = ticks(
            started(&fixture, "function draw() cls(6) spr(0, 10, 10) end"),
            1,
        );
        assert!(is_running(&system));
        assert_eq!(render(&system), filled(6));
    }

    #[test]
    fn later_commands_draw_over_earlier_ones() {
        let fixture = Fixture::new("sys-draw-order");
        let system = ticks(
            started(&fixture, "function draw() cls(1) pset(5, 5, 2) end"),
            1,
        );
        let mut expected = filled(1);
        expected.set_pixel(5, 5, 2);
        assert_eq!(render(&system), expected);
    }

    #[test]
    fn sprites_render_in_draw_command_order() {
        let fixture = Fixture::new("sys-draw-sprite-order");
        let dir = fixture.root().join("pong");
        let system = shell_with_cartridge(
            &fixture,
            r#"function draw() cls(9) spr(0, 0, 0) pset(0, 0, 7) print("A", 4, 0, 8) end"#,
        );
        std::fs::write(dir.join("sprites.bin"), [0x22; 32]).unwrap();
        let system = ticks(enter(enter(system, "load pong"), "run"), 1);
        let fb = render(&system);

        assert_eq!(fb.pixel(0, 0), Some(7), "pset follows sprite");
        assert_eq!(fb.pixel(4, 0), Some(8), "print follows sprite");
        assert_eq!(fb.pixel(2, 2), Some(2));
        assert_eq!(fb.pixel(3, 0), Some(2), "sprite follows cls");
        assert_eq!(fb.pixel(7, 7), Some(2));
        assert_eq!(fb.pixel(8, 0), Some(9));
    }

    #[test]
    fn sprite_blocks_compose_grid_tiles_in_draw_command_order() {
        let fixture = Fixture::new("sys-draw-sprite-block");
        let dir = fixture.root().join("pong");
        let system = shell_with_cartridge(
            &fixture,
            "function draw() cls(9) spr(1, 0, 0, 2, 2) pset(0, 0, 7) end",
        );
        let mut bytes = vec![0; 18 * 32];
        for (tile, fill) in [(1, 0x11), (2, 0x22), (17, 0x33), (16, 0x44)] {
            bytes[tile * 32..(tile + 1) * 32].fill(fill);
        }
        std::fs::write(dir.join("sprites.bin"), bytes).unwrap();
        let system = ticks(enter(enter(system, "load pong"), "run"), 1);
        let fb = render(&system);

        assert_eq!(fb.pixel(0, 0), Some(7), "pset follows sprite");
        assert_eq!(fb.pixel(1, 0), Some(1));
        assert_eq!(fb.pixel(8, 7), Some(2));
        assert_eq!(
            fb.pixel(0, 8),
            Some(3),
            "second row starts one grid row down"
        );
        assert_eq!(fb.pixel(15, 15), Some(9), "tile 18 is transparent");
        assert_eq!(fb.pixel(16, 0), Some(9));
    }

    #[test]
    fn a_fresh_run_reloads_object_tiles() {
        let fixture = Fixture::new("sys-draw-sprite-reload");
        let dir = fixture.root().join("pong");
        let system = shell_with_cartridge(&fixture, "function draw() cls(0) spr(0, 10, 10) end");
        std::fs::write(dir.join("sprites.bin"), [0x11; 32]).unwrap();
        let system = ticks(enter(enter(system, "load pong"), "run"), 1);
        assert_eq!(render(&system).pixel(10, 10), Some(1));

        let system = stop_cartridge(system);
        std::fs::write(dir.join("sprites.bin"), [0x22; 32]).unwrap();
        let system = ticks(enter(system, "run"), 1);
        assert_eq!(render(&system).pixel(10, 10), Some(2));
    }

    #[test]
    fn off_screen_pixels_are_ignored() {
        let fixture = Fixture::new("sys-draw-clip");
        let script = "function draw() cls(1) pset(-1, 0, 2) pset(320, 10, 2) pset(0, 240, 2) pset(1e300, 0, 2) end";
        let system = ticks(started(&fixture, script), 1);
        assert!(is_running(&system));
        assert_eq!(render(&system), filled(1));
    }

    #[test]
    fn screen_text_uses_the_rom_font() {
        let fixture = Fixture::new("sys-draw-text");
        let system = ticks(
            started(
                &fixture,
                r#"function draw() cls(1) print("AB", 10, 20, 7) end"#,
            ),
            1,
        );
        let mut expected = filled(1);
        draw_text(&mut expected, fake_rom().font(), "AB", 10, 20, 7);
        assert_eq!(render(&system), expected);
        // The fake font draws 3×3 blocks in 4-pixel cells.
        assert_eq!(expected.pixel(12, 22), Some(7));
        assert_eq!(expected.pixel(13, 20), Some(1));
        assert_eq!(expected.pixel(14, 20), Some(7));
    }

    #[test]
    fn the_frame_persists_until_the_next_draw_replaces_it() {
        let fixture = Fixture::new("sys-draw-persist");
        let script = "local n = 0 function tick() n += 1 end function draw() cls(n) end";
        let system = ticks(started(&fixture, script), 1);
        let rom = fake_rom();
        for _ in 0..3 {
            let mut fb = filled(200);
            system.render(&mut fb, &rom);
            assert_eq!(fb, filled(1));
        }
        let system = ticks(system, 1);
        assert_eq!(render(&system), filled(2));
    }

    #[test]
    fn stopping_discards_the_frame_and_shows_the_shell() {
        let fixture = Fixture::new("sys-draw-stop");
        let draws = "function draw() cls(1) end";
        let stopped = [
            stop_cartridge(ticks(started(&fixture, draws), 1)),
            ticks(
                started(
                    &fixture,
                    "local n = 0 function draw() n += 1 cls(1) if n == 2 then error(\"x\") end end",
                ),
                2,
            ),
        ];
        for system in stopped {
            assert_eq!(loaded_name(&system), Some("Pong"));
            let fb = render(&system);
            assert_eq!(fb.pixel(0, 0), Some(BACKGROUND));
            assert!(fb.as_bytes().iter().all(|&b| b != 1));
        }
    }

    #[test]
    fn a_draw_error_stops_the_cartridge() {
        let fixture = Fixture::new("sys-draw-error");
        let system = ticks(
            started(&fixture, "function draw() cls(1) pset(0, 0, 999) end"),
            1,
        );
        assert_eq!(
            after(&system, "> run"),
            [
                "Running Pong 0.1.0",
                "! Runtime error in draw: code/main.luau:1: bad argument #3 to 'pset' \
                 (palette index 0-255 expected, got 999)",
            ]
        );
    }

    #[test]
    fn drawing_outside_draw_stops_the_cartridge() {
        let fixture = Fixture::new("sys-draw-phase");
        let system = ticks(started(&fixture, "function tick() cls(1) end"), 1);
        assert_eq!(
            after(&system, "> run"),
            [
                "Running Pong 0.1.0",
                "! Runtime error in tick: code/main.luau:1: 'cls' can only be called during draw()",
            ]
        );
    }

    #[test]
    fn logged_levels_reach_the_shell() {
        let fixture = Fixture::new("sys-log");
        let script = r#"function tick() log.trace("t") log.warn("w") end"#;
        let system = press_break(ticks(started(&fixture, script), 1));
        assert_eq!(
            after(&system, "> run"),
            ["Running Pong 0.1.0", "t", "w", "Paused Pong 0.1.0"]
        );
    }

    // Game input.

    use glaux_input::{Button, Key};

    const UP_STATE: &str = r#"function tick() log.info(btn(Button.Up), btnp(Button.Up)) end"#;
    const SPACE_STATE: &str = r#"function tick() log.info(key(Key.Space), keyp(Key.Space)) end"#;

    fn space_down() -> InputEvent {
        InputEvent::KeyDown(Key::Space)
    }

    fn down() -> InputEvent {
        InputEvent::ButtonDown(Button::Up)
    }

    fn up() -> InputEvent {
        InputEvent::ButtonUp(Button::Up)
    }

    /// Runs one tick per entry of `ticks` and returns what the cartridge
    /// logged on each.
    fn logged_per_tick(mut system: System, ticks: &[&[InputEvent]]) -> (System, Vec<String>) {
        let mut lines = Vec::new();
        for input in ticks {
            let before = transcript(&system).len();
            system = system.update(input);
            lines.extend(transcript(&system).split_off(before));
        }
        (system, lines)
    }

    #[test]
    fn buttons_reach_the_cartridge_per_tick() {
        let fixture = Fixture::new("sys-btn");
        let (_, lines) = logged_per_tick(
            started(&fixture, UP_STATE),
            &[
                &[down()],
                &[],
                &[up()],
                &[down()],
                &[up(), InputEvent::ButtonDown(Button::A)],
                &[down(), up()],
                &[],
            ],
        );
        assert_eq!(
            lines,
            [
                "true    true",
                "true    false",
                "false   false",
                "true    true",
                "false   false",
                "false   true",
                "false   false",
            ]
        );
    }

    #[test]
    fn keys_reach_the_cartridge_per_tick() {
        let fixture = Fixture::new("sys-key");
        let (_, lines) = logged_per_tick(
            started(&fixture, SPACE_STATE),
            &[
                &[space_down()],
                // A platform auto-repeat that reaches the system anyway.
                &[space_down()],
                &[InputEvent::KeyUp(Key::Space)],
                &[space_down(), InputEvent::KeyUp(Key::Space)],
            ],
        );
        assert_eq!(
            lines,
            [
                "true    true",
                "true    false",
                "false   false",
                "false   true",
            ]
        );
    }

    #[test]
    fn catch_up_ticks_press_only_once() {
        // As the main loop does: the first due tick takes the pending input
        // and the catch-up ticks after it take none.
        let fixture = Fixture::new("sys-btn-catch-up");
        let (_, lines) = logged_per_tick(
            started(&fixture, UP_STATE),
            &[&[down(), down()], &[], &[], &[]],
        );
        assert_eq!(
            lines,
            [
                "true    true",
                "true    false",
                "true    false",
                "true    false"
            ]
        );
    }

    #[test]
    fn editing_events_do_not_press_buttons() {
        let fixture = Fixture::new("sys-btn-edit");
        let (_, lines) = logged_per_tick(
            started(&fixture, UP_STATE),
            &[&[InputEvent::Up, InputEvent::Text('z')]],
        );
        assert_eq!(lines, ["false   false"]);
    }

    #[test]
    fn break_pauses_the_cartridge_despite_buttons() {
        let fixture = Fixture::new("sys-btn-break");
        let system = started(&fixture, UP_STATE).update(&[down(), InputEvent::Break]);
        assert!(is_paused_game(&system));
        assert_eq!(
            after(&system, "> run"),
            ["Running Pong 0.1.0", "Paused Pong 0.1.0"]
        );
    }

    #[test]
    fn a_new_run_starts_with_buttons_released() {
        let fixture = Fixture::new("sys-btn-rerun");
        let system = started(&fixture, UP_STATE).update(&[down()]);
        let system = enter(stop_cartridge(system), "run");
        // Up stays physically held, so the host sends nothing new.
        let (system, lines) = logged_per_tick(system, &[&[]]);
        assert_eq!(lines, ["false   false"]);
        assert!(is_running(&system));
    }

    #[test]
    fn input_mode_follows_the_active_context() {
        let fixture = Fixture::new("sys-input-mode");
        assert_eq!(new_system().input_mode(), InputMode::Text);
        let system = shell_with_cartridge(&fixture, "");
        assert_eq!(system.input_mode(), InputMode::Text);
        let system = enter(enter(system, "load pong"), "run");
        assert_eq!(system.input_mode(), InputMode::Game);
        let system = press_break(system);
        assert_eq!(system.input_mode(), InputMode::Text);
        let system = open_shell(system);
        assert_eq!(system.input_mode(), InputMode::Text);
        let system = enter(system, "resume");
        assert_eq!(system.input_mode(), InputMode::Game);
        let system = stop_cartridge(system);
        assert_eq!(system.input_mode(), InputMode::Text);
    }

    // Pause and resume.

    /// Logs a tick counter kept in the VM, and its entry and `boot()` runs.
    const COUNTER: &str = r#"
        log.info("entry")
        function boot() log.info("boot") end
        local n = 0
        function tick() n += 1 log.info("tick", n) end
    "#;

    /// What the cartridge logged during `n` more ticks of `system`.
    fn logged_ticks(system: System, n: usize) -> (System, Vec<String>) {
        let empty: Vec<&[InputEvent]> = vec![&[]; n];
        logged_per_tick(system, &empty)
    }

    #[test]
    fn break_pauses_without_disposing_of_the_vm() {
        let fixture = Fixture::new("sys-pause");
        let system = press_break(ticks(started(&fixture, COUNTER), 3));
        assert!(is_paused_game(&system));
        assert_eq!(transcript(&system).last().unwrap(), "Paused Pong 0.1.0");
        let (system, lines) = logged_ticks(system, 5);
        assert_eq!(
            lines,
            Vec::<String>::new(),
            "a paused cartridge is not ticked"
        );
        assert!(is_paused_game(&system));
    }

    #[test]
    fn break_on_the_pause_screen_resumes_the_same_vm() {
        let fixture = Fixture::new("sys-pause-resume");
        let system = press_break(press_break(ticks(started(&fixture, COUNTER), 3)));
        assert!(is_running(&system));
        assert_eq!(transcript(&system).last().unwrap(), "Resumed Pong 0.1.0");
        let (system, lines) = logged_ticks(system, 1);
        assert_eq!(lines, ["tick    4"]);
        assert_eq!(
            transcript(&system)
                .iter()
                .filter(|line| *line == "entry" || *line == "boot")
                .count(),
            2,
            "entry and boot ran only once"
        );
    }

    #[test]
    fn paused_cartridges_keep_their_logical_time() {
        let fixture = Fixture::new("sys-pause-time");
        let script = "function tick() log.info(frame(), t() * 60) end";
        let system = press_break(ticks(started(&fixture, script), 2));
        let system = press_break(ticks(system, 10));
        let (_, lines) = logged_ticks(system, 1);
        assert_eq!(lines, ["2   2"]);
    }

    #[test]
    fn the_pause_screen_shows_the_frozen_frame_under_the_overlay() {
        let fixture = Fixture::new("sys-pause-render");
        let script = "local n = 0 function tick() n += 1 end function draw() cls(n) end";
        let system = press_break(ticks(started(&fixture, script), 1));
        let rom = fake_rom();
        let SystemState::Paused { running, .. } = system.state() else {
            panic!("expected a paused cartridge");
        };
        let frozen = |running: &RunningCartridge| {
            let mut fb = Framebuffer::new();
            running.render(&mut fb, rom.font());
            fb
        };
        assert_eq!(frozen(running), filled(1));

        let fb = render(&system);
        assert_ne!(fb, filled(1));
        let shaded = glaux_video::shade::shade_table(rom.palette(), 65);
        assert_eq!(fb.pixel(0, 0), Some(shaded[1]));
        let (x, y) = pause::PLATE_ORIGIN;
        assert_eq!(fb.pixel(x + 1, y + 1), Some(BACKGROUND));
        assert_eq!(fb.pixel(x + 5, y + 5), Some(TEXT));

        // Neither rendering nor time on the pause screen touches the frame
        // the cartridge drew.
        let system = ticks(system, pause::PULSE_TICKS as usize);
        let SystemState::Paused { running, .. } = system.state() else {
            panic!("expected a paused cartridge");
        };
        assert_eq!(frozen(running), filled(1));
        let pulsed = render(&system);
        assert_ne!(pulsed, fb, "the indicator animates");
        assert_eq!(pulsed.pixel(0, 0), Some(shaded[1]));
    }

    #[test]
    fn enter_opens_the_shell_with_the_cartridge_paused() {
        let fixture = Fixture::new("sys-pause-shell");
        let system = open_shell(press_break(ticks(started(&fixture, COUNTER), 2)));
        assert!(is_paused_shell(&system));
        assert_eq!(system.input_mode(), InputMode::Text);
        assert_eq!(
            transcript(&system).last().unwrap(),
            "Type 'resume' to continue or 'stop' to end."
        );
        assert!(row_has_ink(&render(&system), 0), "the shell is drawn");

        // Break in the shell leaves the cartridge paused.
        let system = press_break(enter(system, "info"));
        assert!(is_paused_shell(&system));
        assert_eq!(transcript(&system).last().unwrap(), "STATE       PAUSED");
    }

    #[test]
    fn events_after_enter_reach_the_shell() {
        let fixture = Fixture::new("sys-pause-shell-events");
        let mut events = vec![InputEvent::Enter];
        events.extend("info".chars().map(InputEvent::Text));
        events.push(InputEvent::Enter);
        let system = press_break(started(&fixture, "")).update(&events);
        assert_eq!(transcript(&system).last().unwrap(), "STATE       PAUSED");
    }

    #[test]
    fn resume_from_the_shell_continues_the_same_vm() {
        let fixture = Fixture::new("sys-pause-shell-resume");
        let system = open_shell(press_break(ticks(started(&fixture, COUNTER), 2)));
        let system = enter(system, "resume");
        assert!(is_running(&system));
        assert_eq!(after(&system, "> resume"), ["Resumed Pong 0.1.0"]);
        let (_, lines) = logged_ticks(system, 1);
        assert_eq!(lines, ["tick    3"]);
    }

    #[test]
    fn stop_disposes_of_the_paused_vm() {
        let fixture = Fixture::new("sys-pause-stop");
        let system = stop_cartridge(ticks(started(&fixture, COUNTER), 2));
        assert_eq!(loaded_name(&system), Some("Pong"));
        assert_eq!(after(&system, "> stop"), ["Stopped Pong 0.1.0"]);

        let system = enter(system, "run");
        assert_eq!(
            after(&system, "> run"),
            ["Running Pong 0.1.0", "entry", "boot"]
        );
        let (_, lines) = logged_ticks(system, 1);
        assert_eq!(lines, ["tick    1"]);
    }

    #[test]
    fn load_and_run_cannot_replace_a_paused_cartridge() {
        let fixture = Fixture::new("sys-pause-refuse");
        let system = open_shell(press_break(ticks(started(&fixture, COUNTER), 2)));
        let system = enter(enter(system, "load pong"), "run");
        assert!(is_paused_shell(&system));
        assert_eq!(
            after(&system, "> load pong"),
            [
                "! Cartridge paused: resume or stop it first",
                "> run",
                "! Cartridge paused: resume or stop it first",
            ]
        );
        let (_, lines) = logged_ticks(enter(system, "resume"), 1);
        assert_eq!(lines, ["tick    3"]);
    }

    #[test]
    fn unload_cannot_dispose_of_a_paused_cartridge() {
        let fixture = Fixture::new("sys-pause-unload");
        let system = open_shell(press_break(ticks(started(&fixture, COUNTER), 2)));
        let system = enter(system, "unload");
        assert!(is_paused_shell(&system));
        assert_eq!(
            after(&system, "> unload"),
            ["! Cartridge paused: resume or stop it first"]
        );

        let (_, lines) = logged_ticks(enter(system, "resume"), 1);
        assert_eq!(lines, ["tick    3"]);
    }

    #[test]
    fn a_runtime_failure_after_resuming_is_not_resumable() {
        let fixture = Fixture::new("sys-pause-fail");
        let script = r#"local n = 0 function tick() n += 1 if n == 2 then error("boom") end end"#;
        let system = press_break(press_break(ticks(started(&fixture, script), 1)));
        let system = ticks(system, 1);
        assert_eq!(loaded_name(&system), Some("Pong"));
        assert_eq!(
            transcript(&system).last().unwrap(),
            "! Runtime error in tick: code/main.luau:1: boom"
        );
        let system = enter(system, "resume");
        assert_eq!(after(&system, "> resume"), ["! No paused cartridge"]);
        assert_eq!(loaded_name(&system), Some("Pong"));
    }

    #[test]
    fn scrollback_and_history_survive_a_pause() {
        let fixture = Fixture::new("sys-pause-history");
        let system = enter(shell_with_cartridge(&fixture, COUNTER), "load pong");
        let system = ticks(enter(system, "run"), 1);
        let system = open_shell(press_break(system));
        let system = system.update(&[InputEvent::Up]);
        let SystemState::Paused { shell, .. } = system.state() else {
            panic!("expected a paused cartridge");
        };
        let lines = shell.transcript();
        assert!(lines.contains(&"> load pong".to_owned()));
        assert!(lines.contains(&"tick    1".to_owned()));
        // Up recalled the last command, typed before the cartridge ran.
        let editor = shell.lines(text_columns(CELL_W), text_rows(CELL_H));
        assert!(
            editor
                .iter()
                .any(|line| line.text == "run" && line.col == 2)
        );
    }

    #[test]
    fn resumed_input_starts_released() {
        let fixture = Fixture::new("sys-pause-input");
        let resumes: [fn(System) -> System; 2] =
            [press_break, |system| enter(open_shell(system), "resume")];
        for resume in resumes {
            // Up goes down before the pause and stays physically held, so the
            // host sends nothing more for it.
            let (system, lines) = logged_per_tick(started(&fixture, UP_STATE), &[&[down()]]);
            assert_eq!(lines, ["true    true"]);
            let system = resume(press_break(system));
            let (_, lines) = logged_per_tick(system, &[&[], &[]]);
            assert_eq!(lines, ["false   false", "false   false"]);
        }
    }

    #[test]
    fn resumed_keys_start_released() {
        let fixture = Fixture::new("sys-pause-keys");
        let resumes: [fn(System) -> System; 2] =
            [press_break, |system| enter(open_shell(system), "resume")];
        for resume in resumes {
            let (system, lines) =
                logged_per_tick(started(&fixture, SPACE_STATE), &[&[space_down()]]);
            assert_eq!(lines, ["true    true"]);
            let system = resume(press_break(system));
            let (_, lines) = logged_per_tick(system, &[&[], &[]]);
            assert_eq!(lines, ["false   false", "false   false"]);
        }
    }

    #[test]
    fn keys_pressed_while_paused_do_not_reach_the_cartridge() {
        let fixture = Fixture::new("sys-pause-keys-ignored");
        let system = press_break(started(&fixture, SPACE_STATE)).update(&[space_down()]);
        let (_, lines) = logged_per_tick(press_break(system), &[&[]]);
        assert_eq!(lines, ["false   false"]);
    }

    #[test]
    fn a_new_run_starts_with_keys_released() {
        let fixture = Fixture::new("sys-key-rerun");
        let system = started(&fixture, SPACE_STATE).update(&[space_down()]);
        let system = enter(stop_cartridge(system), "run");
        let (_, lines) = logged_per_tick(system, &[&[]]);
        assert_eq!(lines, ["false   false"]);
    }

    #[test]
    fn a_press_in_the_pausing_tick_is_dropped() {
        let fixture = Fixture::new("sys-pause-input-same-tick");
        let system = started(&fixture, UP_STATE).update(&[down(), InputEvent::Break]);
        let (_, lines) = logged_per_tick(press_break(system), &[&[], &[up()], &[down()]]);
        assert_eq!(lines, ["false   false", "false   false", "true    true"]);
    }

    /// Each channel's (waveform, frequency, volume) after `system` writes
    /// its sound into `psg`.
    ///
    /// `System::audio` remembers which source last wrote the PSG, so a test
    /// must keep passing it the same PSG.
    fn voices(system: &mut System, psg: &mut Psg) -> [(Waveform, u16, u8); CHANNELS] {
        system.audio(psg);
        std::array::from_fn(|index| {
            let channel = psg.channel(index).unwrap();
            (channel.waveform(), channel.frequency(), channel.volume())
        })
    }

    fn is_silent(system: &mut System, psg: &mut Psg) -> bool {
        voices(system, psg)
            .iter()
            .all(|&(_, _, volume)| volume == 0)
    }

    /// Each channel's gate after `system` writes its sound into `psg`.
    fn gates(system: &mut System, psg: &mut Psg) -> [bool; CHANNELS] {
        system.audio(psg);
        std::array::from_fn(|index| psg.channel(index).unwrap().gate())
    }

    // System ROM audio.

    /// Boots `system` the way the main loop does, writing and rendering
    /// the PSG after every update, until the shell takes over. Returns the
    /// samples of each tick; tick `n` is the one that reaches frame `n + 1`.
    fn boot_audio(mut system: System, psg: &mut Psg) -> (System, Vec<Vec<i16>>) {
        let mut ticks = Vec::new();
        while matches!(system.state(), SystemState::Boot { .. }) {
            system = system.update(&[]);
            system.audio(psg);
            ticks.push(render_psg(psg));
        }
        (system, ticks)
    }

    fn quiet(samples: &[i16]) -> bool {
        samples.iter().all(|&sample| sample == 0)
    }

    /// Ticks of the boot chime rendered on its own: long enough for every
    /// release to finish.
    const CHIME_TICKS: usize = 60;

    fn chime_alone() -> Vec<Vec<i16>> {
        let mut sound = SystemSound::new(sounds());
        sound.play(SystemSfx::Boot);
        let mut psg = Psg::new();
        (0..CHIME_TICKS)
            .map(|_| {
                sound.tick();
                sound.apply(&mut psg, false);
                render_psg(&mut psg)
            })
            .collect()
    }

    #[test]
    fn boot_plays_its_chime_once_through_the_psg() {
        let (_, ticks) = boot_audio(new_system(), &mut Psg::new());
        let start = boot::CHIME_FRAME as usize - 1;
        let chime = chime_alone();
        assert!(chime.iter().any(|samples| !quiet(samples)));
        assert!(quiet(chime.last().unwrap()), "the chime has faded out");

        assert!(ticks[..start].iter().all(|samples| quiet(samples)));
        assert_eq!(&ticks[start..start + CHIME_TICKS], chime.as_slice());
        assert!(
            ticks[start + CHIME_TICKS..]
                .iter()
                .all(|samples| quiet(samples))
        );
    }

    #[test]
    fn a_rom_without_sounds_boots_silent() {
        let system = System::new(
            info(),
            SystemSounds::default(),
            video_defaults(),
            cartridges(),
        );
        let (_, ticks) = boot_audio(system, &mut Psg::new());
        assert!(ticks.iter().all(|samples| quiet(samples)));
    }

    #[test]
    fn the_shell_starts_with_no_system_sound() {
        let mut psg = Psg::new();
        let (mut system, _) = boot_audio(new_system(), &mut psg);
        assert!(!system.sound.is_active());
        system.audio(&mut psg);
        assert_silent(&mut psg);

        let mut system = enter(enter(system, "tone 1 330 9"), "gate 1 on");
        assert_eq!(gates(&mut system, &mut psg), [false, true, false, false]);
        assert_eq!(voices(&mut system, &mut psg)[1], (Waveform::Square, 330, 9));
    }

    #[test]
    fn a_chime_cut_off_by_the_shell_leaves_nothing_behind() {
        let mut psg = Psg::new();
        let mut sound = SystemSound::new(sounds());
        sound.play(SystemSfx::Boot);
        let mut system = System {
            state: SystemState::Boot {
                boot: Boot::at_frame(boot::FINISH_FRAME - 2),
                cartridges: cartridges(),
            },
            info: info(),
            audio_source: AudioSource::Silent,
            sound,
            video_defaults: video_defaults(),
        };
        system = system.update(&[]);
        system.audio(&mut psg);
        assert!(gates(&mut system, &mut psg).iter().any(|&open| open));
        assert!(!quiet(&render_psg(&mut psg)));

        system = system.update(&[]);
        assert!(matches!(system.state(), SystemState::Shell { .. }));
        assert!(!system.sound.is_active());
        system.audio(&mut psg);
        assert_silent(&mut psg);
    }

    #[test]
    fn boot_runs_the_same_without_audio_output() {
        let mut psg = Psg::new();
        let mut heard = new_system();
        let mut unheard = new_system();
        while let SystemState::Boot { boot, .. } = heard.state() {
            let SystemState::Boot { boot: other, .. } = unheard.state() else {
                panic!("boot ended early without audio output");
            };
            assert_eq!(boot, other);
            heard = heard.update(&[]);
            heard.audio(&mut psg);
            unheard = unheard.update(&[]);
        }
        assert_eq!(shell_of(&heard), shell_of(&unheard));
    }

    #[test]
    fn every_boot_sounds_the_same() {
        let first = boot_audio(new_system(), &mut Psg::new()).1;
        assert_eq!(boot_audio(new_system(), &mut Psg::new()).1, first);
    }

    #[test]
    fn shell_tones_reach_the_psg_until_turned_off() {
        let mut psg = Psg::new();
        let mut system = shell();
        assert!(is_silent(&mut system, &mut psg));

        let system = enter(system, "tone 0 440 12");
        let mut system = enter(system, "tone 2 220 5 saw");
        let playing = voices(&mut system, &mut psg);
        assert_eq!(playing[0], (Waveform::Square, 440, 12));
        assert_eq!(playing[1].2, 0);
        assert_eq!(playing[2], (Waveform::Saw, 220, 5));
        assert_eq!(playing[3].2, 0);

        let mut system = enter(system, "tone 0 off");
        let playing = voices(&mut system, &mut psg);
        assert_eq!(playing[0].2, 0);
        assert_eq!(playing[2], (Waveform::Saw, 220, 5));

        let mut system = enter(system, "tone off");
        assert!(is_silent(&mut system, &mut psg));
    }

    #[test]
    fn shell_gates_and_envelopes_reach_the_psg() {
        let mut psg = Psg::new();
        let mut system = enter(shell(), "tone 0 440 12");
        assert_eq!(gates(&mut system, &mut psg), [false; CHANNELS]);
        assert_eq!(render_psg(&mut psg), vec![0; SAMPLES_PER_TICK]);

        let system = enter(system, "env 0 10 80 8 200");
        let mut system = enter(system, "gate 0 on");
        assert_eq!(gates(&mut system, &mut psg), [true, false, false, false]);
        let channel = psg.channel(0).unwrap();
        assert_eq!(
            channel.adsr(),
            Adsr {
                attack: 480,
                decay: 3840,
                sustain: 8,
                release: 9600,
            }
        );
        assert_eq!(channel.envelope_stage(), EnvelopeStage::Attack);

        // Rewriting the same voice every tick lets the envelope run on
        // instead of restarting it.
        for _ in 0..5 {
            gates(&mut system, &mut psg);
            render_psg(&mut psg);
        }
        assert_eq!(
            psg.channel(0).unwrap().envelope_stage(),
            EnvelopeStage::Sustain
        );

        let mut system = enter(system, "gate 0 off");
        assert_eq!(gates(&mut system, &mut psg), [false; CHANNELS]);
        assert_eq!(
            psg.channel(0).unwrap().envelope_stage(),
            EnvelopeStage::Release
        );

        let mut system = enter(enter(system, "gate 0 on"), "tone 0 off");
        assert_eq!(gates(&mut system, &mut psg), [false; CHANNELS]);
        assert_eq!(psg.channel(0).unwrap().volume(), 0);
    }

    fn render_psg(psg: &mut Psg) -> Vec<i16> {
        let mut out = vec![0; SAMPLES_PER_TICK];
        psg.render(&mut out);
        out
    }

    #[test]
    fn tones_are_muted_while_a_cartridge_runs_or_shows_its_pause_screen() {
        let mut psg = Psg::new();
        let fixture = Fixture::new("sys-tone");
        let system = enter(shell_with_cartridge(&fixture, ""), "tone 0 440 12");
        let system = enter(system, "tone 3 1200 4 noise");
        let mut system = enter(enter(system, "gate 0 on"), "gate 3 on");
        assert_eq!(gates(&mut system, &mut psg), [true, false, false, true]);

        let mut system = enter(enter(system, "load pong"), "run");
        assert!(is_running(&system));
        assert!(is_silent(&mut system, &mut psg));
        assert_eq!(gates(&mut system, &mut psg), [false; CHANNELS]);

        let mut system = press_break(system);
        assert!(is_paused_game(&system));
        assert!(is_silent(&mut system, &mut psg));
        assert_eq!(gates(&mut system, &mut psg), [false; CHANNELS]);

        let mut system = open_shell(system);
        let playing = voices(&mut system, &mut psg);
        assert_eq!(playing[0], (Waveform::Square, 440, 12));
        assert_eq!(playing[3], (Waveform::Noise, 1200, 4));
        assert_eq!(gates(&mut system, &mut psg), [true, false, false, true]);
    }

    // Cartridge audio.

    /// Plays the tick already written into `psg`, then advances `system` one
    /// tick and writes its sound, the way the main loop does. Returns the
    /// samples played.
    fn step(system: System, psg: &mut Psg) -> (System, Vec<i16>) {
        let samples = render_psg(psg);
        let mut system = system.update(&[]);
        system.audio(psg);
        (system, samples)
    }

    fn steps(mut system: System, psg: &mut Psg, n: usize) -> System {
        for _ in 0..n {
            system = step(system, psg).0;
        }
        system
    }

    /// Starts `script` and writes the sound of its start into `psg`.
    fn started_audio(fixture: &Fixture, script: &str, psg: &mut Psg) -> System {
        let mut system = started(fixture, script);
        assert!(is_running(&system));
        system.audio(psg);
        system
    }

    fn stage(psg: &Psg, index: usize) -> EnvelopeStage {
        psg.channel(index).unwrap().envelope_stage()
    }

    fn assert_silent(psg: &mut Psg) {
        for index in 0..CHANNELS {
            let channel = psg.channel(index).unwrap();
            assert_eq!(channel.volume(), 0, "channel {index}");
            assert_eq!(
                channel.envelope_stage(),
                EnvelopeStage::Idle,
                "channel {index}"
            );
        }
        assert_eq!(render_psg(psg), vec![0; SAMPLES_PER_TICK]);
    }

    /// Channel 0 holds a 100 ms attack, no decay and sustain 8: it reaches
    /// Sustain within 6 ticks and has room to attack again from there.
    const HELD_NOTE: &str = r#"
        audio.env(0, 100, 0, 8, 1000)
        audio.tone(0, 440, 12, "square")
        function boot() audio.gate(0, true) end
    "#;

    #[test]
    fn cartridge_audio_reaches_the_psg_in_samples() {
        let mut psg = Psg::new();
        let fixture = Fixture::new("sys-audio-units");
        let script = r#"
            audio.env(1, 10, 80, 8, 200)
            audio.tone(1, 660, 9, "triangle")
            audio.gate(1, true)
        "#;
        let _system = started_audio(&fixture, script, &mut psg);
        let channel = psg.channel(1).unwrap();
        assert_eq!(
            channel.adsr(),
            Adsr {
                attack: 480,
                decay: 3840,
                sustain: 8,
                release: 9600,
            }
        );
        assert_eq!(
            (channel.waveform(), channel.frequency(), channel.volume()),
            (Waveform::Triangle, 660, 9)
        );
        assert_eq!(channel.envelope_stage(), EnvelopeStage::Attack);
        assert!(render_psg(&mut psg).iter().any(|&sample| sample != 0));
    }

    #[test]
    fn cartridge_gates_open_retrigger_and_release() {
        let mut psg = Psg::new();
        let fixture = Fixture::new("sys-audio-gate");
        let script = format!(
            r#"{HELD_NOTE}
            function tick()
                if frame() == 10 then audio.gate(0, true) end
                if frame() == 20 then audio.gate(0, false) end
            end
            "#
        );
        let system = started_audio(&fixture, &script, &mut psg);
        assert_eq!(stage(&psg, 0), EnvelopeStage::Attack);

        let system = steps(system, &mut psg, 10);
        assert_eq!(stage(&psg, 0), EnvelopeStage::Sustain);

        // Opening an open gate retriggers the attack from the held level.
        let system = steps(system, &mut psg, 1);
        assert_eq!(stage(&psg, 0), EnvelopeStage::Attack);
        assert!(psg.channel(0).unwrap().gate());

        let _system = steps(system, &mut psg, 10);
        assert_eq!(stage(&psg, 0), EnvelopeStage::Release);
        assert!(!psg.channel(0).unwrap().gate());
    }

    #[test]
    fn cartridge_tone_changes_do_not_retrigger_the_envelope() {
        let mut psg = Psg::new();
        let fixture = Fixture::new("sys-audio-tone");
        let script = format!(
            r#"{HELD_NOTE}
            function tick()
                if frame() == 10 then audio.tone(0, 880, 5, "saw") end
            end
            "#
        );
        let system = steps(started_audio(&fixture, &script, &mut psg), &mut psg, 10);
        assert_eq!(stage(&psg, 0), EnvelopeStage::Sustain);
        let level = psg.channel(0).unwrap().envelope_level();

        let _system = steps(system, &mut psg, 1);
        let channel = psg.channel(0).unwrap();
        assert_eq!(
            (channel.waveform(), channel.frequency(), channel.volume()),
            (Waveform::Saw, 880, 5)
        );
        assert_eq!(channel.envelope_stage(), EnvelopeStage::Sustain);
        assert_eq!(channel.envelope_level(), level);
    }

    #[test]
    fn gate_writes_within_one_tick_all_reach_the_psg() {
        let mut psg = Psg::new();
        let fixture = Fixture::new("sys-audio-blip");
        // Opening and closing in the same tick with an instant attack leaves
        // a release from full level; the final gate state alone would leave
        // the channel idle.
        let script = r#"
            audio.env(0, 0, 0, 15, 1000)
            audio.tone(0, 440, 12, "square")
            audio.gate(0, true)
            audio.gate(0, false)
        "#;
        let _system = started_audio(&fixture, script, &mut psg);
        let channel = psg.channel(0).unwrap();
        assert_eq!(channel.envelope_stage(), EnvelopeStage::Release);
        assert_eq!(channel.envelope_level(), ENVELOPE_FULL);
    }

    #[test]
    fn cartridge_audio_persists_across_ticks() {
        let mut psg = Psg::new();
        let fixture = Fixture::new("sys-audio-persist");
        let script = r#"
            function boot()
                audio.tone(2, 330, 7, "noise")
                audio.gate(2, true)
            end
            function tick() end
        "#;
        let mut system = started_audio(&fixture, script, &mut psg);
        for _ in 0..30 {
            let (next, samples) = step(system, &mut psg);
            system = next;
            assert!(samples.iter().any(|&sample| sample != 0));
        }
        let channel = psg.channel(2).unwrap();
        assert_eq!(
            (channel.waveform(), channel.frequency(), channel.volume()),
            (Waveform::Noise, 330, 7)
        );
        assert!(channel.gate());
    }

    #[test]
    fn pausing_silences_the_cartridge_and_resuming_reattacks_open_gates() {
        let fixture = Fixture::new("sys-audio-pause");
        let resumes: [fn(System) -> System; 2] =
            [press_break, |system| enter(open_shell(system), "resume")];
        for resume in resumes {
            let mut psg = Psg::new();
            let system = steps(started_audio(&fixture, HELD_NOTE, &mut psg), &mut psg, 10);
            assert_eq!(stage(&psg, 0), EnvelopeStage::Sustain);

            let mut system = press_break(system);
            system.audio(&mut psg);
            assert!(is_paused_game(&system));
            assert_silent(&mut psg);

            let mut system = resume(system);
            assert!(is_running(&system));
            system.audio(&mut psg);
            let channel = psg.channel(0).unwrap();
            assert_eq!(
                (channel.waveform(), channel.frequency(), channel.volume()),
                (Waveform::Square, 440, 12)
            );
            assert_eq!(channel.adsr().sustain, 8);
            assert_eq!(channel.envelope_stage(), EnvelopeStage::Attack);
            assert_eq!(channel.envelope_level(), 0);
        }
    }

    #[test]
    fn stopping_or_failing_leaves_the_psg_silent() {
        let fixture = Fixture::new("sys-audio-stop");
        let mut psg = Psg::new();
        let system = steps(started_audio(&fixture, HELD_NOTE, &mut psg), &mut psg, 3);
        let mut system = stop_cartridge(system);
        system.audio(&mut psg);
        assert_eq!(loaded_name(&system), Some("Pong"));
        assert_silent(&mut psg);

        let mut psg = Psg::new();
        let script =
            format!(r#"{HELD_NOTE} function tick() if frame() == 3 then error("boom") end end"#);
        let system = steps(started_audio(&fixture, &script, &mut psg), &mut psg, 4);
        assert!(!is_running(&system));
        assert_silent(&mut psg);
    }

    #[test]
    fn shell_and_cartridge_audio_stay_isolated() {
        let mut psg = Psg::new();
        let fixture = Fixture::new("sys-audio-isolated");
        let script = r#"
            audio.tone(1, 660, 9, "saw")
            audio.gate(1, true)
        "#;
        let system = enter(shell_with_cartridge(&fixture, script), "tone 0 440 12");
        let mut system = enter(system, "gate 0 on");
        assert_eq!(gates(&mut system, &mut psg), [true, false, false, false]);

        // The shell's voice does not play over the cartridge.
        let mut system = enter(enter(system, "load pong"), "run");
        assert_eq!(gates(&mut system, &mut psg), [false, true, false, false]);
        assert_eq!(voices(&mut system, &mut psg)[0].2, 0);

        // The cartridge's voice does not play over the shell, and the shell's
        // own voice returns.
        let mut system = open_shell(press_break(system));
        assert_eq!(gates(&mut system, &mut psg), [true, false, false, false]);
        assert_eq!(voices(&mut system, &mut psg)[1].2, 0);

        let mut system = enter(system, "stop");
        assert_eq!(gates(&mut system, &mut psg), [true, false, false, false]);
        assert_eq!(voices(&mut system, &mut psg)[1].2, 0);
    }

    #[test]
    fn same_cartridge_writes_produce_identical_psg_state() {
        let script = r#"
            audio.env(0, 5, 40, 6, 120)
            audio.env(3, 0, 20, 4, 60)
            audio.tone(0, 440, 12, "square")
            audio.tone(1, 660, 6, "triangle")
            audio.tone(2, 220, 5, "saw")
            audio.tone(3, 1200, 4, "noise")
            function tick()
                local f = frame()
                audio.gate(f % 4, f % 3 ~= 0)
                if f % 5 == 0 then audio.tone(2, 220 + f, 5, "saw") end
            end
        "#;
        let run = |fixture: &Fixture, shell_tones: bool| {
            let mut psg = Psg::new();
            let mut system = shell_with_cartridge(fixture, script);
            if shell_tones {
                system = enter(enter(system, "tone 2 1000 15 noise"), "gate 2 on");
                system = steps(system, &mut psg, 7);
            }
            let mut system = enter(enter(system, "load pong"), "run");
            system.audio(&mut psg);
            let mut samples = Vec::new();
            for _ in 0..30 {
                let (next, played) = step(system, &mut psg);
                system = next;
                samples.extend(played);
            }
            (samples, psg)
        };
        let (samples, psg) = run(&Fixture::new("sys-audio-determinism-a"), false);
        assert!(samples.iter().any(|&sample| sample != 0));
        assert_eq!(
            run(&Fixture::new("sys-audio-determinism-b"), false),
            (samples.clone(), psg.clone())
        );
        assert_eq!(
            run(&Fixture::new("sys-audio-determinism-c"), true),
            (samples, psg)
        );
    }

    // Creating cartridges.

    const WELCOME: [&str; 7] = [
        "Your new cartridge is ready!",
        "My Game",
        "",
        "Every great game starts somewhere.",
        "This one starts with you.",
        "",
        "Try 'run' to see your first creation.",
    ];

    #[test]
    fn new_loads_the_created_cartridge_without_running_it() {
        let fixture = Fixture::new("sys-new");
        let system = enter(shell_with_cartridge(&fixture, ""), "new my-game");
        assert!(matches!(system.state(), SystemState::Shell { .. }));
        assert_eq!(loaded_name(&system), Some("My Game"));
        assert_eq!(after(&system, "> new my-game"), WELCOME);

        let system = enter(system, "info");
        assert_eq!(transcript(&system).last().unwrap(), "STATE       LOADED");
    }

    #[test]
    fn a_failed_new_announces_nothing_and_keeps_the_loaded_cartridge() {
        let fixture = Fixture::new("sys-new-fail");
        let system = enter(shell_with_cartridge(&fixture, ""), "load pong");
        assert_eq!(after(&system, "> load pong"), ["Loaded Pong 0.1.0"]);
        let system = enter(system, "new pong");
        assert_eq!(after(&system, "> new pong"), ["! /pong already exists"]);
        assert_eq!(loaded_name(&system), Some("Pong"));
    }

    #[test]
    fn new_then_run_starts_the_starter_cartridge() {
        let fixture = Fixture::new("sys-new-run");
        let system = enter(
            enter(shell_with_cartridge(&fixture, ""), "new my-game"),
            "run",
        );
        assert!(is_running(&system));
        let system = ticks(system, 120);
        assert!(is_running(&system));
        assert_eq!(after(&system, "> run"), ["Running My Game 0.1.0"]);
    }

    /// Per-tick input that presses `button` on the first tick, releases it
    /// after `held` ticks, and lasts `total` ticks.
    fn hold(button: Button, held: usize, total: usize) -> Vec<Vec<InputEvent>> {
        let mut inputs = vec![Vec::new(); total];
        inputs[0].push(InputEvent::ButtonDown(button));
        if held < total {
            inputs[held].push(InputEvent::ButtonUp(button));
        }
        inputs
    }

    /// Per-tick input that taps `button` (pressed for one tick) at each of
    /// `at`, lasting `total` ticks.
    fn taps(button: Button, at: &[usize], total: usize) -> Vec<Vec<InputEvent>> {
        let mut inputs = vec![Vec::new(); total];
        for &tick in at {
            inputs[tick].push(InputEvent::ButtonDown(button));
            inputs[tick + 1].push(InputEvent::ButtonUp(button));
        }
        inputs
    }

    /// The frame a freshly created and started starter cartridge shows after
    /// one tick per entry of `inputs`. `name` keeps each run's storage apart.
    ///
    /// Runs compared with each other last the same number of ticks, so
    /// anything the starter animates by time alone matches between them.
    fn starter_frame(name: &str, inputs: &[Vec<InputEvent>]) -> Framebuffer {
        let fixture = Fixture::new(name);
        let mut system = enter(enter(shell_with_cartridge(&fixture, ""), "new owl"), "run");
        for input in inputs {
            system = system.update(input);
        }
        assert!(is_running(&system), "{:?}", transcript(&system));
        render(&system)
    }

    /// The leftmost and rightmost columns where `a` and `b` differ.
    fn differing_columns(a: &Framebuffer, b: &Framebuffer) -> Option<(i32, i32)> {
        let columns: Vec<i32> = (0..WIDTH as i32)
            .filter(|&x| (0..HEIGHT as i32).any(|y| a.pixel(x, y) != b.pixel(x, y)))
            .collect();
        Some((*columns.first()?, *columns.last()?))
    }

    #[test]
    fn the_starter_owl_follows_the_arrows() {
        let idle = starter_frame("sys-starter-idle", &vec![Vec::new(); 30]);
        let right = starter_frame("sys-starter-right", &hold(Button::Right, 20, 30));
        let left = starter_frame("sys-starter-left", &hold(Button::Left, 20, 30));
        let (right_min, right_max) = differing_columns(&idle, &right).unwrap();
        let (left_min, left_max) = differing_columns(&idle, &left).unwrap();
        assert!(
            right_min + right_max > left_min + left_max,
            "right {right_min}..{right_max}, left {left_min}..{left_max}"
        );
    }

    #[test]
    fn the_starter_owl_stays_on_screen() {
        let stopped = starter_frame("sys-starter-stop", &hold(Button::Left, 300, 600));
        let held = starter_frame("sys-starter-held", &hold(Button::Left, 600, 600));
        assert!(stopped == held, "holding Left longer moved the owl further");

        let idle = starter_frame("sys-starter-idle-long", &vec![Vec::new(); 600]);
        let (min, _) = differing_columns(&idle, &held).unwrap();
        assert_eq!(min, 0, "the owl rests against the left edge");
    }

    #[test]
    fn the_starter_owl_reacts_to_a() {
        let idle = starter_frame("sys-starter-a-idle", &vec![Vec::new(); 10]);
        let pressed = starter_frame("sys-starter-a-press", &taps(Button::A, &[0], 10));
        assert!(differing_columns(&idle, &pressed).is_some());

        // Long after the hop and the speech bubble, the message still shows
        // how many times the owl was greeted.
        let once = starter_frame("sys-starter-a-once", &taps(Button::A, &[0], 150));
        let twice = starter_frame("sys-starter-a-twice", &taps(Button::A, &[0, 4], 150));
        assert!(differing_columns(&once, &twice).is_some());
    }
}
