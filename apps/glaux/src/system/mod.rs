//! System software: the boot sequence, the shell that follows it, and the
//! cartridge runtime the shell starts.
//!
//! The system advances one step per logical tick and redraws the whole
//! framebuffer from its current state on every frame.

mod boot;
mod line_editor;
mod pause;
mod running;
mod shell;

pub use boot::Boot;
pub use shell::Shell;

use glaux_input::{InputEvent, InputMode};
use glaux_luau::RuntimeError;
use glaux_machine::framebuffer::{Framebuffer, HEIGHT, WIDTH};
use glaux_machine::machine::{GENERATION, NAME};
use glaux_machine::system_rom::SystemRom;
use glaux_video::draw::{clear, draw_text, fill_rect};
use pause::PauseOverlay;
use running::{FailedStart, RunningCartridge, StartError};
use semver::Version;
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

/// The system UI state machine.
#[derive(Debug)]
pub struct System {
    state: SystemState,
    info: SystemInfo,
}

impl System {
    /// Starts at frame 0 of the boot sequence. The shell that follows
    /// navigates `cartridges`.
    pub fn new(info: SystemInfo, cartridges: CartridgeStorage) -> Self {
        Self {
            state: SystemState::Boot {
                boot: Boot::new(),
                cartridges,
            },
            info,
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
        let Self { state, info } = self;
        let state = match state {
            SystemState::Boot {
                mut boot,
                cartridges,
            } => {
                boot.tick();
                if boot.is_finished() {
                    tracing::info!("boot finished, entering shell");
                    SystemState::Shell {
                        shell: Box::new(Shell::new(info.clone(), cartridges)),
                        loaded: None,
                    }
                } else {
                    SystemState::Boot { boot, cartridges }
                }
            }
            SystemState::Shell { shell, loaded } => shell_tick(shell, loaded, input),
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
        Self { state, info }
    }

    /// Redraws the whole framebuffer for the current state using the ROM's
    /// font.
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
                overlay.render(framebuffer, rom.palette());
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

/// Feeds `input` to the shell and carries out the cartridge requests its
/// commands make.
///
/// A started cartridge does not take input, so events that follow a
/// successful `run` in the same tick are dropped.
fn shell_tick(
    mut shell: Box<Shell>,
    mut loaded: Option<LoadedCartridge>,
    input: &[InputEvent],
) -> SystemState {
    // Input is deliberately not logged: user input stays out of
    // application logs.
    for (index, &event) in input.iter().enumerate() {
        match shell.handle_input(event, CartridgeSlot::loaded(loaded.as_ref())) {
            None => {}
            Some(ShellRequest::Load(cartridge)) => loaded = Some(cartridge),
            Some(ShellRequest::Unload) => {
                let Some(cartridge) = loaded.take() else {
                    unreachable!("UNLOAD was requested without a loaded cartridge");
                };
                let identity = cartridge_identity(cartridge.manifest());
                tracing::info!(cartridge = %identity, "cartridge unloaded");
                shell.report(format!("UNLOADED {identity}"));
            }
            Some(ShellRequest::Run) => {
                // `run` only asks to start when a cartridge is loaded.
                let Some(cartridge) = loaded else {
                    unreachable!("RUN was requested without a loaded cartridge");
                };
                return match start(shell, cartridge) {
                    SystemState::Shell { shell, loaded } => {
                        shell_tick(shell, loaded, &input[index + 1..])
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
fn start(mut shell: Box<Shell>, cartridge: LoadedCartridge) -> SystemState {
    let identity = cartridge_identity(cartridge.manifest());
    match RunningCartridge::start(cartridge) {
        Ok(mut running) => {
            tracing::info!(cartridge = %identity, "cartridge started");
            shell.report(format!("RUNNING {identity}"));
            shell.log(running.take_output());
            SystemState::Running { shell, running }
        }
        Err(failed) => {
            let FailedStart { cartridge, error } = *failed;
            match error {
                StartError::Cartridge(err) => shell.report_error(err.to_string()),
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
    shell.report(format!("PAUSED {identity}"));
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
                shell.report("TYPE RESUME TO CONTINUE OR STOP TO END");
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
    for &event in input {
        match shell.handle_input(event, CartridgeSlot::Paused(running.cartridge())) {
            None => {}
            Some(ShellRequest::Resume) => return resume(shell, running),
            Some(ShellRequest::Stop) => return stop(shell, running, StopReason::Stopped),
            // `load`, `unload` and `run` refuse to replace a paused cartridge.
            Some(_request @ (ShellRequest::Load(_) | ShellRequest::Unload | ShellRequest::Run)) => {
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
    shell.report(format!("RESUMED {identity}"));
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
            shell.report(format!("STOPPED {identity}"));
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

    fn info() -> SystemInfo {
        SystemInfo {
            rom_version: Version::new(0, 1, 0),
        }
    }

    fn cartridges() -> CartridgeStorage {
        CartridgeStorage::Unavailable
    }

    fn new_system() -> System {
        System::new(info(), cartridges())
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

        for _ in 0..100 {
            system = system.update(&[]);
        }
        assert_eq!(shell_of(&system), &Shell::new(info(), cartridges()));
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
        // in column 2; typing puts 'A' there and moves the cursor to 3.
        let ink = |fb: &Framebuffer, col: i32| fb.pixel((col + 1) * CELL_W, 2 * CELL_H);
        assert_eq!(
            (ink(&empty, 2), ink(&empty, 3)),
            (Some(TEXT), Some(BACKGROUND))
        );
        assert_eq!((ink(&typed, 2), ink(&typed, 3)), (Some(TEXT), Some(TEXT)));
    }

    #[test]
    fn submitted_line_stays_on_screen_above_a_new_prompt() {
        let mut system = shell();
        system = system.update(&[InputEvent::Text('A'), InputEvent::Enter]);

        let fb = render(&system);
        // Row 1 keeps "> A"; the error follows on row 2 and the new, empty
        // prompt on row 3 with its cursor in column 2.
        let glyph = |col: i32, row: i32| fb.pixel((col + 1) * CELL_W, (row + 1) * CELL_H);
        assert_eq!(glyph(2, 1), Some(TEXT));
        assert_eq!(glyph(0, 2), Some(ERROR));
        assert_eq!((glyph(0, 3), glyph(2, 3)), (Some(TEXT), Some(TEXT)));
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
        std::fs::write(dir.join("main.luau"), script).unwrap();
        let mut system = System::new(info(), fixture.storage());
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
            SystemState::Shell { loaded, .. } => {
                loaded.as_ref().map(|c| c.manifest().name.as_str())
            }
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
            ["RUNNING Pong 0.1.0", "entry", "boot"]
        );

        let system = stop_cartridge(ticks(system, 3));
        assert_eq!(
            after(&system, "> run"),
            [
                "RUNNING Pong 0.1.0",
                "entry",
                "boot",
                "tick    1",
                "draw    1",
                "tick    2",
                "draw    2",
                "tick    3",
                "draw    3",
                "PAUSED Pong 0.1.0",
                "TYPE RESUME TO CONTINUE OR STOP TO END",
                "> stop",
                "STOPPED Pong 0.1.0",
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
                "RUNNING Pong 0.1.0",
                "PAUSED Pong 0.1.0",
                "TYPE RESUME TO CONTINUE OR STOP TO END"
            ]
        );
        // Nothing typed while running reached the command line.
        let system = system.update(&[InputEvent::Enter]);
        assert_eq!(
            transcript(&system).last().unwrap(),
            "TYPE RESUME TO CONTINUE OR STOP TO END"
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
                "RUNNING Pong 0.1.0",
                "1",
                "PAUSED Pong 0.1.0",
                "TYPE RESUME TO CONTINUE OR STOP TO END",
                "> stop",
                "STOPPED Pong 0.1.0"
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
                vec!["x", "! RUNTIME ERROR: main.luau:1: boom"],
            ),
            (
                r#"log.info("e") function boot() log.info("b") error("no") end"#,
                vec!["e", "b", "! RUNTIME ERROR IN BOOT: main.luau:1: no"],
            ),
            (
                "function boot() while true do end end",
                vec!["! EXECUTION LIMIT EXCEEDED IN BOOT"],
            ),
            ("while true do end", vec!["! EXECUTION LIMIT EXCEEDED"]),
        ] {
            let system = started(&fixture, script);
            assert_eq!(after(&system, "> run"), expected, "{script}");
            assert_eq!(loaded_name(&system), Some("Pong"), "{script}");
        }

        let system = started(&fixture, "local = 1");
        let lines = after(&system, "> run");
        assert!(
            matches!(lines.as_slice(), [only] if only.starts_with("! SYNTAX ERROR: main.luau:1:")),
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
                "! RUNTIME ERROR IN TICK: main.luau:1: boom",
            ),
            (
                r#"function tick() log.info("t") end function draw() error("bad") end"#,
                "! RUNTIME ERROR IN DRAW: main.luau:1: bad",
            ),
            (
                r#"function tick() log.info("t") while true do end end"#,
                "! EXECUTION LIMIT EXCEEDED IN TICK",
            ),
            (
                r#"function tick() log.info("t") end
                   function draw() pcall(function() while true do end end) end"#,
                "! EXECUTION LIMIT EXCEEDED IN DRAW",
            ),
        ] {
            let system = ticks(started(&fixture, script), 1);
            assert_eq!(
                after(&system, "> run"),
                ["RUNNING Pong 0.1.0", "t", error],
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
        assert_eq!(after(&system, "> unload"), ["UNLOADED Pong 0.1.0"]);

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
        let manifest = std::fs::read(dir.join("cart.json")).unwrap();

        let mut system = System::new(info(), fixture.storage());
        while matches!(system.state(), SystemState::Boot { .. }) {
            system = system.update(&[]);
        }
        let system = enter(enter(enter(system, "cd games"), "load pong"), "unload");
        let system = enter(system, "pwd");

        assert_eq!(transcript(&system).last().unwrap(), "/games");
        assert_eq!(std::fs::read(dir.join("cart.json")).unwrap(), manifest);
        assert_eq!(std::fs::read(dir.join("save.dat")).unwrap(), b"kept");
    }

    #[test]
    fn unload_without_a_cartridge_reports_an_error() {
        let fixture = Fixture::new("sys-unload-empty");
        let system = enter(shell_with_cartridge(&fixture, ""), "unload");
        assert_eq!(after(&system, "> unload"), ["! NO CARTRIDGE LOADED"]);
        assert_eq!(loaded_name(&system), None);
    }

    #[test]
    fn missing_entry_is_reported_at_run() {
        let fixture = Fixture::new("sys-run-missing");
        let system = enter(shell_with_cartridge(&fixture, ""), "load pong");
        std::fs::remove_file(fixture.root().join("pong/main.luau")).unwrap();
        let system = enter(system, "run");
        assert_eq!(after(&system, "> run"), ["! ENTRY NOT FOUND: main.luau"]);
        assert_eq!(loaded_name(&system), Some("Pong"));
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
                "RUNNING Pong 0.1.0",
                "! RUNTIME ERROR IN DRAW: main.luau:1: bad argument #3 to 'pset' \
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
                "RUNNING Pong 0.1.0",
                "! RUNTIME ERROR IN TICK: main.luau:1: 'cls' can only be called during draw()",
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
            ["RUNNING Pong 0.1.0", "t", "w", "PAUSED Pong 0.1.0"]
        );
    }

    // Game input.

    use glaux_input::GameButton;

    const UP_STATE: &str = r#"function tick() log.info(btn("up"), btnp("up")) end"#;

    fn down() -> InputEvent {
        InputEvent::ButtonDown(GameButton::Up)
    }

    fn up() -> InputEvent {
        InputEvent::ButtonUp(GameButton::Up)
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
                &[up(), InputEvent::ButtonDown(GameButton::A)],
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
            ["RUNNING Pong 0.1.0", "PAUSED Pong 0.1.0"]
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
        assert_eq!(transcript(&system).last().unwrap(), "PAUSED Pong 0.1.0");
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
        assert_eq!(transcript(&system).last().unwrap(), "RESUMED Pong 0.1.0");
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
            "TYPE RESUME TO CONTINUE OR STOP TO END"
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
        assert_eq!(after(&system, "> resume"), ["RESUMED Pong 0.1.0"]);
        let (_, lines) = logged_ticks(system, 1);
        assert_eq!(lines, ["tick    3"]);
    }

    #[test]
    fn stop_disposes_of_the_paused_vm() {
        let fixture = Fixture::new("sys-pause-stop");
        let system = stop_cartridge(ticks(started(&fixture, COUNTER), 2));
        assert_eq!(loaded_name(&system), Some("Pong"));
        assert_eq!(after(&system, "> stop"), ["STOPPED Pong 0.1.0"]);

        let system = enter(system, "run");
        assert_eq!(
            after(&system, "> run"),
            ["RUNNING Pong 0.1.0", "entry", "boot"]
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
                "! CARTRIDGE PAUSED: RESUME OR STOP IT FIRST",
                "> run",
                "! CARTRIDGE PAUSED: RESUME OR STOP IT FIRST",
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
            ["! CARTRIDGE PAUSED: RESUME OR STOP IT FIRST"]
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
            "! RUNTIME ERROR IN TICK: main.luau:1: boom"
        );
        let system = enter(system, "resume");
        assert_eq!(after(&system, "> resume"), ["! NO PAUSED CARTRIDGE"]);
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
    fn a_press_in_the_pausing_tick_is_dropped() {
        let fixture = Fixture::new("sys-pause-input-same-tick");
        let system = started(&fixture, UP_STATE).update(&[down(), InputEvent::Break]);
        let (_, lines) = logged_per_tick(press_break(system), &[&[], &[up()], &[down()]]);
        assert_eq!(lines, ["false   false", "false   false", "true    true"]);
    }
}
