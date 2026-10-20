//! System software, made of the boot sequence, the shell that follows it,
//! the editor workspace beside it, and the cartridge runtime the shell
//! starts.
//!
//! The system advances one step per logical tick and redraws the whole
//! framebuffer from its current state on every frame.

mod audio;
mod boot;
mod clipboard;
mod code_cursor;
mod code_document;
mod code_edit;
mod code_history;
mod code_search;
mod code_selection;
mod code_session;
mod code_syntax;
mod code_text;
mod credits;
mod docs;
mod editor_settings;
mod external_url;
mod line_editor;
mod pause;
mod running;
mod sfx;
mod shell;
mod sprite_document;
mod syntax_theme;
#[cfg(test)]
mod test_font;
#[cfg(test)]
mod test_frames;
mod ui_errors;
mod unsaved;
mod video;
mod workspace;

pub use boot::Boot;
pub use clipboard::Clipboard;
use code_session::CodeSession;
use credits::CreditsView;
#[cfg(test)]
pub use editor_settings::IndentStyle;
pub use editor_settings::{EditorSettings, INDENT_SIZES, TAB_SIZES};
pub use external_url::{ExternalUrl, ExternalUrlError};
pub use shell::Shell;
use sprite_document::SpriteDocument;
pub use video::CartridgeVideoDefaults;
use workspace::{Tab, Workspace, WorkspaceRequest, WorkspaceVideo};

use glaux_i18n::Arg;
use glaux_input::{InputEvent, InputMode, Point};
use glaux_machine::font::BitmapFont;
use glaux_machine::framebuffer::{Framebuffer, HEIGHT, WIDTH};
use glaux_machine::machine::{GENERATION, NAME};
use glaux_machine::palette::Palette;
use glaux_machine::psg::Psg;
use glaux_machine::system_rom::{SystemCursor, SystemRom, SystemSounds};
use glaux_script::RuntimeError;
use glaux_video::draw::{clear, draw_text, fill_rect};
use pause::PauseOverlay;
use running::{FailedStart, RunningCartridge, StartError};
use semver::Version;
use sfx::SystemSound;
use shell::{CartridgeSlot, ShellRequest, cartridge_identity};
use unsaved::{Edits, Outcome, Pending, UnsavedFlow};

use std::collections::VecDeque;
use std::path::PathBuf;

use crate::cartridge::LoadedCartridge;
use crate::cartridge::source::{SourceEditError, SourcePath};
use crate::cartridge_fs::CartridgeStorage;
use crate::sprite_import::SheetSource;
use crate::ui_text;

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
    /// Break opens the workspace on a loaded cartridge.
    Shell {
        shell: Box<Shell>,
        loaded: Option<LoadedCartridge>,
    },
    /// The credits view is the active UI. The shell keeps its state, taking
    /// no input, and Escape on one of the view's main pages returns to it.
    Credits {
        shell: Box<Shell>,
        loaded: Option<LoadedCartridge>,
        view: Box<CreditsView>,
    },
    /// The editor workspace is the active UI, working on the loaded
    /// cartridge without starting it. The shell keeps its state, taking no
    /// input, and Break returns to it.
    Workspace {
        shell: Box<Shell>,
        cartridge: LoadedCartridge,
        /// How the workspace shows the cartridge's colors, prepared when it
        /// opened.
        video: WorkspaceVideo,
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
fn paused_shell_hint() -> String {
    ui_text::get("system-type-resume-to-continue-or-stop-to-end")
}

/// Shown when Break asks for the workspace with no cartridge loaded. Fits
/// one shell row.
fn no_cartridge_hint() -> String {
    ui_text::get("system-nothing-to-edit-try-new-id-or-load-path")
}

/// Facts about the running system that the shell reports.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SystemInfo {
    /// Version of the System ROM the system runs from.
    pub rom_version: Version,
}

/// `GLAUX G1  SYSTEM ROM x.y.z`, shared by the boot header and `VERSION`.
fn identity_line(rom_version: &Version) -> String {
    ui_text::get_with(
        "system-identity",
        &[
            ("name", Arg::Text(NAME)),
            ("generation", Arg::Text(GENERATION)),
            ("rom-version", Arg::Text(&rom_version.to_string())),
        ],
    )
}

/// Work the system asks of the host, outside the machine. See
/// [`System::take_host_request`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostRequest {
    /// Show the host's file dialog to choose one PNG file, opening in
    /// `directory`. Answer with [`System::file_picked`].
    PickPng { directory: PathBuf },
    /// Keep these Code view settings for the next launch.
    SaveEditorSettings(EditorSettings),
    /// Open this link in the program the operating system uses for it. The
    /// system asked because the user chose the link.
    OpenExternal(ExternalUrl),
}

/// How the host's file dialog answered a [`HostRequest::PickPng`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PickedFile {
    Chosen(PathBuf),
    /// Closed without a file. The host may not tell this apart from a
    /// dialog that failed to show.
    Cancelled,
    /// The dialog could not be shown, for this reason.
    Unavailable(String),
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
    /// Kept across visits, so the workspace reopens on the tab it was left
    /// on.
    workspace: Workspace,
    /// The loaded cartridge's video state as the editors changed it. When
    /// present it always belongs to the cartridge that is loaded, running
    /// or paused. It is dropped whenever that cartridge is replaced or
    /// unloaded, which the shell refuses while it holds unsaved edits.
    sprites: Option<SpriteDocument>,
    /// The loaded cartridge's open scripts and the Code view's tabs over
    /// them. Like `sprites`, it always belongs to the loaded cartridge and
    /// is dropped only once another cartridge has loaded or this one is
    /// unloaded, so a failed `load` keeps it.
    code: Option<CodeSession>,
    /// Host work asked for and not yet taken by the host, oldest first.
    host_requests: VecDeque<HostRequest>,
    /// The question about unsaved edits that holds up quitting or a shell
    /// command. While open it takes every input, whatever the state.
    unsaved: Option<UnsavedFlow>,
    /// Quitting was asked for with unsaved edits, and the next update asks
    /// about them.
    exit_requested: bool,
    /// Quitting was confirmed. See [`exit_confirmed`](Self::exit_confirmed).
    exiting: bool,
}

/// What the states lend the shell and the question about unsaved edits.
struct Context<'a> {
    video_defaults: &'a CartridgeVideoDefaults,
    workspace: &'a mut Workspace,
    sprites: &'a mut Option<SpriteDocument>,
    code: &'a mut Option<CodeSession>,
    unsaved: &'a mut Option<UnsavedFlow>,
}

impl Context<'_> {
    /// Whether the editors hold edits not written to the cartridge.
    fn has_unsaved(&self) -> bool {
        self.sprites
            .as_ref()
            .is_some_and(SpriteDocument::is_modified)
            || self.code.as_ref().is_some_and(CodeSession::is_modified)
    }
}

impl System {
    /// Starts at frame 0 of the boot sequence, which plays the System ROM's
    /// `sounds`. The shell that follows navigates `cartridges`, and
    /// cartridges it starts fall back to `video_defaults`. The workspace is
    /// laid out for the System ROM's `font` and drawn in the System ROM's
    /// `palette`.
    pub fn new(
        info: SystemInfo,
        sounds: SystemSounds,
        video_defaults: CartridgeVideoDefaults,
        cartridges: CartridgeStorage,
        font: &BitmapFont,
        palette: Palette,
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
            workspace: Workspace::new(font, palette),
            sprites: None,
            code: None,
            host_requests: VecDeque::new(),
            unsaved: None,
            exit_requested: false,
            exiting: false,
        }
    }

    /// Asks to quit Glaux, as closing the window does. Without unsaved
    /// edits quitting is confirmed at once. With them, the next update
    /// asks whether to save them, pausing a running cartridge first. A
    /// request while that question is open is ignored.
    pub fn request_exit(&mut self) {
        if self.exiting || self.unsaved.is_some() {
            return;
        }
        let unsaved = self
            .sprites
            .as_ref()
            .is_some_and(SpriteDocument::is_modified)
            || self.code.as_ref().is_some_and(CodeSession::is_modified);
        if unsaved {
            tracing::info!("quit requested with unsaved edits");
            self.exit_requested = true;
        } else {
            self.exiting = true;
        }
    }

    /// Whether Glaux should quit now: asked to, and its edits saved or
    /// discarded.
    pub fn exit_confirmed(&self) -> bool {
        self.exiting
    }

    #[cfg(test)]
    fn unsaved_flow(&self) -> Option<&UnsavedFlow> {
        self.unsaved.as_ref()
    }

    /// Shows code in the workspace with `settings`, the ones kept from an
    /// earlier launch.
    pub fn with_editor_settings(mut self, settings: EditorSettings) -> Self {
        self.workspace.set_editor_settings(settings);
        self
    }

    #[cfg(test)]
    pub fn state(&self) -> &SystemState {
        &self.state
    }

    /// The kind of input the current state consumes: game buttons while a
    /// cartridge runs, text and the pointer in the workspace, and text
    /// everywhere else, including while a cartridge is paused.
    pub fn input_mode(&self) -> InputMode {
        match self.state {
            SystemState::Running { .. } => InputMode::Game,
            // The credits view reads the same keys as the workspace, but
            // draws no pointer.
            SystemState::Workspace { .. } | SystemState::Credits { .. } => InputMode::Workspace,
            SystemState::Boot { .. } | SystemState::Shell { .. } | SystemState::Paused { .. } => {
                InputMode::Text
            }
        }
    }

    /// Draws the system's mouse pointer with its hotspot on `pointer`, the
    /// logical pixel under the host's pointer, in the shape the workspace
    /// picks for what is under it. Only the workspace, the one
    /// pointer-driven state, shows it.
    ///
    /// Call on a copy of the frame drawn by [`render`](Self::render), once
    /// per presented frame. The pointer then follows the host at its
    /// refresh rate rather than the tick rate, and never ends up in the
    /// machine's framebuffer.
    pub fn draw_cursor(
        &self,
        framebuffer: &mut Framebuffer,
        rom: &SystemRom,
        pointer: Option<Point>,
    ) {
        if !matches!(self.state, SystemState::Workspace { .. }) {
            return;
        }
        let (cursor, frame) = match (pointer, &self.unsaved) {
            (Some(at), Some(flow)) => {
                let layout = self.workspace.dialog_layout(flow.dialog(), false);
                match layout.button_at(at) {
                    Some(_) => (SystemCursor::Hand, 0),
                    None => (SystemCursor::Arrow, 0),
                }
            }
            (Some(at), None) => self.workspace.cursor(at, self.code.as_ref()),
            (None, _) => (SystemCursor::Arrow, 0),
        };
        workspace::draw_cursor(framebuffer, rom.ui().cursor(cursor), frame, pointer);
    }

    /// Advances the system by one logical tick, applying the tick's `input`.
    ///
    /// Input is ignored during the boot sequence. In the shell,
    /// [`InputEvent::Break`] opens the workspace on the loaded cartridge,
    /// and in the workspace it returns to the shell. While a cartridge runs,
    /// Break pauses it and button events reach its `btn` and `btnp`.
    /// Anything else is ignored. On the pause screen, Break resumes the
    /// cartridge and Enter opens the shell with it still paused.
    ///
    /// While the system asks what to do with unsaved edits, its dialog
    /// takes every input instead, and nothing else advances.
    ///
    /// Copying and pasting in the workspace read and write `clipboard` as
    /// their input is handled.
    pub fn update_with(self, input: &[InputEvent], clipboard: &mut dyn Clipboard) -> Self {
        let Self {
            state,
            info,
            audio_source,
            mut sound,
            video_defaults,
            mut workspace,
            mut sprites,
            mut code,
            mut host_requests,
            mut unsaved,
            exit_requested,
            mut exiting,
        } = self;
        let mut ctx = Context {
            video_defaults: &video_defaults,
            workspace: &mut workspace,
            sprites: &mut sprites,
            code: &mut code,
            unsaved: &mut unsaved,
        };
        let mut state = state;
        if exit_requested {
            state = guard_exit(state, &mut ctx, &mut exiting);
        }
        let state = if ctx.unsaved.is_some() {
            unsaved_tick(state, input, &mut ctx, &mut exiting)
        } else {
            match state {
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
                SystemState::Shell { shell, loaded } => shell_tick(shell, loaded, input, &mut ctx),
                SystemState::Credits {
                    shell,
                    loaded,
                    view,
                } => credits_tick(shell, loaded, view, input),
                SystemState::Workspace {
                    shell,
                    cartridge,
                    video,
                } => {
                    let (state, requests) = workspace_tick(
                        shell,
                        cartridge,
                        video,
                        ctx.workspace,
                        Documents {
                            sprites: ctx.sprites,
                            code: ctx.code,
                        },
                        ctx.video_defaults,
                        input,
                        clipboard,
                    );
                    host_requests.extend(requests);
                    state
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
            }
        };
        sound.tick();
        Self {
            state,
            info,
            audio_source,
            sound,
            video_defaults,
            workspace,
            sprites,
            code,
            host_requests,
            unsaved,
            exit_requested: false,
            exiting,
        }
    }

    /// [`update_with`](Self::update_with) on a clipboard that starts
    /// empty and is dropped afterwards, for tests that don't copy or paste.
    #[cfg(test)]
    pub fn update(self, input: &[InputEvent]) -> Self {
        self.update_with(input, &mut clipboard::MemoryClipboard::default())
    }

    /// Takes the oldest work the system asked of the host and the host has
    /// not taken yet, if any. Call it after every
    /// [`update_with`](Self::update_with) until it returns `None`.
    pub fn take_host_request(&mut self) -> Option<HostRequest> {
        self.host_requests.pop_front()
    }

    /// Hands the system the answer to a [`HostRequest::PickPng`]. A chosen
    /// file is read and converted for import into the sprite document,
    /// which only changes once the user confirms. An answer nobody waits
    /// for any more, because the wait was stopped or the workspace closed,
    /// is ignored.
    pub fn file_picked(&mut self, picked: PickedFile) {
        let SystemState::Workspace { video, .. } = &mut self.state else {
            tracing::info!(
                ?picked,
                "file dialog answered outside the workspace, ignored"
            );
            return;
        };
        if !self.workspace.awaits_file() {
            tracing::info!(
                ?picked,
                "file dialog answered after the wait stopped, ignored"
            );
            return;
        }
        match picked {
            PickedFile::Cancelled => self.workspace.file_dialog_closed(),
            PickedFile::Unavailable(message) => self.workspace.file_dialog_failed(&message),
            PickedFile::Chosen(path) => {
                let Some(document) = self.sprites.as_ref().filter(|_| video.is_ready()) else {
                    self.workspace.file_dialog_closed();
                    return;
                };
                tracing::info!(path = %path.display(), "reading spritesheet to import");
                let name = path.file_name().map_or_else(
                    || path.display().to_string(),
                    |name| name.to_string_lossy().into_owned(),
                );
                let read = SheetSource::read(&path, None);
                self.workspace
                    .sheet_read(name, read, document, &self.video_defaults);
                self.workspace.refresh_video(video, document);
            }
        }
    }

    /// Writes the system's sound for the current state into `psg`.
    ///
    /// The boot sequence plays System ROM sound effects, the shell's
    /// diagnostic voices play while the shell is the active view, and a
    /// running cartridge plays its own audio. The pause screen and the
    /// workspace are silent.
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
    /// Call once after each [`update_with`](Self::update_with): a cartridge's
    /// envelope and gate writes reach the PSG once each.
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
            }
            | SystemState::Workspace { .. }
            | SystemState::Credits { .. } => AudioSource::Silent,
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
            }
            | SystemState::Workspace { .. }
            | SystemState::Credits { .. } => {}
        }
    }

    /// The master palette the frame drawn by [`render`](Self::render)
    /// resolves to: the cartridge's own while its frame is on screen, the
    /// System ROM palette with the cartridge's object colors after the UI
    /// colors in the workspace, and the System ROM palette otherwise.
    pub fn frame_palette<'a>(&'a self, rom: &'a SystemRom) -> &'a Palette {
        match &self.state {
            SystemState::Workspace { video, .. } => video.frame_palette().unwrap_or(rom.palette()),
            SystemState::Running { running, .. }
            | SystemState::Paused {
                running,
                view: PausedView::Game(_),
                ..
            } => running.palette(),
            SystemState::Boot { .. }
            | SystemState::Shell { .. }
            | SystemState::Credits { .. }
            | SystemState::Paused {
                view: PausedView::Shell,
                ..
            } => rom.palette(),
        }
    }

    /// Redraws the whole framebuffer for the current state using the ROM's
    /// font and UI images, with the question about unsaved edits on top
    /// while it is open. In the workspace, `pointer` is the logical pixel
    /// under the host's pointer, used to highlight what it hovers.
    pub fn render(&self, framebuffer: &mut Framebuffer, rom: &SystemRom, pointer: Option<Point>) {
        let Some(flow) = &self.unsaved else {
            self.render_state(framebuffer, rom, pointer);
            return;
        };
        // Nothing under the dialog reacts to the pointer.
        self.render_state(framebuffer, rom, None);
        let whole_screen = !matches!(self.state, SystemState::Workspace { .. });
        let layout = self.workspace.dialog_layout(flow.dialog(), whole_screen);
        flow.dialog().render(framebuffer, rom.font(), &layout);
    }

    fn render_state(&self, framebuffer: &mut Framebuffer, rom: &SystemRom, pointer: Option<Point>) {
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
            } => shell.lines(font, text_columns(cell_w), text_rows(cell_h)),
            SystemState::Workspace {
                cartridge, video, ..
            } => {
                self.workspace.render(
                    framebuffer,
                    font,
                    rom.ui(),
                    cartridge,
                    video
                        .source(self.sprites.as_ref())
                        .as_ref()
                        .map(|source| *source)
                        .map_err(String::as_str),
                    self.code.as_ref(),
                    pointer,
                );
                return;
            }
            SystemState::Credits { view, .. } => {
                view.render(framebuffer, rom);
                return;
            }
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
                let cells = i32::try_from(font.text_cells(&line.text)).unwrap_or(i32::MAX);
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
/// commands make. Break opens the workspace on the loaded cartridge, or
/// explains how to get one.
///
/// A started cartridge does not take input, so events that follow a
/// successful `run` in the same tick are dropped. So are events after a
/// Break that opens the workspace, so keys meant for the shell never act
/// on it, and events after a command that asks what to do with unsaved
/// edits, which the question takes instead.
///
/// Replacing or unloading the cartridge drops its documents, once the new
/// cartridge has loaded or the old one is unloaded. A `load` that fails
/// keeps them. While they hold unsaved edits, the shell asks first unless
/// told to discard them.
fn shell_tick(
    mut shell: Box<Shell>,
    loaded: Option<LoadedCartridge>,
    input: &[InputEvent],
    ctx: &mut Context<'_>,
) -> SystemState {
    // Ticking before the input lets this tick's input restart the blink.
    shell.tick();
    shell_input(shell, loaded, input, ctx)
}

/// Feeds `input` to the shell, as [`shell_tick`] does, without ticking it.
fn shell_input(
    mut shell: Box<Shell>,
    mut loaded: Option<LoadedCartridge>,
    input: &[InputEvent],
    ctx: &mut Context<'_>,
) -> SystemState {
    // Input is deliberately not logged: user input stays out of
    // application logs.
    for (index, &event) in input.iter().enumerate() {
        if event == InputEvent::Break {
            match loaded {
                Some(cartridge) => {
                    let video_defaults = ctx.video_defaults;
                    return open_workspace(
                        shell,
                        cartridge,
                        ctx.workspace,
                        Documents {
                            sprites: ctx.sprites,
                            code: ctx.code,
                        },
                        video_defaults,
                    );
                }
                None => {
                    shell.report(no_cartridge_hint());
                    continue;
                }
            }
        }
        let slot = CartridgeSlot::loaded(loaded.as_ref(), ctx.has_unsaved());
        let request = shell.handle_input(event, slot);
        match shell_request(shell, loaded, request, &input[index + 1..], ctx) {
            (
                SystemState::Shell {
                    shell: kept,
                    loaded: still,
                },
                true,
            ) => {
                (shell, loaded) = (kept, still);
            }
            (state, _) => return state,
        }
    }
    SystemState::Shell { shell, loaded }
}

/// Carries out `request`, a shell command's cartridge request. `rest` is
/// the input that followed it in this tick. Returns the state that
/// follows, and whether the shell still takes the rest of this tick's
/// input.
fn shell_request(
    mut shell: Box<Shell>,
    mut loaded: Option<LoadedCartridge>,
    request: Option<ShellRequest>,
    rest: &[InputEvent],
    ctx: &mut Context<'_>,
) -> (SystemState, bool) {
    match request {
        None => {}
        // The shell only asks to load a cartridge that already loaded.
        Some(ShellRequest::Load { cartridge, origin }) => {
            discard_documents(ctx.sprites, ctx.code, ctx.workspace);
            let cartridge = loaded.insert(*cartridge);
            shell.announce_loaded(cartridge.manifest(), origin);
        }
        Some(ShellRequest::Unload) => {
            let Some(cartridge) = loaded.take() else {
                unreachable!("UNLOAD was requested without a loaded cartridge");
            };
            discard_documents(ctx.sprites, ctx.code, ctx.workspace);
            let identity = cartridge_identity(cartridge.manifest());
            tracing::info!(cartridge = %identity, "cartridge unloaded");
            shell.report(ui_text::get_with(
                "system-unloaded",
                &[("identity", Arg::Text(&identity.to_string()))],
            ));
        }
        Some(ShellRequest::Run) => {
            // `run` only asks to start when a cartridge is loaded.
            let Some(cartridge) = loaded else {
                unreachable!("RUN was requested without a loaded cartridge");
            };
            let state = match start(shell, cartridge, ctx.video_defaults, ctx.sprites.as_ref()) {
                SystemState::Shell { shell, loaded } => shell_input(shell, loaded, rest, ctx),
                state => state,
            };
            return (state, false);
        }
        Some(ShellRequest::ConfirmUnsaved { action, line }) => {
            let pending = Pending::Command {
                action,
                line: line.clone(),
            };
            match UnsavedFlow::new(pending, ctx.code.as_ref(), ctx.sprites.as_ref()) {
                Some(flow) => {
                    tracing::info!("asking what to do with unsaved edits");
                    *ctx.unsaved = Some(flow);
                    return (SystemState::Shell { shell, loaded }, false);
                }
                // Nothing is unsaved after all: run it as confirmed.
                None => {
                    let slot = CartridgeSlot::loaded(loaded.as_ref(), false);
                    let request = shell.run_confirmed(&line, slot);
                    return shell_request(shell, loaded, request, rest, ctx);
                }
            }
        }
        Some(ShellRequest::Credits) => {
            tracing::info!("opening the credits");
            let view = Box::new(CreditsView::open());
            return (
                SystemState::Credits {
                    shell,
                    loaded,
                    view,
                },
                false,
            );
        }
        Some(_request @ (ShellRequest::Resume | ShellRequest::Stop)) => {
            unreachable!("{_request:?} was requested without a paused cartridge")
        }
    }
    (SystemState::Shell { shell, loaded }, true)
}

/// Asks what to do with unsaved edits before quitting, or confirms
/// quitting at once without any. A running cartridge is paused and the
/// question asked over the shell, so cancelling loses nothing. An
/// unfinished sprite stroke is ended first so its edit is saved whole.
fn guard_exit(state: SystemState, ctx: &mut Context<'_>, exiting: &mut bool) -> SystemState {
    if ctx.unsaved.is_some() {
        return state;
    }
    if matches!(state, SystemState::Workspace { .. }) {
        ctx.workspace.end_stroke(ctx.sprites.as_mut());
    }
    let Some(flow) = UnsavedFlow::new(Pending::Exit, ctx.code.as_ref(), ctx.sprites.as_ref())
    else {
        *exiting = true;
        return state;
    };
    *ctx.unsaved = Some(flow);
    let (shell, running) = match state {
        SystemState::Running { shell, running } => match pause(shell, running) {
            SystemState::Paused { shell, running, .. } => (shell, running),
            state => return state,
        },
        SystemState::Paused { shell, running, .. } => (shell, running),
        state => return state,
    };
    SystemState::Paused {
        shell,
        running,
        view: PausedView::Shell,
    }
}

/// Feeds `input` to the open question about unsaved edits, and once it is
/// answered carries out what waited on it. Events after the answer are
/// dropped, as after any change of screen.
fn unsaved_tick(
    mut state: SystemState,
    input: &[InputEvent],
    ctx: &mut Context<'_>,
    exiting: &mut bool,
) -> SystemState {
    for &event in input {
        let Some(flow) = ctx.unsaved.as_mut() else {
            break;
        };
        let whole_screen = !matches!(state, SystemState::Workspace { .. });
        let layout = ctx.workspace.dialog_layout(flow.dialog(), whole_screen);
        let Some(cartridge) = cartridge_of(&state) else {
            // Unsaved edits always belong to a cartridge the system holds.
            *ctx.unsaved = None;
            break;
        };
        let edits = Edits {
            cartridge,
            code: ctx.code.get_or_insert_default(),
            sprites: ctx.sprites.as_mut(),
            defaults: ctx.video_defaults,
        };
        match flow.handle_input(event, &layout, edits) {
            Outcome::Open => continue,
            Outcome::Stopped(message) => {
                tracing::info!("unsaved edits kept, nothing went ahead");
                if let Some(message) = message {
                    report(&mut state, ctx.workspace, message);
                }
            }
            Outcome::Proceed(Pending::Exit) => {
                tracing::info!("quitting");
                *exiting = true;
            }
            Outcome::Proceed(Pending::Command { line, .. }) => {
                *ctx.unsaved = None;
                state = run_confirmed(state, &line, ctx);
            }
        }
        *ctx.unsaved = None;
        break;
    }
    if let SystemState::Workspace { video, .. } = &mut state
        && let Some(document) = ctx.sprites.as_ref()
    {
        ctx.workspace.refresh_video(video, document);
    }
    state
}

/// The cartridge `state` holds, if any.
fn cartridge_of(state: &SystemState) -> Option<&LoadedCartridge> {
    match state {
        SystemState::Boot { .. } => None,
        SystemState::Shell { loaded, .. } | SystemState::Credits { loaded, .. } => loaded.as_ref(),
        SystemState::Workspace { cartridge, .. } => Some(cartridge),
        SystemState::Running { running, .. } | SystemState::Paused { running, .. } => {
            Some(running.cartridge())
        }
    }
}

/// Shows `message` where `state` shows notices: the shell's scrollback or
/// the workspace's status bar.
fn report(state: &mut SystemState, workspace: &mut Workspace, message: String) {
    match state {
        SystemState::Shell { shell, .. } | SystemState::Paused { shell, .. } => {
            shell.report(message);
        }
        SystemState::Workspace { .. } => workspace.report(message),
        SystemState::Boot { .. } | SystemState::Running { .. } | SystemState::Credits { .. } => {}
    }
}

/// Runs the shell command `line` once its unsaved edits were saved or
/// discarded.
fn run_confirmed(state: SystemState, line: &str, ctx: &mut Context<'_>) -> SystemState {
    let SystemState::Shell { mut shell, loaded } = state else {
        tracing::warn!("a confirmed shell command found no shell to run in");
        return state;
    };
    let slot = CartridgeSlot::loaded(loaded.as_ref(), ctx.has_unsaved());
    let request = shell.run_confirmed(line, slot);
    shell_request(shell, loaded, request, &[], ctx).0
}

/// The documents of the loaded cartridge, lent together.
struct Documents<'a> {
    sprites: &'a mut Option<SpriteDocument>,
    code: &'a mut Option<CodeSession>,
}

/// Drops the documents of a cartridge that was replaced or unloaded, and
/// everything the workspace showed of them.
fn discard_documents(
    sprites: &mut Option<SpriteDocument>,
    code: &mut Option<CodeSession>,
    workspace: &mut Workspace,
) {
    if sprites
        .take()
        .is_some_and(|document| document.is_modified())
    {
        tracing::info!("unsaved graphics changes discarded");
    }
    if let Some(session) = code.take() {
        if session.is_modified() {
            tracing::info!("unsaved code changes discarded");
        }
        tracing::debug!("code documents closed");
    }
    workspace.forget_code();
}

/// Makes the workspace the active UI on `cartridge`.
///
/// A document kept from an earlier visit is brought up to date with the
/// files without losing unsaved changes or, for assets whose files are
/// unchanged, their undo history. See [`SpriteDocument::reopen`]. Without
/// one, the video assets are read afresh.
///
/// The Code view's session is kept from earlier visits, or started, and the
/// entry script opens if the workspace opens on the Code view for the
/// first time.
fn open_workspace(
    shell: Box<Shell>,
    cartridge: LoadedCartridge,
    workspace: &mut Workspace,
    documents: Documents<'_>,
    video_defaults: &CartridgeVideoDefaults,
) -> SystemState {
    let Documents { sprites, code } = documents;
    let identity = cartridge_identity(cartridge.manifest());
    tracing::info!(cartridge = %identity, tab = ?workspace.tab(), "workspace opened");
    workspace.end_stroke(sprites.as_mut());
    let document = match sprites.take() {
        Some(mut document) => {
            workspace.reopened(document.reopen(&cartridge, video_defaults));
            Ok(document)
        }
        None => SpriteDocument::load(&cartridge, video_defaults)
            .map_err(|err| ui_errors::video::video_load(&err)),
    };
    let video = match document {
        Ok(document) => workspace.prepare_video(sprites.insert(document)),
        Err(message) => WorkspaceVideo::unavailable(message),
    };
    open_entry_once(workspace, code.get_or_insert_default(), &cartridge);
    SystemState::Workspace {
        shell,
        cartridge,
        video,
    }
}

/// Feeds `input` to the workspace and carries out the file work it asks
/// for, returning the work it asks of the host, if any. Break returns to
/// the shell with the cartridge still loaded, unless a dialog is open,
/// which takes it as Escape. Events after a Break that leaves are dropped,
/// so nothing meant for the workspace reaches the shell. Copies and pastes
/// read and write `clipboard` as they come.
#[expect(
    clippy::too_many_arguments,
    reason = "the workspace state is lent in parts the tick hands on separately"
)]
fn workspace_tick(
    shell: Box<Shell>,
    cartridge: LoadedCartridge,
    mut video: WorkspaceVideo,
    workspace: &mut Workspace,
    documents: Documents<'_>,
    video_defaults: &CartridgeVideoDefaults,
    input: &[InputEvent],
    clipboard: &mut dyn Clipboard,
) -> (SystemState, Vec<HostRequest>) {
    let Documents { sprites, code } = documents;
    let code = code.get_or_insert_default();
    workspace.set_entry_script(&cartridge.manifest().runtime.entry);
    workspace.tick(Some(code));
    let mut host_requests = Vec::new();
    for &event in input {
        if event == InputEvent::Break && !workspace.claims_break() {
            workspace.end_stroke(sprites.as_mut());
            tracing::info!("workspace closed");
            let state = SystemState::Shell {
                shell,
                loaded: Some(cartridge),
            };
            return (state, host_requests);
        }
        // The editors only change what they can show.
        let document = sprites.as_mut().filter(|_| video.is_ready());
        let request = workspace.handle_input(event, document, Some(code));
        open_entry_once(workspace, code, &cartridge);
        let Some(request) = request else {
            continue;
        };
        let request = match request {
            WorkspaceRequest::ListScripts => {
                workspace.scripts_listed(cartridge.script_files());
                continue;
            }
            WorkspaceRequest::OpenScript(path) => {
                let result = code.open(&cartridge, &path).map(drop);
                workspace.script_opened(code, &path, result);
                continue;
            }
            WorkspaceRequest::SaveEditorSettings(settings) => {
                host_requests.push(HostRequest::SaveEditorSettings(settings));
                continue;
            }
            WorkspaceRequest::SaveScript(id) => {
                workspace.script_saved(id, code.save(&cartridge, id));
                continue;
            }
            WorkspaceRequest::OverwriteScript { id, seen } => {
                let result = code.overwrite(&cartridge, id, seen.as_deref());
                workspace.script_saved(id, result);
                continue;
            }
            WorkspaceRequest::SaveAndCloseScript(id) => {
                let result = code.save(&cartridge, id);
                let written = result.is_ok();
                workspace.script_saved(id, result);
                if written {
                    code.close(id);
                    workspace.script_closed(code);
                }
                continue;
            }
            WorkspaceRequest::NewScript => {
                workspace.open_new_script(cartridge.manifest().runtime.language);
                continue;
            }
            WorkspaceRequest::CreateScript(path) => {
                let result = cartridge
                    .create_source(&path)
                    .map_err(|err| ui_errors::fs(&err))
                    .and_then(|()| {
                        code.open(&cartridge, &path)
                            .map(drop)
                            .map_err(|err| ui_errors::code::document(&err))
                    });
                workspace.script_created(code, &path, result);
                continue;
            }
            WorkspaceRequest::PrepareDelete(id) => {
                let Some(path) = code.document(id).map(|document| document.path().clone()) else {
                    continue;
                };
                if cartridge.is_entry(&path) {
                    workspace.script_deleted(
                        code,
                        &path,
                        Err(SourceEditError::Entry(path.clone())),
                    );
                } else if let Some(state) = code.disk_state(&cartridge, id) {
                    workspace.delete_prompt(code, id, state);
                }
                continue;
            }
            WorkspaceRequest::DeleteScript { id, seen } => {
                let Some(path) = code.document(id).map(|document| document.path().clone()) else {
                    continue;
                };
                let result = cartridge.delete_source(&path, seen.as_deref());
                if result.is_ok() {
                    code.close(id);
                }
                workspace.script_deleted(code, &path, result);
                continue;
            }
            WorkspaceRequest::RenameScript { id, to } => {
                let result = code.move_script(&cartridge, id, &to);
                workspace.script_renamed(code, &to, result);
                continue;
            }
            WorkspaceRequest::OpenExternal(url) => {
                host_requests.push(HostRequest::OpenExternal(url));
                continue;
            }
            WorkspaceRequest::Copy(text) => {
                clipboard.set_text(&text);
                continue;
            }
            WorkspaceRequest::Paste => {
                workspace.paste(code, clipboard.text());
                continue;
            }
            request => request,
        };
        let Some(document) = sprites.as_mut().filter(|_| video.is_ready()) else {
            continue;
        };
        match request {
            WorkspaceRequest::Save => workspace.saved(document.save(&cartridge)),
            WorkspaceRequest::Overwrite(seen) => {
                workspace.saved(document.overwrite(&cartridge, &seen));
            }
            WorkspaceRequest::Reload(assets) => {
                let result = document
                    .reload(&cartridge, &assets, video_defaults)
                    .map_err(|err| ui_errors::video::video_load(&err));
                workspace.reloaded(&assets, result);
            }
            WorkspaceRequest::PickSpritesheet => {
                host_requests.push(HostRequest::PickPng {
                    directory: cartridge.host_dir().to_path_buf(),
                });
            }
            WorkspaceRequest::ListScripts
            | WorkspaceRequest::OpenScript(_)
            | WorkspaceRequest::SaveEditorSettings(_)
            | WorkspaceRequest::SaveScript(_)
            | WorkspaceRequest::OverwriteScript { .. }
            | WorkspaceRequest::SaveAndCloseScript(_)
            | WorkspaceRequest::NewScript
            | WorkspaceRequest::CreateScript(_)
            | WorkspaceRequest::PrepareDelete(_)
            | WorkspaceRequest::DeleteScript { .. }
            | WorkspaceRequest::RenameScript { .. }
            | WorkspaceRequest::OpenExternal(_)
            | WorkspaceRequest::Copy(_)
            | WorkspaceRequest::Paste => {
                unreachable!("code requests are carried out above")
            }
        }
    }
    if let Some(document) = sprites.as_ref() {
        workspace.refresh_video(&mut video, document);
    }
    let state = SystemState::Workspace {
        shell,
        cartridge,
        video,
    };
    (state, host_requests)
}

/// Opens the cartridge's entry script in `code` the first time the Code
/// view shows, and never again. A session whose tabs were all closed stays
/// empty.
fn open_entry_once(workspace: &mut Workspace, code: &mut CodeSession, cartridge: &LoadedCartridge) {
    if workspace.tab() != Tab::Code || !code.take_entry_request() {
        return;
    }
    let entry = &cartridge.manifest().runtime.entry;
    let result = SourcePath::parse(entry)
        .map_err(|err| ui_errors::code::source_path(&err))
        .and_then(|path| {
            code.open(cartridge, &path)
                .map(drop)
                .map_err(|err| ui_errors::code::document(&err))
        });
    workspace.entry_opened(code, result);
}

/// Starts `cartridge`, reporting its start or failure and what it logged.
/// A cartridge that fails to start stays loaded in the shell. When
/// `sprites` holds unsaved changes, the cartridge runs with the changed
/// assets instead of their files.
fn start(
    mut shell: Box<Shell>,
    cartridge: LoadedCartridge,
    video_defaults: &CartridgeVideoDefaults,
    sprites: Option<&SpriteDocument>,
) -> SystemState {
    let identity = cartridge_identity(cartridge.manifest());
    let overrides = sprites.map(SpriteDocument::unsaved).unwrap_or_default();
    let edited = !overrides.is_empty();
    match RunningCartridge::start(cartridge, video_defaults, overrides) {
        Ok(mut running) => {
            tracing::info!(cartridge = %identity, edited, "cartridge started");
            if edited {
                shell.report(ui_text::get_with(
                    "system-running-with-unsaved-graphics-changes",
                    &[("identity", Arg::Text(&identity.to_string()))],
                ));
            } else {
                shell.report(ui_text::get_with(
                    "system-running",
                    &[("identity", Arg::Text(&identity.to_string()))],
                ));
            }
            shell.warn_if_experimental(running.cartridge().manifest().runtime.language);
            shell.log(running.take_output());
            SystemState::Running { shell, running }
        }
        Err(failed) => {
            let FailedStart { cartridge, error } = *failed;
            match error {
                StartError::Cartridge(err) => shell.report_error(ui_errors::cartridge(&err)),
                StartError::Video(err) => shell.report_error(ui_errors::video::video_asset(&err)),
                StartError::Script(failure) => {
                    tracing::info!(
                        cartridge = %identity,
                        error = %failure.error,
                        "cartridge failed to start"
                    );
                    shell.log(failure.output);
                    shell.report_error(ui_errors::runtime::runtime(&failure.error));
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
    shell.report(ui_text::get_with(
        "system-paused",
        &[("identity", Arg::Text(&identity.to_string()))],
    ));
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
                shell.report(paused_shell_hint());
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

/// Feeds `input` to the credits view, which closes on its own Escape and
/// leaves the shell as it was. Events after the close are dropped, as after
/// any change of screen.
fn credits_tick(
    shell: Box<Shell>,
    loaded: Option<LoadedCartridge>,
    mut view: Box<CreditsView>,
    input: &[InputEvent],
) -> SystemState {
    view.tick();
    if input.iter().any(|&event| view.handle_input(event)) {
        return SystemState::Shell { shell, loaded };
    }
    SystemState::Credits {
        shell,
        loaded,
        view,
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
            // `load`, `unload`, `new` and `run` refuse to replace a paused
            // cartridge before asking about unsaved edits.
            Some(
                _request @ (ShellRequest::Load { .. }
                | ShellRequest::Unload
                | ShellRequest::Run
                | ShellRequest::Credits
                | ShellRequest::ConfirmUnsaved { .. }),
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
/// press. It is first ticked on the next update.
fn resume(mut shell: Box<Shell>, mut running: RunningCartridge) -> SystemState {
    running.reset_input();
    let identity = cartridge_identity(running.cartridge().manifest());
    tracing::info!(cartridge = %identity, "cartridge resumed");
    shell.report(ui_text::get_with(
        "system-resumed",
        &[("identity", Arg::Text(&identity.to_string()))],
    ));
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
            shell.report(ui_text::get_with(
                "system-stopped",
                &[("identity", Arg::Text(&identity.to_string()))],
            ));
        }
        StopReason::Failed(err) => {
            tracing::info!(cartridge = %identity, error = %err, "cartridge failed");
            shell.report_error(ui_errors::runtime::runtime(&err));
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
mod tests;
