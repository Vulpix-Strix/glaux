//! System software: the boot sequence, the shell that follows it, the
//! editor workspace beside it, and the cartridge runtime the shell starts.
//!
//! The system advances one step per logical tick and redraws the whole
//! framebuffer from its current state on every frame.

mod audio;
mod boot;
mod code_cursor;
mod code_document;
mod code_selection;
mod code_session;
mod code_text;
mod editor_settings;
mod line_editor;
mod pause;
mod running;
mod sfx;
mod shell;
mod sprite_document;
#[cfg(test)]
mod test_font;
#[cfg(test)]
mod test_frames;
mod video;
mod workspace;

pub use boot::Boot;
use code_session::CodeSession;
pub use editor_settings::{EditorSettings, TAB_SIZES};
pub use shell::Shell;
use sprite_document::SpriteDocument;
pub use video::CartridgeVideoDefaults;
use workspace::{Tab, Workspace, WorkspaceRequest, WorkspaceVideo};

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

use std::collections::VecDeque;
use std::path::PathBuf;

use crate::cartridge::LoadedCartridge;
use crate::cartridge::source::SourcePath;
use crate::cartridge_fs::CartridgeStorage;
use crate::sprite_import::SheetSource;

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
const PAUSED_SHELL_HINT: &str = "Type 'resume' to continue or 'stop' to end.";

/// Shown when Break asks for the workspace with no cartridge loaded. Fits
/// one shell row.
const NO_CARTRIDGE_HINT: &str = "Nothing to edit. Try 'new <id>' or 'load <path>'.";

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

/// Work the system asks of the host, outside the machine: see
/// [`System::take_host_request`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostRequest {
    /// Show the host's file dialog to choose one PNG file, opening in
    /// `directory`. Answer with [`System::file_picked`].
    PickPng { directory: PathBuf },
    /// Keep these Code view settings for the next launch.
    SaveEditorSettings(EditorSettings),
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
    /// or paused: it is dropped whenever that cartridge is replaced or
    /// unloaded, which the shell refuses while it holds unsaved edits.
    sprites: Option<SpriteDocument>,
    /// The loaded cartridge's open scripts and the Code view's tabs over
    /// them. Like `sprites`, it always belongs to the loaded cartridge and
    /// is dropped only once another cartridge has loaded or this one is
    /// unloaded, so a failed `load` keeps it.
    code: Option<CodeSession>,
    /// Host work asked for and not yet taken by the host, oldest first.
    host_requests: VecDeque<HostRequest>,
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
        }
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
            SystemState::Workspace { .. } => InputMode::Workspace,
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
    /// per presented frame: the pointer then follows the host at its
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
        let (cursor, frame) = match pointer {
            Some(at) => self.workspace.cursor(at, self.code.as_ref()),
            None => (SystemCursor::Arrow, 0),
        };
        workspace::draw_cursor(framebuffer, rom.ui().cursor(cursor), frame, pointer);
    }

    /// Advances the system by one logical tick, applying the tick's `input`.
    ///
    /// Input is ignored during the boot sequence. In the shell,
    /// [`InputEvent::Break`] opens the workspace on the loaded cartridge,
    /// and in the workspace it returns to the shell. While a cartridge runs,
    /// Break pauses it and button events reach its `btn` and `btnp`;
    /// anything else is ignored. On the pause screen, Break resumes the
    /// cartridge and Enter opens the shell with it still paused.
    pub fn update(self, input: &[InputEvent]) -> Self {
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
            SystemState::Shell { shell, loaded } => shell_tick(
                shell,
                loaded,
                input,
                &video_defaults,
                &mut workspace,
                Documents {
                    sprites: &mut sprites,
                    code: &mut code,
                },
            ),
            SystemState::Workspace {
                shell,
                cartridge,
                video,
            } => {
                let (state, requests) = workspace_tick(
                    shell,
                    cartridge,
                    video,
                    &mut workspace,
                    Documents {
                        sprites: &mut sprites,
                        code: &mut code,
                    },
                    &video_defaults,
                    input,
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
        }
    }

    /// Takes the oldest work the system asked of the host and the host has
    /// not taken yet, if any. Call it after every [`update`](Self::update)
    /// until it returns `None`.
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
            }
            | SystemState::Workspace { .. } => AudioSource::Silent,
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
            | SystemState::Workspace { .. } => {}
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
            | SystemState::Paused {
                view: PausedView::Shell,
                ..
            } => rom.palette(),
        }
    }

    /// Redraws the whole framebuffer for the current state using the ROM's
    /// font and UI images. In the workspace, `pointer` is the logical pixel
    /// under the host's pointer, used to highlight what it hovers.
    pub fn render(&self, framebuffer: &mut Framebuffer, rom: &SystemRom, pointer: Option<Point>) {
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
                    video.source(self.sprites.as_ref()),
                    self.code.as_ref(),
                    pointer,
                );
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
/// on it.
///
/// Replacing or unloading the cartridge drops its `documents`, once the
/// new cartridge has loaded or the old one is unloaded; a `load` that
/// fails keeps them. The shell refuses to while they hold unsaved edits
/// unless told to discard them.
fn shell_tick(
    mut shell: Box<Shell>,
    mut loaded: Option<LoadedCartridge>,
    input: &[InputEvent],
    video_defaults: &CartridgeVideoDefaults,
    workspace: &mut Workspace,
    documents: Documents<'_>,
) -> SystemState {
    let Documents { sprites, code } = documents;
    // Ticking before the input lets this tick's input restart the blink.
    shell.tick();
    // Input is deliberately not logged: user input stays out of
    // application logs.
    for (index, &event) in input.iter().enumerate() {
        if event == InputEvent::Break {
            match loaded {
                Some(cartridge) => {
                    return open_workspace(
                        shell,
                        cartridge,
                        workspace,
                        Documents { sprites, code },
                        video_defaults,
                    );
                }
                None => {
                    shell.report(NO_CARTRIDGE_HINT);
                    continue;
                }
            }
        }
        let unsaved = sprites.as_ref().is_some_and(SpriteDocument::is_modified);
        match shell.handle_input(event, CartridgeSlot::loaded(loaded.as_ref(), unsaved)) {
            None => {}
            // The shell only asks to load a cartridge that already loaded.
            Some(ShellRequest::Load { cartridge, origin }) => {
                discard_documents(sprites, code, workspace);
                let cartridge = loaded.insert(*cartridge);
                shell.announce_loaded(cartridge.manifest(), origin);
            }
            Some(ShellRequest::Unload) => {
                let Some(cartridge) = loaded.take() else {
                    unreachable!("UNLOAD was requested without a loaded cartridge");
                };
                discard_documents(sprites, code, workspace);
                let identity = cartridge_identity(cartridge.manifest());
                tracing::info!(cartridge = %identity, "cartridge unloaded");
                shell.report(format!("Unloaded {identity}"));
            }
            Some(ShellRequest::Run) => {
                // `run` only asks to start when a cartridge is loaded.
                let Some(cartridge) = loaded else {
                    unreachable!("RUN was requested without a loaded cartridge");
                };
                return match start(shell, cartridge, video_defaults, sprites.as_ref()) {
                    SystemState::Shell { shell, loaded } => shell_tick(
                        shell,
                        loaded,
                        &input[index + 1..],
                        video_defaults,
                        workspace,
                        Documents { sprites, code },
                    ),
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
    if code.take().is_some() {
        tracing::debug!("code documents closed");
    }
    workspace.forget_code();
}

/// Makes the workspace the active UI on `cartridge`.
///
/// A document kept from an earlier visit is brought up to date with the
/// files without losing unsaved changes or, for assets whose files are
/// unchanged, their undo history; see [`SpriteDocument::reopen`]. Without
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
        None => SpriteDocument::load(&cartridge, video_defaults).map_err(|err| err.to_string()),
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
/// for, returning the work it asks of the host, if any. Break returns to the shell with the cartridge still loaded, unless
/// a dialog is open, which takes it as Escape. Events after a Break that
/// leaves are dropped, so nothing meant for the workspace reaches the
/// shell.
fn workspace_tick(
    shell: Box<Shell>,
    cartridge: LoadedCartridge,
    mut video: WorkspaceVideo,
    workspace: &mut Workspace,
    documents: Documents<'_>,
    video_defaults: &CartridgeVideoDefaults,
    input: &[InputEvent],
) -> (SystemState, Vec<HostRequest>) {
    let Documents { sprites, code } = documents;
    let code = code.get_or_insert_default();
    workspace.tick(Some(code));
    let mut host_requests = Vec::new();
    for &event in input {
        if event == InputEvent::Break && !workspace.has_modal() {
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
                    .map_err(|err| err.to_string());
                workspace.reloaded(&assets, result);
            }
            WorkspaceRequest::PickSpritesheet => {
                host_requests.push(HostRequest::PickPng {
                    directory: cartridge.host_dir().to_path_buf(),
                });
            }
            WorkspaceRequest::ListScripts
            | WorkspaceRequest::OpenScript(_)
            | WorkspaceRequest::SaveEditorSettings(_) => {
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
/// view shows, and never again: a session whose tabs were all closed stays
/// empty.
fn open_entry_once(workspace: &mut Workspace, code: &mut CodeSession, cartridge: &LoadedCartridge) {
    if workspace.tab() != Tab::Code || !code.take_entry_request() {
        return;
    }
    let entry = &cartridge.manifest().runtime.entry;
    let result = SourcePath::parse(entry)
        .map_err(|err| err.to_string())
        .and_then(|path| {
            code.open(cartridge, &path)
                .map(drop)
                .map_err(|err| err.to_string())
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
                shell.report(format!("Running {identity} with unsaved graphics changes"));
            } else {
                shell.report(format!("Running {identity}"));
            }
            shell.warn_if_experimental(running.cartridge().manifest().runtime.language);
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
mod tests;
