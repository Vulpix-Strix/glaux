//! System software, made of the boot sequence, the shell that follows it,
//! the editor workspace beside it, and the cartridge runtime the shell
//! starts.
//!
//! The system advances one step per logical tick and redraws the whole
//! framebuffer from its current state on every frame.

mod audio;
mod boot;
mod clipboard;
mod credits;
mod definition;
mod docs;
mod editing;
mod editor_settings;
mod external_url;
mod line_editor;
mod logs;
mod menu_state;
mod pause;
mod render;
mod running;
mod screen;
mod sfx;
mod shell;
mod sprite_document;
mod state;
mod syntax_theme;
#[cfg(test)]
mod test_font;
#[cfg(test)]
mod test_frames;
mod ui_errors;
mod unsaved;
mod video;
mod workspace;

use boot::Boot;
pub use clipboard::Clipboard;
use credits::CreditsView;
use editing::session::CodeSession;
#[cfg(test)]
pub use editor_settings::IndentStyle;
pub use editor_settings::{EditorSettings, INDENT_SIZES, TAB_SIZES};
pub use external_url::{ExternalUrl, ExternalUrlError};
use logs::{LogPanel, LogStore};
pub use menu_state::MenuState;
use shell::Shell;
use sprite_document::SpriteDocument;
pub use video::CartridgeVideoDefaults;
use workspace::{DocsStart, Workspace, WorkspaceVideo};

use glaux_i18n::Arg;
use glaux_input::{InputEvent, InputMode};
use glaux_machine::font::BitmapFont;
use glaux_machine::machine::{GENERATION, NAME};
use glaux_machine::palette::Palette;
use glaux_machine::psg::Psg;
use glaux_machine::system_rom::SystemSounds;
use glaux_script::RuntimeError;
use pause::PauseOverlay;
use running::RunningCartridge;
use semver::Version;
use sfx::SystemSound;
use state::Context;
use state::confirm::{guard_exit, unsaved_tick};
use state::debugging::{debug_from_workspace, debugging_tick};
use state::logs::{receive_diagnostics, route_logs};
use state::paused::{paused_game_tick, paused_shell_tick};
use state::running::running_tick;
use state::screens::{credits_tick, docs_tick};
use state::shell::{open_docs_apart, shell_audio, shell_tick};
use state::workspace::{RunKind, run_from_workspace, workspace_tick};
use unsaved::UnsavedFlow;

use std::collections::VecDeque;
use std::path::PathBuf;

use crate::cartridge::LoadedCartridge;
use crate::cartridge_fs::CartridgeStorage;
use crate::logging::Diagnostics;
use crate::sprite_import::SheetSource;
use crate::ui_text;

/// Where the system is in its own navigation. Each variant owns exactly the
/// state that phase needs, and transitions consume one variant to build the
/// next.
#[derive(Debug)]
pub(crate) enum SystemState {
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
    /// The documentation browser is the active UI, the one the workspace
    /// keeps, filling the screen. The shell keeps its state, taking no
    /// input, and Escape on the page returns to it.
    Docs {
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
    /// A debugged cartridge that stopped at a breakpoint or a step, and
    /// waits in the middle of a tick. The Code view is the active UI and
    /// shows where, but cannot edit the scripts. The shell keeps its state,
    /// taking no input.
    Debugging {
        shell: Box<Shell>,
        running: RunningCartridge,
        /// How the workspace shows the cartridge's colors.
        video: WorkspaceVideo,
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
pub(crate) enum PausedView {
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
    /// What the current or last run logged, shared with the shell.
    logs: LogStore,
    /// The overlay that shows `logs`.
    log_panel: LogPanel,
    /// Where the diagnostics meant for the log panel arrive from.
    diagnostics: Option<Diagnostics>,
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
            logs: LogStore::default(),
            log_panel: LogPanel::default(),
            diagnostics: None,
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

    /// Opens the documentation browser on its front page, as the `docs`
    /// command and the Docs button do, if [`menu_state`](Self::menu_state)
    /// offers it. Unlike F1, it does not look up the symbol under the
    /// caret.
    pub fn open_documentation(mut self) -> Self {
        if !self.menu_state().documentation {
            return self;
        }
        match self.state {
            SystemState::Shell { shell, loaded } => {
                self.state = open_docs_apart(&mut self.workspace, shell, loaded, DocsStart::Home);
            }
            state => {
                self.state = state;
                if self.workspace.show_docs(self.sprites.as_mut()) {
                    tracing::info!("opening the documentation");
                }
            }
        }
        self
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

    /// Shows the diagnostics that arrive from `diagnostics` in the log panel
    /// from now on.
    pub fn with_diagnostics(mut self, diagnostics: Diagnostics) -> Self {
        self.diagnostics = Some(diagnostics);
        self
    }

    /// Shows code in the workspace with `settings`, the ones kept from an
    /// earlier launch.
    pub fn with_editor_settings(mut self, settings: EditorSettings) -> Self {
        self.workspace.set_editor_settings(settings);
        self
    }

    #[cfg(test)]
    pub(crate) fn state(&self) -> &SystemState {
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
            SystemState::Workspace { .. }
            | SystemState::Debugging { .. }
            | SystemState::Credits { .. }
            | SystemState::Docs { .. } => InputMode::Workspace,
            SystemState::Boot { .. } | SystemState::Shell { .. } | SystemState::Paused { .. } => {
                InputMode::Text
            }
        }
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
    #[hotpath::measure(label = "system::update", impl_type = "System")]
    pub fn update_with(mut self, input: &[InputEvent], clipboard: &mut dyn Clipboard) -> Self {
        // `Self` has no `Drop`, so the state can move out of `self` and the
        // other fields be lent to `ctx` while it is away.
        let mut state = self.state;
        let mut ctx = Context {
            video_defaults: &self.video_defaults,
            workspace: &mut self.workspace,
            sprites: &mut self.sprites,
            code: &mut self.code,
            logs: &mut self.logs,
            log_panel: &mut self.log_panel,
            unsaved: &mut self.unsaved,
        };
        if let Some(diagnostics) = &self.diagnostics {
            receive_diagnostics(diagnostics, ctx.logs);
        }
        if self.exit_requested {
            state = guard_exit(state, &mut ctx, &mut self.exiting);
        }
        let state = if ctx.unsaved.is_some() {
            unsaved_tick(state, input, &mut ctx, &mut self.exiting)
        } else {
            match state {
                SystemState::Boot {
                    mut boot,
                    cartridges,
                } => {
                    if let Some(sfx) = boot.tick() {
                        self.sound.play(sfx);
                    }
                    if boot.is_finished() {
                        tracing::info!("boot finished, entering shell");
                        self.sound.stop();
                        SystemState::Shell {
                            shell: Box::new(Shell::new(self.info.clone(), cartridges)),
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
                SystemState::Docs { shell, loaded } => docs_tick(
                    shell,
                    loaded,
                    ctx.workspace,
                    input,
                    clipboard,
                    &mut self.host_requests,
                ),
                SystemState::Workspace {
                    shell,
                    cartridge,
                    video,
                } => {
                    let routed = route_logs(&mut ctx, input, true);
                    let (state, requests, run) = workspace_tick(
                        shell,
                        cartridge,
                        video,
                        &mut ctx,
                        &routed.forward,
                        clipboard,
                    );
                    self.host_requests.extend(requests);
                    if let (Some(location), SystemState::Workspace { cartridge, .. }) =
                        (&routed.jump, &state)
                    {
                        ctx.log_panel.close();
                        let code = ctx.code.get_or_insert_default();
                        ctx.workspace.show_location(code, cartridge, location);
                    }
                    match run {
                        Some(RunKind::Run) => run_from_workspace(state, &mut ctx),
                        Some(RunKind::Debug) => debug_from_workspace(state, &mut ctx),
                        None => state,
                    }
                }
                SystemState::Debugging {
                    shell,
                    running,
                    video,
                } => {
                    let routed = route_logs(&mut ctx, input, true);
                    let (state, requests) =
                        debugging_tick(shell, running, video, &mut ctx, &routed.forward, clipboard);
                    self.host_requests.extend(requests);
                    if let (Some(location), SystemState::Debugging { running, .. }) =
                        (&routed.jump, &state)
                    {
                        ctx.log_panel.close();
                        let code = ctx.code.get_or_insert_default();
                        ctx.workspace
                            .show_location(code, running.cartridge(), location);
                    }
                    state
                }
                SystemState::Running { shell, mut running } => {
                    let routed = route_logs(&mut ctx, input, false);
                    if routed.toggled {
                        // Nothing held while the panel took the keys may
                        // reach the cartridge as a press.
                        running.reset_input();
                    }
                    running_tick(shell, running, &routed.forward, &mut ctx)
                }
                SystemState::Paused {
                    shell,
                    running,
                    view: PausedView::Game(overlay),
                } => paused_game_tick(shell, running, overlay, input, &mut ctx),
                SystemState::Paused {
                    shell,
                    running,
                    view: PausedView::Shell,
                } => paused_shell_tick(shell, running, input, &mut ctx),
            }
        };
        self.sound.tick();
        self.log_panel.sync(&self.logs);
        self.state = state;
        self.exit_requested = false;
        self
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
            | SystemState::Debugging { .. }
            | SystemState::Credits { .. }
            | SystemState::Docs { .. } => AudioSource::Silent,
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
            | SystemState::Debugging { .. }
            | SystemState::Credits { .. }
            | SystemState::Docs { .. } => {}
        }
    }
}

#[cfg(test)]
mod tests;
